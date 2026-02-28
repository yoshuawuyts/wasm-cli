use ratatui::{
    prelude::*,
    widgets::{Block, Cell, Paragraph, Row, StatefulWidget, Table, TableState, Widget},
};
use wasm_package_manager::ImageView;

use super::format_size;

/// State for the packages list view
#[derive(Debug, Default)]
pub struct PackagesViewState {
    /// Table selection state
    pub table_state: TableState,
    /// Current filter query
    pub filter_query: String,
    /// Whether filter mode is active
    pub filter_active: bool,
}

impl PackagesViewState {
    /// Creates a new packages view state
    #[must_use]
    pub fn new() -> Self {
        Self {
            table_state: TableState::default().with_selected(Some(0)),
            filter_query: String::new(),
            filter_active: false,
        }
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.table_state.selected()
    }

    pub(crate) fn select_next(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0);
        let next = if current >= len - 1 { 0 } else { current + 1 };
        self.table_state.select(Some(next));
    }

    pub(crate) fn select_prev(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0);
        let prev = if current == 0 { len - 1 } else { current - 1 };
        self.table_state.select(Some(prev));
    }
}

/// View for displaying the list of installed packages
#[derive(Debug)]
pub struct PackagesView<'a> {
    packages: &'a [ImageView],
}

impl<'a> PackagesView<'a> {
    /// Creates a new packages view
    #[must_use]
    pub fn new(packages: &'a [ImageView]) -> Self {
        Self { packages }
    }
}

impl StatefulWidget for PackagesView<'_> {
    type State = PackagesViewState;

    #[allow(clippy::indexing_slicing)]
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Split area into filter input, content, and shortcuts bar
        let layout = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);
        let filter_area = layout[0];
        let content_area = layout[1];
        let shortcuts_area = layout[2];

        // Render filter input
        let filter_style = if state.filter_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };

        let filter_text = if state.filter_active {
            format!("{}_", state.filter_query)
        } else if state.filter_query.is_empty() {
            "Press / to filter...".to_string()
        } else {
            state.filter_query.clone()
        };

        let filter_block = Block::bordered()
            .title(" Filter ")
            .border_style(filter_style);
        let filter_input = Paragraph::new(filter_text)
            .style(filter_style)
            .block(filter_block);
        filter_input.render(filter_area, buf);

        if self.packages.is_empty() {
            let message = if state.filter_query.is_empty() {
                "No packages stored."
            } else {
                "No packages found matching your filter."
            };
            Paragraph::new(message).centered().render(content_area, buf);
        } else {
            // Create header row
            let header = Row::new(vec![
                Cell::from("Repository").style(Style::default().bold()),
                Cell::from("Registry").style(Style::default().bold()),
                Cell::from("Tag").style(Style::default().bold()),
                Cell::from("Size").style(Style::default().bold()),
                Cell::from("Digest").style(Style::default().bold()),
            ])
            .style(Style::default().fg(Color::Yellow));

            // Create data rows
            let rows: Vec<Row> = self
                .packages
                .iter()
                .map(|entry| {
                    let tag = entry.ref_tag.as_deref().unwrap_or("-");
                    let size = format_size(entry.size_on_disk);
                    let digest = entry
                        .ref_digest
                        .as_ref()
                        .map(|d| {
                            // Strip "sha256:" prefix
                            d.strip_prefix("sha256:").unwrap_or(d).to_string()
                        })
                        .unwrap_or_else(|| "-".to_string());
                    Row::new(vec![
                        Cell::from(entry.ref_repository.clone()),
                        Cell::from(entry.ref_registry.clone()),
                        Cell::from(tag.to_string()),
                        Cell::from(size),
                        Cell::from(digest),
                    ])
                })
                .collect();

            let table = Table::new(
                rows,
                [
                    Constraint::Percentage(30),
                    Constraint::Percentage(20),
                    Constraint::Percentage(15),
                    Constraint::Percentage(12),
                    Constraint::Percentage(23),
                ],
            )
            .header(header)
            .row_highlight_style(Style::default().bg(Color::DarkGray));

            StatefulWidget::render(table, content_area, buf, &mut state.table_state);
        }

        // Render shortcuts bar
        let shortcuts = Line::from(vec![
            Span::styled(" / ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Filter  "),
            Span::styled(" p ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Pull  "),
            Span::styled(" d ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Delete  "),
            Span::styled(
                " Enter ",
                Style::default().fg(Color::Black).bg(Color::Yellow),
            ),
            Span::raw(" View details  "),
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Clear "),
        ]);
        Paragraph::new(shortcuts)
            .style(Style::default().fg(Color::DarkGray))
            .render(shortcuts_area, buf);
    }
}

impl Widget for PackagesView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut state = PackagesViewState::new();
        StatefulWidget::render(self, area, buf, &mut state);
    }
}
