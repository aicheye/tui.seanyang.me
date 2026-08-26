use rand::seq::SliceRandom;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::theme;
use crate::{
    data::{SiteData, snapshot},
    input::Key,
    section::SectionView,
};

/// ASCII art wordmark — rendered at the top of the home screen.
const WORDMARK: &str = r#"┏━┓┏━╸┏━┓┏┓╻   ╻ ╻┏━┓┏┓╻┏━╸
┗━┓┣╸ ┣━┫┃┗┫   ┗┳┛┣━┫┃┗┫┃╺┓
┗━┛┗━╸╹ ╹╹ ╹    ╹ ╹ ╹╹ ╹┗━┛"#;

// University progress bar constants
// UWaterloo BSE: 2025-09-01 → 2030-05-01
const UNI_START: u64 = 1_756_684_800; // 2025-09-01 UTC
const UNI_END: u64 = 1_903_824_000; // 2030-05-01 UTC
const UNI_TOTAL: f64 = (UNI_END - UNI_START) as f64;

// Term boundary timestamps (UTC) — tick marks on the progress bar.
// Each entry is where one academic term ends and the next begins.
const TERM_MARKS: &[u64] = &[
    1_767_225_600, // 2026-01-01
    1_777_593_600, // 2026-05-01
    1_788_220_800, // 2026-09-01
    1_798_761_600, // 2027-01-01
    1_809_129_600, // 2027-05-01
    1_819_756_800, // 2027-09-01
    1_830_297_600, // 2028-01-01
    1_840_752_000, // 2028-05-01
    1_851_379_200, // 2028-09-01
    1_861_920_000, // 2029-01-01
    1_872_288_000, // 2029-05-01
    1_882_915_200, // 2029-09-01
    1_893_456_000, // 2030-01-01
];

pub struct HomeSection {
    /// Shuffled indices into the current quote list.
    order: Vec<usize>,
    cursor: usize,
}

impl HomeSection {
    pub fn new() -> Self {
        let mut section = Self {
            order: Vec::new(),
            cursor: 0,
        };
        section.reshuffle(snapshot().quotes.len());
        section
    }

    /// Rebuild the shuffled order for `n` quotes and reset the cursor.
    fn reshuffle(&mut self, n: usize) {
        let mut order: Vec<usize> = (0..n).collect();
        order.shuffle(&mut rand::thread_rng());
        self.order = order;
        self.cursor = 0;
    }
}

impl SectionView for HomeSection {
    fn label(&self) -> &'static str {
        "Home"
    }

    fn handle_key(&mut self, key: Key) {
        if self.order.is_empty() {
            return;
        }
        match key {
            Key::Right | Key::Char('l') | Key::Char('n') => {
                self.cursor = (self.cursor + 1) % self.order.len();
            }
            Key::Left | Key::Char('h') | Key::Char('p') => {
                self.cursor = self.cursor.checked_sub(1).unwrap_or(self.order.len() - 1);
            }
            _ => {}
        }
    }

    fn update(&mut self) {
        // Re-sync the shuffle order if the quote list changed after a refresh.
        let n = snapshot().quotes.len();
        if self.order.len() != n {
            self.reshuffle(n);
        }
    }

    fn render(&self, f: &mut Frame, area: Rect) {
        let data = snapshot();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),                               // top margin
                Constraint::Length(WORDMARK.lines().count() as u16), // wordmark
                Constraint::Length(1),                               // location
                Constraint::Length(1),                               // spacer
                Constraint::Length(1),                               // tagline
                Constraint::Length(1),                               // spacer
                Constraint::Length(2),                               // progress bar
                Constraint::Min(2),                                  // gap
                Constraint::Length(6),                               // quote
            ])
            .split(area);

        render_wordmark(f, chunks[1]);
        render_location(f, chunks[2], &data.primary_email.label, &data.location);
        render_tagline(f, chunks[4], &data.adjectives);
        render_progress(f, chunks[6]);
        render_quote(f, self, &data, chunks[8]);
    }
}

fn render_wordmark(f: &mut Frame, area: Rect) {
    let p = Paragraph::new(WORDMARK)
        .style(theme::green_bold())
        .alignment(Alignment::Center);
    f.render_widget(p, area);
}

fn render_location(f: &mut Frame, area: Rect, email: &str, location: &str) {
    let right = format!("⦿ {location}");
    let pad = (area.width as usize).saturating_sub(email.chars().count() + right.chars().count());
    let line = Line::from(vec![
        Span::styled(email.to_string(), theme::secondary()),
        Span::raw(" ".repeat(pad)),
        Span::styled("⦿ ", theme::primary()),
        Span::styled(location.to_string(), theme::secondary()),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_tagline(f: &mut Frame, area: Rect, adjectives: &[String]) {
    let mut spans: Vec<Span<'static>> = vec![Span::styled("[  ", theme::green())];
    for (i, adj) in adjectives.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", theme::green()));
        }
        spans.push(Span::styled(adj.clone(), theme::body()));
    }
    spans.push(Span::styled("  ]", theme::green()));

    let p = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);
    f.render_widget(p, area);
}

fn render_progress(f: &mut Frame, area: Rect) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(UNI_START);

    let elapsed = now.saturating_sub(UNI_START) as f64;
    let pct = (elapsed / UNI_TOTAL * 100.0).clamp(0.0, 100.0);
    let fill_frac = pct / 100.0;

    let bar_w = area.width as usize;
    let filled = (fill_frac * bar_w as f64) as usize;

    // Compute tick column positions from timestamps
    let tick_cols: Vec<usize> = TERM_MARKS
        .iter()
        .map(|&ts| {
            let frac = ts.saturating_sub(UNI_START) as f64 / UNI_TOTAL;
            (frac * bar_w as f64).round() as usize
        })
        .filter(|&c| c < bar_w)
        .collect();

    // Build bar as grouped spans: runs of █, ░, and │ tick marks
    let mut bar_spans: Vec<Span<'static>> = Vec::new();
    let mut i = 0usize;
    while i < bar_w {
        if tick_cols.contains(&i) {
            let style = if i < filled {
                theme::body()
            } else {
                theme::secondary()
            };
            bar_spans.push(Span::styled("│", style));
            i += 1;
        } else {
            let next_tick = tick_cols.iter().find(|&&c| c > i).copied().unwrap_or(bar_w);
            let region_end = next_tick.min(bar_w);
            let fill_end = filled.min(region_end);
            if i < fill_end {
                bar_spans.push(Span::styled(
                    "█".repeat(fill_end - i),
                    Style::default().fg(theme::GREEN),
                ));
                i = fill_end;
            }
            if i < region_end {
                bar_spans.push(Span::styled("░".repeat(region_end - i), theme::secondary()));
                i = region_end;
            }
        }
    }

    let left = "uwaterloo bse '30";
    let right = format!("{:.8}%", pct);
    let pad = (area.width as usize).saturating_sub(left.len() + right.len());
    let label = Line::from(vec![
        Span::styled(left, theme::secondary()),
        Span::raw(" ".repeat(pad)),
        Span::styled(right, theme::secondary()),
    ]);
    let bar = Line::from(bar_spans);

    f.render_widget(Paragraph::new(vec![label, bar]), area);
}

fn render_quote(f: &mut Frame, section: &HomeSection, data: &SiteData, area: Rect) {
    if data.quotes.is_empty() {
        return;
    }
    let idx = section.order.get(section.cursor).copied().unwrap_or(0) % data.quotes.len();
    let quote = &data.quotes[idx];
    let total = section.order.len().max(1);
    let index_label = format!("{} / {}", section.cursor + 1, total);

    // Manual word-wrap so every visual line gets its own │ prefix.
    let prefix_w = 3usize; // "│  "
    let content_w = (area.width as usize).saturating_sub(prefix_w);
    let full_text = format!("\"{}\"", quote.text);
    let wrapped = word_wrap(&full_text, content_w);

    let quote_style = Style::default()
        .fg(theme::HI)
        .add_modifier(Modifier::ITALIC);
    let mut tui_lines: Vec<Line> = wrapped
        .into_iter()
        .map(|chunk| {
            Line::from(vec![
                Span::styled("│", theme::secondary()),
                Span::raw("  "),
                Span::styled(chunk, quote_style),
            ])
        })
        .collect();

    tui_lines.push(Line::from(Span::styled("│", theme::secondary())));
    tui_lines.push(Line::from(vec![
        Span::styled("│", theme::secondary()),
        Span::raw("  "),
        Span::styled("— ", theme::secondary()),
        Span::styled(quote.author.clone(), theme::green_bold()),
        Span::styled(format!("  ({index_label})  "), theme::secondary()),
        Span::styled("←/→", theme::primary()),
        Span::styled(" prev/next", theme::secondary()),
    ]));

    f.render_widget(Paragraph::new(tui_lines), area);
}

/// Word-wrap `text` so no line exceeds `max_width` display columns.
/// Uses char count as a proxy for display width (sufficient for this content).
fn word_wrap(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_w = 0usize;
    for word in text.split_whitespace() {
        let word_w = word.chars().count();
        if current.is_empty() {
            current.push_str(word);
            current_w = word_w;
        } else if current_w + 1 + word_w <= max_width {
            current.push(' ');
            current.push_str(word);
            current_w += 1 + word_w;
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_w = word_w;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
