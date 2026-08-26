//! Runtime data store: holds the current [`SiteData`] snapshot and pulls fresh
//! copies from the website over HTTP.
//!
//! Reads are lock-free-ish (an `RwLock` around an `Arc`) and cheap, so the UI
//! can grab a [`snapshot`] on every frame. Pulls are rate-limited to one per
//! [`MIN_REFRESH_INTERVAL_SECS`] so a burst of SSH connections can't hammer
//! the site.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;

use super::fallback::fallback;
use super::model::{AdjectivesDoc, Job, Project, Quote, SiteData, SocialsDoc};

/// Minimum seconds between pulls. Connections inside this window reuse the
/// cached snapshot instead of fetching again.
const MIN_REFRESH_INTERVAL_SECS: u64 = 60;

/// Default base URL for the JSON data, overridable via `SITE_DATA_URL`.
///
/// Served from jsDelivr (the same `public/data/*.json` files committed to the
/// website repo) rather than `seanyang.ca` directly, since the site sits behind
/// Vercel's bot challenge which blocks non-browser clients.
const DEFAULT_BASE_URL: &str = "https://cdn.jsdelivr.net/gh/aicheye/seanyang.ca@main/public/data";

/// Per-request timeout for data fetches.
const FETCH_TIMEOUT_SECS: u64 = 10;

struct Store {
    data: RwLock<Arc<SiteData>>,
    /// Unix seconds of the last pull attempt (0 = never attempted).
    last_fetch: AtomicU64,
    /// True while a pull is in flight; prevents concurrent fetches.
    fetching: AtomicBool,
}

static STORE: OnceLock<Store> = OnceLock::new();

fn store() -> &'static Store {
    STORE.get_or_init(|| Store {
        data: RwLock::new(Arc::new(fallback())),
        last_fetch: AtomicU64::new(0),
        fetching: AtomicBool::new(false),
    })
}

/// A consistent snapshot of the current site data. Cheap to clone (`Arc`),
/// safe to call on every render.
pub fn snapshot() -> Arc<SiteData> {
    store().data.read().expect("data lock poisoned").clone()
}

fn base_url() -> String {
    std::env::var("SITE_DATA_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Perform the initial, awaited data pull at startup. On failure the bundled
/// fallback (already installed) is kept, so the TUI is always usable.
pub async fn init() {
    let s = store();
    s.last_fetch.store(now_secs(), Ordering::Relaxed);
    match fetch_all(base_url()).await {
        Ok(data) => {
            *s.data.write().expect("data lock poisoned") = Arc::new(data);
            tracing::info!("loaded site data from {}", base_url());
        }
        Err(e) => {
            tracing::warn!("initial site data load failed, using bundled fallback: {e:#}");
        }
    }
}

/// Trigger a background refresh if at least [`MIN_REFRESH_INTERVAL_SECS`] have
/// elapsed since the last attempt.
///
/// Safe to call from synchronous code running inside the Tokio runtime (e.g.
/// on every SSH connection): it never blocks and spawns its own task. If a
/// pull is already running or the window hasn't elapsed, it's a cheap no-op.
pub fn refresh_if_stale() {
    let s = store();
    let now = now_secs();
    let last = s.last_fetch.load(Ordering::Relaxed);
    if last != 0 && now.saturating_sub(last) < MIN_REFRESH_INTERVAL_SECS {
        return;
    }

    // Claim the fetch slot; bail out if another pull is already running.
    if s.fetching
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    // Mark the attempt up front so failures still respect the rate limit.
    s.last_fetch.store(now, Ordering::Relaxed);

    tokio::spawn(async move {
        match fetch_all(base_url()).await {
            Ok(data) => {
                *store().data.write().expect("data lock poisoned") = Arc::new(data);
                tracing::info!("refreshed site data");
            }
            Err(e) => tracing::warn!("site data refresh failed: {e:#}"),
        }
        store().fetching.store(false, Ordering::Release);
    });
}

/// Fetch and assemble a full [`SiteData`] from the individual JSON files,
/// concurrently.
async fn fetch_all(base: String) -> anyhow::Result<SiteData> {
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .build()?;
    let base = base.trim_end_matches('/').to_string();

    let (adjectives, jobs, projects, quotes, socials) = tokio::try_join!(
        fetch_json::<AdjectivesDoc>(&client, &base, "adjectives.json"),
        fetch_json::<Vec<Job>>(&client, &base, "jobs.json"),
        fetch_json::<Vec<Project>>(&client, &base, "projects.json"),
        fetch_json::<Vec<Quote>>(&client, &base, "quotes.json"),
        fetch_json::<SocialsDoc>(&client, &base, "socials.json"),
    )?;

    Ok(SiteData {
        adjectives: adjectives.adjectives,
        jobs,
        projects,
        quotes,
        socials: socials.socials,
        primary_email: socials.primary_email,
        location: adjectives.location,
    })
}

async fn fetch_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    base: &str,
    path: &str,
) -> anyhow::Result<T> {
    let url = format!("{base}/{path}");
    let bytes = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("failed to parse {url}: {e}"))
}
