use ratatui::{
    prelude::*,
    widgets::{Cell, Paragraph, Row, Table, Widget},
};
use wasm_package_manager::storage::StateInfo;

/// View for displaying settings and state information
#[derive(Debug)]
pub struct SettingsView<'a> {
    state_info: Option<&'a StateInfo>,
}

impl<'a> SettingsView<'a> {
    /// Creates a new settings view
    #[must_use]
    pub fn new(state_info: Option<&'a StateInfo>) -> Self {
        Self { state_info }
    }
}

impl Widget for SettingsView<'_> {
    #[allow(clippy::indexing_slicing)]
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.state_info {
            Some(info) => {
                // Split area for migrations, configuration, and storage sections
                let layout = Layout::vertical([
                    Constraint::Length(3), // Migrations
                    Constraint::Length(5), // Configuration
                    Constraint::Min(0),    // Storage
                ])
                .split(area);

                // Migrations section
                let migrations = Text::from(vec![
                    Line::from(vec![Span::styled(
                        "Migrations",
                        Style::default().bold().fg(Color::Yellow),
                    )]),
                    Line::from(format!(
                        "  Current:  {}/{}",
                        info.migration_current(),
                        info.migration_total()
                    )),
                ]);
                Paragraph::new(migrations).render(layout[0], buf);

                // Configuration section
                let config_path = info.config_file();
                let config_status = if config_path.exists() {
                    "exists"
                } else {
                    "not created"
                };
                let local_config_path = wasm_package_manager::Config::local_config_path();
                let local_config_status = if local_config_path.exists() {
                    "exists"
                } else {
                    "not found"
                };
                let local_config_display = local_config_path.to_string_lossy().replace('\\', "/");
                let configuration = Text::from(vec![
                    Line::from(vec![Span::styled(
                        "Configuration",
                        Style::default().bold().fg(Color::Yellow),
                    )]),
                    Line::from(format!(
                        "  Global config: {} ({})",
                        config_path.display().to_string().replace('\\', "/"),
                        config_status
                    )),
                    Line::from(format!(
                        "  Local config:  {} ({})",
                        local_config_display, local_config_status
                    )),
                ]);
                Paragraph::new(configuration).render(layout[1], buf);

                // Storage section with table
                let storage_layout =
                    Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(layout[2]);

                let storage_header = Line::from(vec![Span::styled(
                    "Storage",
                    Style::default().bold().fg(Color::Yellow),
                )]);
                Paragraph::new(storage_header).render(storage_layout[0], buf);

                // Compute column widths based on content
                // Normalize path separators for consistent cross-platform display
                let executable_path = info.executable().display().to_string().replace('\\', "/");
                let data_dir_path = info.data_dir().display().to_string().replace('\\', "/");
                let store_dir_path = info.store_dir().display().to_string().replace('\\', "/");
                let metadata_file_path = info
                    .metadata_file()
                    .display()
                    .to_string()
                    .replace('\\', "/");
                let store_size = super::format_size(info.store_size());
                let metadata_size = super::format_size(info.metadata_size());

                // Column 1: longest is "Image metadata" = 14 chars
                let col1_width = 14;
                // Column 2: longest path
                let col2_width = executable_path
                    .len()
                    .max(data_dir_path.len())
                    .max(store_dir_path.len())
                    .max(metadata_file_path.len());
                // Column 3: longest size string or "-"
                let col3_width = store_size.len().max(metadata_size.len()).max(1);

                // Create data rows
                let rows = vec![
                    Row::new(vec![
                        Cell::from("Executable"),
                        Cell::from(executable_path),
                        Cell::from("-"),
                    ]),
                    Row::new(vec![
                        Cell::from("Data storage"),
                        Cell::from(data_dir_path),
                        Cell::from("-"),
                    ]),
                    Row::new(vec![
                        Cell::from("Content store"),
                        Cell::from(store_dir_path),
                        Cell::from(store_size),
                    ]),
                    Row::new(vec![
                        Cell::from("Image metadata"),
                        Cell::from(metadata_file_path),
                        Cell::from(metadata_size),
                    ]),
                ];

                let table = Table::new(
                    rows,
                    [
                        Constraint::Length(col1_width as u16),
                        Constraint::Length(col2_width as u16),
                        Constraint::Length(col3_width as u16),
                    ],
                )
                .column_spacing(3);

                Widget::render(table, storage_layout[1], buf);
            }
            None => {
                Paragraph::new("Loading state information...").render(area, buf);
            }
        }
    }
}
