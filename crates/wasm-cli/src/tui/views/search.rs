use ratatui::{
    prelude::*,
    widgets::{Block, Cell, Paragraph, Row, StatefulWidget, Table, TableState, Widget},
};
use wasm_package_manager::storage::KnownPackageView;

/// State for the search view
#[derive(Debug, Default)]
pub struct SearchViewState {
    /// Table selection state
    pub table_state: TableState,
    /// Current search query
    pub search_query: String,
    /// Whether search mode is active
    pub search_active: bool,
}

impl SearchViewState {
    /// Creates a new search view state
    #[must_use]
    pub fn new() -> Self {
        Self {
            table_state: TableState::default().with_selected(Some(0)),
            search_query: String::new(),
            search_active: false,
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

/// View for displaying search results
#[derive(Debug)]
pub struct SearchView<'a> {
    packages: &'a [KnownPackageView],
}

impl<'a> SearchView<'a> {
    /// Creates a new search view
    #[must_use]
    pub fn new(packages: &'a [KnownPackageView]) -> Self {
        Self { packages }
    }
}

impl StatefulWidget for SearchView<'_> {
    type State = SearchViewState;

    #[allow(clippy::indexing_slicing)]
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Split area into search input, content, and shortcuts bar
        let layout = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);
        let search_area = layout[0];
        let content_area = layout[1];
        let shortcuts_area = layout[2];

        // Render search input
        let search_style = if state.search_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };

        let search_text = if state.search_active {
            format!("{}_", state.search_query)
        } else if state.search_query.is_empty() {
            "Press / to search...".to_string()
        } else {
            state.search_query.clone()
        };

        let search_block = Block::bordered()
            .title(" Search ")
            .border_style(search_style);
        let search_input = Paragraph::new(search_text)
            .style(search_style)
            .block(search_block);
        search_input.render(search_area, buf);

        // Render package list
        if self.packages.is_empty() {
            let message = if state.search_query.is_empty() {
                "No known packages. Pull a package to add it to the list."
            } else {
                "No packages found matching your search."
            };
            Paragraph::new(message).centered().render(content_area, buf);
        } else {
            // Create header row
            let header = Row::new(vec![
                Cell::from("Repository").style(Style::default().bold()),
                Cell::from("Registry").style(Style::default().bold()),
                Cell::from("Tags").style(Style::default().bold()),
                Cell::from("Last Seen").style(Style::default().bold()),
            ])
            .style(Style::default().fg(Color::Yellow));

            // Create data rows
            let rows: Vec<Row> = self
                .packages
                .iter()
                .map(|entry| {
                    // Format tags (show first few)
                    let tags_display = if entry.tags.is_empty() {
                        "-".to_string()
                    } else if entry.tags.len() <= 3 {
                        entry.tags.join(", ")
                    } else {
                        format!("{}, +{}", entry.tags[..2].join(", "), entry.tags.len() - 2)
                    };
                    // Format the date nicely (just show date part)
                    let last_seen = entry
                        .last_seen_at
                        .split('T')
                        .next()
                        .unwrap_or(&entry.last_seen_at);
                    Row::new(vec![
                        Cell::from(entry.repository.clone()),
                        Cell::from(entry.registry.clone()),
                        Cell::from(tags_display),
                        Cell::from(last_seen.to_string()),
                    ])
                })
                .collect();

            let table = Table::new(
                rows,
                [
                    Constraint::Percentage(35),
                    Constraint::Percentage(25),
                    Constraint::Percentage(20),
                    Constraint::Percentage(20),
                ],
            )
            .header(header)
            .row_highlight_style(Style::default().bg(Color::DarkGray));

            StatefulWidget::render(table, content_area, buf, &mut state.table_state);
        }

        // Render shortcuts bar
        let shortcuts = Line::from(vec![
            Span::styled(" / ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Search  "),
            Span::styled(" p ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Pull selected  "),
            Span::styled(" r ", Style::default().fg(Color::Black).bg(Color::Yellow)),
            Span::raw(" Refresh tags  "),
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

impl Widget for SearchView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut state = SearchViewState::new();
        StatefulWidget::render(self, area, buf, &mut state);
    }
}
