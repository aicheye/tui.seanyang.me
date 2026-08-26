//! Compile-time-bundled site content, used until the first successful HTTP
//! pull and whenever the website is unreachable. This keeps the TUI looking
//! identical to the website even when offline.

use serde::Deserialize;

use super::model::{Job, PrimaryEmail, Project, Quote, SiteData, Social};

/// Combined shape of `fallback.json` (mirrors the separately-served files).
#[derive(Deserialize)]
struct FallbackDoc {
    location: String,
    adjectives: Vec<String>,
    jobs: Vec<Job>,
    projects: Vec<Project>,
    quotes: Vec<Quote>,
    #[serde(rename = "primaryEmail")]
    primary_email: PrimaryEmail,
    socials: Vec<Social>,
}

const RAW: &str = include_str!("fallback.json");

/// The bundled fallback snapshot. The JSON is validated by `cargo test`
/// (see the test below), so this parse never fails in a shipped binary.
pub fn fallback() -> SiteData {
    let doc: FallbackDoc = serde_json::from_str(RAW).expect("bundled fallback.json must be valid");
    SiteData {
        adjectives: doc.adjectives,
        jobs: doc.jobs,
        projects: doc.projects,
        quotes: doc.quotes,
        socials: doc.socials,
        primary_email: doc.primary_email,
        location: doc.location,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_parses() {
        let data = fallback();
        assert!(!data.adjectives.is_empty());
        assert!(!data.jobs.is_empty());
        assert!(!data.projects.is_empty());
        assert!(!data.quotes.is_empty());
        assert!(!data.socials.is_empty());
        assert!(!data.primary_email.label.is_empty());
        assert!(!data.location.is_empty());
    }
}
