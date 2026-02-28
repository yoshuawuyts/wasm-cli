use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use wasm_package_manager::oci::ImageView;

/// View for displaying details of an installed package
#[derive(Debug)]
pub struct PackageDetailView<'a> {
    package: &'a ImageView,
}

impl<'a> PackageDetailView<'a> {
    /// Creates a new package detail view
    #[must_use]
    pub fn new(package: &'a ImageView) -> Self {
        Self { package }
    }
}

impl Widget for PackageDetailView<'_> {
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
        let header_text = format!(
            "{}/{}",
            self.package.ref_registry, self.package.ref_repository
        );
        Paragraph::new(header_text)
            .style(Style::default().bold().fg(Color::Yellow))
            .block(Block::default().borders(Borders::BOTTOM))
            .render(layout[0], buf);

        // Build details text
        let mut details = Vec::new();

        details.push(Line::from(vec![
            Span::styled("Registry: ", Style::default().bold()),
            Span::raw(&self.package.ref_registry),
        ]));

        details.push(Line::from(vec![
            Span::styled("Repository: ", Style::default().bold()),
            Span::raw(&self.package.ref_repository),
        ]));

        if let Some(ref mirror) = self.package.ref_mirror_registry {
            details.push(Line::from(vec![
                Span::styled("Mirror Registry: ", Style::default().bold()),
                Span::raw(mirror),
            ]));
        }

        if let Some(ref tag) = self.package.ref_tag {
            details.push(Line::from(vec![
                Span::styled("Tag: ", Style::default().bold()),
                Span::raw(tag),
            ]));
        }

        if let Some(ref digest) = self.package.ref_digest {
            details.push(Line::from(vec![
                Span::styled("Digest: ", Style::default().bold()),
                Span::raw(digest),
            ]));
        }

        details.push(Line::from(vec![
            Span::styled("Size: ", Style::default().bold()),
            Span::raw(super::format_size(self.package.size_on_disk)),
        ]));

        details.push(Line::raw("")); // Empty line

        // Manifest info
        details.push(Line::from(vec![Span::styled(
            "Manifest:",
            Style::default().bold().underlined(),
        )]));

        details.push(Line::from(vec![
            Span::styled("  Schema Version: ", Style::default().bold()),
            Span::raw(self.package.manifest.schema_version.to_string()),
        ]));

        if let Some(ref media_type) = self.package.manifest.media_type {
            details.push(Line::from(vec![
                Span::styled("  Media Type: ", Style::default().bold()),
                Span::raw(media_type),
            ]));
        }

        details.push(Line::from(vec![
            Span::styled("  Config Media Type: ", Style::default().bold()),
            Span::raw(&self.package.manifest.config.media_type),
        ]));
        details.push(Line::from(vec![
            Span::styled("  Config Size: ", Style::default().bold()),
            Span::raw(super::format_size(self.package.manifest.config.size as u64)),
        ]));

        details.push(Line::raw("")); // Empty line

        // Layers info
        let layer_count = self.package.manifest.layers.len();
        details.push(Line::from(vec![
            Span::styled("Layers: ", Style::default().bold()),
            Span::raw(format!("{} layer(s)", layer_count)),
        ]));

        for (i, layer) in self.package.manifest.layers.iter().enumerate() {
            let size_str = super::format_size(layer.size as u64);
            details.push(Line::from(vec![
                Span::styled(format!("  [{}] ", i + 1), Style::default().dim()),
                Span::raw(&layer.media_type),
                Span::styled(format!(" ({})", size_str), Style::default().dim()),
            ]));
        }

        details.push(Line::raw("")); // Empty line

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
