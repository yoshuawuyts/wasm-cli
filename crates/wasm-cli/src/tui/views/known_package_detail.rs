use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use wasm_package_manager::storage::KnownPackageView;

/// View for displaying details of a known package (from search results).
#[derive(Debug)]
#[allow(dead_code)]
pub struct KnownPackageDetailView<'a> {
    package: &'a KnownPackageView,
}

impl<'a> KnownPackageDetailView<'a> {
    /// Creates a new known package detail view.
    #[must_use]
    #[allow(dead_code)]
    pub fn new(package: &'a KnownPackageView) -> Self {
        Self { package }
    }
}

impl Widget for KnownPackageDetailView<'_> {
    #[allow(clippy::indexing_slicing)]
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Split area into content and shortcuts bar
        let main_layout = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
        let content_area = main_layout[0];
        let shortcuts_area = main_layout[1];

        let layout = Layout::vertical([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Details
        ])
        .split(content_area);

        // Header with package name
        let header_text = format!("{}/{}", self.package.registry, self.package.repository);
        Paragraph::new(header_text)
            .style(Style::default().bold().fg(Color::Yellow))
            .block(Block::default().borders(Borders::BOTTOM))
            .render(layout[0], buf);

        // Build details text
        let mut details = Vec::new();

        details.push(Line::from(vec![
            Span::styled("Registry: ", Style::default().bold()),
            Span::raw(&self.package.registry),
        ]));

        details.push(Line::from(vec![
            Span::styled("Repository: ", Style::default().bold()),
            Span::raw(&self.package.repository),
        ]));

        if let Some(ref description) = self.package.description {
            details.push(Line::from(vec![
                Span::styled("Description: ", Style::default().bold()),
                Span::raw(description),
            ]));
        }

        details.push(Line::raw("")); // Empty line

        // Tags info
        details.push(Line::from(vec![
            Span::styled("Tags: ", Style::default().bold()),
            Span::raw(format!("{} tag(s)", self.package.tags.len())),
        ]));

        if !self.package.tags.is_empty() {
            for tag in &self.package.tags {
                details.push(Line::from(vec![
                    Span::styled("  • ", Style::default().dim()),
                    Span::raw(tag),
                ]));
            }
        }

        if !self.package.signature_tags.is_empty() {
            details.push(Line::raw("")); // Empty line
            details.push(Line::from(vec![
                Span::styled("Signature Tags: ", Style::default().bold()),
                Span::raw(format!("{} tag(s)", self.package.signature_tags.len())),
            ]));
            for tag in &self.package.signature_tags {
                details.push(Line::from(vec![
                    Span::styled("  • ", Style::default().dim()),
                    Span::raw(tag),
                ]));
            }
        }

        if !self.package.attestation_tags.is_empty() {
            details.push(Line::raw("")); // Empty line
            details.push(Line::from(vec![
                Span::styled("Attestation Tags: ", Style::default().bold()),
                Span::raw(format!("{} tag(s)", self.package.attestation_tags.len())),
            ]));
            for tag in &self.package.attestation_tags {
                details.push(Line::from(vec![
                    Span::styled("  • ", Style::default().dim()),
                    Span::raw(tag),
                ]));
            }
        }

        details.push(Line::raw("")); // Empty line

        // Timestamps
        details.push(Line::from(vec![
            Span::styled("Last Seen: ", Style::default().bold()),
            Span::raw(&self.package.last_seen_at),
        ]));

        details.push(Line::from(vec![
            Span::styled("Created At: ", Style::default().bold()),
            Span::raw(&self.package.created_at),
        ]));

        Paragraph::new(details)
            .wrap(Wrap { trim: false })
            .render(layout[1], buf);

        // Render shortcuts bar
        let shortcuts = Line::from(vec![
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Back "),
        ]);
        Paragraph::new(shortcuts)
            .style(Style::default().fg(Color::DarkGray))
            .render(shortcuts_area, buf);
    }
}
