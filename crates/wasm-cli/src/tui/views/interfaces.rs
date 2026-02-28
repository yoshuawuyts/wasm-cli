use ratatui::{
    prelude::*,
    widgets::{
        Block, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
        Table, TableState, Widget, Wrap,
    },
};
use wasm_package_manager::WitInterfaceView;

/// State for the interfaces view
#[derive(Debug, Default)]
pub struct InterfacesViewState {
    /// Table state for list selection
    pub table_state: TableState,
    /// Scroll offset for the detail view
    pub detail_scroll: u16,
    /// Whether we're viewing detail (vs list)
    pub viewing_detail: bool,
}

impl InterfacesViewState {
    /// Create a new InterfacesViewState
    #[must_use]
    pub fn new() -> Self {
        Self {
            table_state: TableState::default().with_selected(Some(0)),
            detail_scroll: 0,
            viewing_detail: false,
        }
    }

    /// Get the currently selected interface index
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.table_state.selected()
    }

    /// Select the next interface in the list
    pub fn select_next(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0);
        self.table_state.select(Some((current + 1) % len));
    }

    /// Select the previous interface in the list
    pub fn select_prev(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0);
        self.table_state
            .select(Some(current.checked_sub(1).unwrap_or(len - 1)));
    }

    /// Scroll down in the detail view
    pub fn scroll_down(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_add(1);
    }

    /// Scroll up in the detail view
    pub fn scroll_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(1);
    }
}

/// View for displaying WIT interfaces
#[derive(Debug)]
pub struct InterfacesView<'a> {
    interfaces: &'a [(WitInterfaceView, String)],
}

impl<'a> InterfacesView<'a> {
    /// Create a new InterfacesView with the given interfaces
    #[must_use]
    pub fn new(interfaces: &'a [(WitInterfaceView, String)]) -> Self {
        Self { interfaces }
    }
}

impl StatefulWidget for InterfacesView<'_> {
    type State = InterfacesViewState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if self.interfaces.is_empty() {
            let msg = "No WIT interfaces found.\n\nPull a WebAssembly component to see its interfaces here.\nPress [2] to go to Components, then [p] to pull a package.";
            Paragraph::new(msg)
                .centered()
                .wrap(Wrap { trim: false })
                .render(area, buf);
            return;
        }

        if state.viewing_detail {
            // Render detail view
            if let Some(idx) = state.selected()
                && let Some((interface, component_ref)) = self.interfaces.get(idx)
            {
                self.render_detail(area, buf, state, interface, component_ref);
            }
        } else {
            // Render list view
            self.render_list(area, buf, state);
        }
    }
}

impl InterfacesView<'_> {
    fn render_list(&self, area: Rect, buf: &mut Buffer, state: &mut InterfacesViewState) {
        let header = Row::new(vec!["Package", "Version", "Component"])
            .style(Style::default().bold())
            .bottom_margin(1);

        let rows: Vec<Row> = self
            .interfaces
            .iter()
            .map(|(interface, component_ref)| {
                Row::new(vec![
                    interface.package_name.clone(),
                    interface.version.clone().unwrap_or_else(|| "-".to_string()),
                    component_ref.clone(),
                ])
            })
            .collect();

        let widths = [
            Constraint::Percentage(35),
            Constraint::Percentage(20),
            Constraint::Percentage(45),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(Style::default().bg(Color::DarkGray))
            .highlight_symbol(" ▶ ");

        StatefulWidget::render(table, area, buf, &mut state.table_state);

        // Render help text at bottom
        let help_area = Rect {
            x: area.x,
            y: area.y.saturating_add(area.height.saturating_sub(1)),
            width: area.width,
            height: 1,
        };
        let help_text = " ↑/↓ navigate │ Enter view WIT │ q quit ";
        Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray))
            .render(help_area, buf);
    }

    #[allow(clippy::indexing_slicing)]
    fn render_detail(
        &self,
        area: Rect,
        buf: &mut Buffer,
        state: &mut InterfacesViewState,
        interface: &WitInterfaceView,
        component_ref: &str,
    ) {
        // Split into header and content
        let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);

        // Header with component info
        let header_text = format!(
            "Package: {}  │  Version: {}",
            interface.package_name,
            interface.version.as_deref().unwrap_or("-"),
        );
        let header = Paragraph::new(header_text)
            .style(Style::default().bold())
            .block(Block::bordered().title(format!(" {} ", component_ref)));
        header.render(chunks[0], buf);

        // WIT content with scrolling
        let wit_source = interface.wit_text.as_deref().unwrap_or("<no WIT text>");
        let wit_lines: Vec<Line> = wit_source
            .lines()
            .enumerate()
            .map(|(i, line)| {
                // Apply syntax highlighting
                let style = if line.trim().starts_with("//") || line.trim().starts_with("///") {
                    Style::default().fg(Color::DarkGray)
                } else if line.contains("package ")
                    || line.contains("world ")
                    || line.contains("interface ")
                {
                    Style::default().fg(Color::Cyan).bold()
                } else if line.contains("import ") {
                    Style::default().fg(Color::Green)
                } else if line.contains("export ") {
                    Style::default().fg(Color::Yellow)
                } else if line.contains("func ") || line.contains("resource ") {
                    Style::default().fg(Color::Magenta)
                } else if line.contains("record ")
                    || line.contains("enum ")
                    || line.contains("variant ")
                    || line.contains("flags ")
                {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default()
                };
                Line::styled(format!("{:4} │ {}", i + 1, line), style)
            })
            .collect();

        let total_lines = wit_lines.len() as u16;
        let visible_height = chunks[1].height.saturating_sub(2); // Account for block borders

        // Clamp scroll to valid range
        let max_scroll = total_lines.saturating_sub(visible_height);
        if state.detail_scroll > max_scroll {
            state.detail_scroll = max_scroll;
        }

        let wit_content = Paragraph::new(wit_lines)
            .scroll((state.detail_scroll, 0))
            .block(Block::bordered().title(" WIT Definition (Esc to go back, ↑/↓ to scroll) "));
        wit_content.render(chunks[1], buf);

        // Render scrollbar if needed
        if total_lines > visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));
            let mut scrollbar_state =
                ScrollbarState::new(total_lines as usize).position(state.detail_scroll as usize);

            let scrollbar_area = Rect {
                x: chunks[1].x + chunks[1].width - 1,
                y: chunks[1].y + 1,
                width: 1,
                height: chunks[1].height.saturating_sub(2),
            };
            scrollbar.render(scrollbar_area, buf, &mut scrollbar_state);
        }
    }
}
