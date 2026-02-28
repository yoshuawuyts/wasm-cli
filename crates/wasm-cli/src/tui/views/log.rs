use ratatui::{
    prelude::*,
    widgets::{Paragraph, Widget},
};

/// View for displaying the application log file
#[derive(Debug)]
pub struct LogView<'a> {
    lines: &'a [String],
    scroll_offset: usize,
}

impl<'a> LogView<'a> {
    /// Create a new LogView with the given log lines and scroll offset
    #[must_use]
    pub fn new(lines: &'a [String], scroll_offset: usize) -> Self {
        Self {
            lines,
            scroll_offset,
        }
    }
}

impl Widget for LogView<'_> {
    #[allow(clippy::indexing_slicing)]
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.lines.is_empty() {
            let paragraph =
                Paragraph::new("No log entries found.\n\nRun `wasm self log` to view logs.")
                    .centered()
                    .style(Style::default().fg(Color::DarkGray));
            paragraph.render(area, buf);
        } else {
            let layout = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

            // Title with line count
            let title = Paragraph::new(format!("{} log line(s)", self.lines.len()))
                .style(Style::default().fg(Color::Cyan))
                .alignment(Alignment::Center);
            title.render(layout[0], buf);

            // Log content with scroll
            let visible_height = layout[1].height as usize;
            let total = self.lines.len();
            let start = self.scroll_offset.min(total.saturating_sub(visible_height));
            let end = (start + visible_height).min(total);
            let visible: Vec<Line<'_>> = self.lines[start..end]
                .iter()
                .map(|l| Line::from(l.as_str()))
                .collect();

            let paragraph = Paragraph::new(visible).style(Style::default().fg(Color::White));
            paragraph.render(layout[1], buf);

            // Help text
            let help = Paragraph::new("↑/↓ scroll • G jump to bottom • q quit")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            help.render(layout[2], buf);
        }
    }
}
