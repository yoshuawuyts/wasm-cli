use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    prelude::*,
    widgets::{Block, Clear, Paragraph},
};
use std::time::Duration;
use tokio::sync::mpsc;
use wasm_package_manager::manager::PullResult;
use wasm_package_manager::oci::{ImageEntry, InsertResult};
use wasm_package_manager::storage::{KnownPackage, StateInfo};
use wasm_package_manager::types::WitPackage;

use super::components::{TabBar, TabItem};
use super::views::packages::PackagesViewState;
use super::views::{
    LocalView, LogView, PackageDetailView, PackagesView, SearchView, SearchViewState, SettingsView,
    TypesView, TypesViewState,
};
use super::{AppEvent, ManagerEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Local,
    Components,
    Interfaces,
    Search,
    Settings,
    Log,
}

impl Tab {
    const ALL: [Tab; 6] = [
        Tab::Local,
        Tab::Components,
        Tab::Interfaces,
        Tab::Search,
        Tab::Settings,
        Tab::Log,
    ];
}

impl TabItem for Tab {
    fn all() -> &'static [Self] {
        &Self::ALL
    }

    fn title(&self) -> &'static str {
        match self {
            Tab::Local => "Local [1]",
            Tab::Components => "Components [2]",
            Tab::Interfaces => "Interfaces [3]",
            Tab::Search => "Search [4]",
            Tab::Settings => "Settings [5]",
            Tab::Log => "Log [6]",
        }
    }
}

/// The current input mode of the application
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum InputMode {
    /// Normal navigation mode
    #[default]
    Normal,
    /// Viewing a package detail (with the package index)
    PackageDetail(usize),
    /// Viewing type detail
    TypeDetail,
    /// Pull prompt is active
    PullPrompt(PullPromptState),
    /// Search input is active
    SearchInput,
    /// Filter input is active (for packages tab)
    FilterInput,
}

/// State of the pull prompt
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PullPromptState {
    pub input: String,
    pub error: Option<String>,
    pub in_progress: bool,
}

/// Manager readiness state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ManagerState {
    #[default]
    Loading,
    Ready,
}

pub(crate) struct App {
    running: bool,
    manager_state: ManagerState,
    current_tab: Tab,
    input_mode: InputMode,
    packages: Vec<ImageEntry>,
    packages_view_state: PackagesViewState,
    /// State info from the manager
    state_info: Option<StateInfo>,
    /// Search view state
    search_view_state: SearchViewState,
    /// Known packages for search results
    known_packages: Vec<KnownPackage>,
    /// WIT interfaces with their component references
    wit_types: Vec<(WitPackage, String)>,
    /// Types view state
    types_view_state: TypesViewState,
    /// Local WASM files
    local_wasm_files: Vec<wasm_detector::WasmEntry>,
    /// Log file lines
    log_lines: Vec<String>,
    /// Scroll offset for log view
    log_scroll: usize,
    /// Whether offline mode is enabled
    offline: bool,
    event_sender: mpsc::Sender<AppEvent>,
    manager_receiver: mpsc::Receiver<ManagerEvent>,
}

impl App {
    pub(crate) fn new(
        event_sender: mpsc::Sender<AppEvent>,
        manager_receiver: mpsc::Receiver<ManagerEvent>,
        offline: bool,
    ) -> Self {
        Self {
            running: true,
            manager_state: ManagerState::default(),
            current_tab: Tab::Local,
            input_mode: InputMode::default(),
            packages: Vec::new(),
            packages_view_state: PackagesViewState::new(),
            state_info: None,
            search_view_state: SearchViewState::new(),
            known_packages: Vec::new(),
            wit_types: Vec::new(),
            types_view_state: TypesViewState::new(),
            local_wasm_files: Vec::new(),
            log_lines: Vec::new(),
            log_scroll: 0,
            offline,
            event_sender,
            manager_receiver,
        }
    }

    pub(crate) fn run(mut self, mut terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
        while self.running {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events()?;
            self.handle_manager_events();
        }
        // Notify manager that we're quitting
        let _ = self.event_sender.try_send(AppEvent::Quit);
        Ok(())
    }

    #[allow(clippy::indexing_slicing)]
    fn render_frame(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let status = match (self.manager_state, self.offline) {
            (_, true) => "offline",
            (ManagerState::Ready, false) => "ready",
            (ManagerState::Loading, false) => "loading...",
        };

        // Create main layout with tabs at top
        let layout = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);

        // Render tab bar
        let tab_bar = TabBar::new(format!("wasm(1) - {status}"), self.current_tab);
        frame.render_widget(tab_bar, layout[0]);

        // Render content based on current tab
        let content_block = Block::bordered();
        let content_area = content_block.inner(layout[1]);
        frame.render_widget(content_block, layout[1]);

        match self.current_tab {
            Tab::Local => frame.render_widget(LocalView::new(&self.local_wasm_files), content_area),
            Tab::Components => {
                // Check if we're viewing a package detail
                if let InputMode::PackageDetail(idx) = self.input_mode {
                    if let Some(package) = self.packages.get(idx) {
                        frame.render_widget(PackageDetailView::new(package), content_area);
                    }
                } else {
                    // Sync filter_active state for rendering
                    self.packages_view_state.filter_active =
                        self.input_mode == InputMode::FilterInput;
                    let filtered: Vec<_> = self.filtered_packages().into_iter().cloned().collect();
                    frame.render_stateful_widget(
                        PackagesView::new(&filtered),
                        content_area,
                        &mut self.packages_view_state,
                    );
                }
            }
            Tab::Interfaces => {
                frame.render_stateful_widget(
                    TypesView::new(&self.wit_types),
                    content_area,
                    &mut self.types_view_state,
                );
            }
            Tab::Search => {
                // Sync search_active state for rendering
                self.search_view_state.search_active = self.input_mode == InputMode::SearchInput;
                frame.render_stateful_widget(
                    SearchView::new(&self.known_packages),
                    content_area,
                    &mut self.search_view_state,
                );
            }
            Tab::Settings => {
                frame.render_widget(SettingsView::new(self.state_info.as_ref()), content_area);
            }
            Tab::Log => {
                frame.render_widget(LogView::new(&self.log_lines, self.log_scroll), content_area);
            }
        }

        // Render pull prompt overlay if active
        if let InputMode::PullPrompt(ref state) = self.input_mode {
            Self::render_pull_prompt(frame, area, state);
        }
    }

    #[allow(clippy::indexing_slicing)]
    fn render_pull_prompt(frame: &mut Frame, area: Rect, state: &PullPromptState) {
        // Calculate centered popup area
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = if state.error.is_some() { 7 } else { 5 };
        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        // Build the prompt content
        let title = if state.in_progress {
            " Pull Package (pulling...) "
        } else {
            " Pull Package "
        };

        let block = Block::bordered()
            .title(title)
            .style(Style::default().bg(Color::DarkGray));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Layout for input and optional error
        let chunks = if state.error.is_some() {
            Layout::vertical([
                Constraint::Length(1), // Label
                Constraint::Length(1), // Input
                Constraint::Length(1), // Error
            ])
            .split(inner)
        } else {
            Layout::vertical([
                Constraint::Length(1), // Label
                Constraint::Length(1), // Input
            ])
            .split(inner)
        };

        // Label
        let label = Paragraph::new("Enter package reference (e.g., ghcr.io/user/pkg:tag):");
        frame.render_widget(label, chunks[0]);

        // Input field with cursor
        let input_text = format!("{}_", state.input);
        let input = Paragraph::new(input_text).style(Style::default().fg(Color::Yellow));
        frame.render_widget(input, chunks[1]);

        // Error message if present
        if let Some(ref error) = state.error {
            let error_msg = Paragraph::new(error.as_str()).style(Style::default().fg(Color::Red));
            frame.render_widget(error_msg, chunks[2]);
        }
    }

    fn handle_events(&mut self) -> std::io::Result<()> {
        // Poll with a timeout so we can also check manager events
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key(key_event.code, key_event.modifiers);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_manager_events(&mut self) {
        while let Ok(event) = self.manager_receiver.try_recv() {
            match event {
                ManagerEvent::Ready => {
                    self.manager_state = ManagerState::Ready;
                    // Request packages list and state info when manager is ready
                    let _ = self.event_sender.try_send(AppEvent::RequestPackages);
                    let _ = self.event_sender.try_send(AppEvent::RequestStateInfo);
                    let _ = self.event_sender.try_send(AppEvent::RequestKnownPackages);
                    let _ = self.event_sender.try_send(AppEvent::RequestWitTypes);
                    let _ = self.event_sender.try_send(AppEvent::DetectLocalWasm);
                }
                ManagerEvent::PackagesList(packages) => {
                    self.packages = packages;
                }
                ManagerEvent::StateInfo(state_info) => {
                    self.state_info = Some(state_info);
                }
                ManagerEvent::PullResult(result) => {
                    self.handle_pull_result(result);
                }
                ManagerEvent::DeleteResult(_result) => {
                    // Delete completed, packages list will be refreshed automatically
                }
                ManagerEvent::SearchResults(packages)
                | ManagerEvent::KnownPackagesList(packages) => {
                    self.known_packages = packages;
                }
                ManagerEvent::RefreshTagsResult(_result) => {
                    // Tag refresh completed, packages list will be refreshed automatically
                }
                ManagerEvent::WitTypesList(types) => {
                    self.wit_types = types;
                }
                ManagerEvent::LocalWasmList(files) => {
                    self.local_wasm_files = files;
                }
                ManagerEvent::PullProgress(_progress) => {
                    // Progress events received — TUI rendering deferred to follow-up
                }
                ManagerEvent::LogLines(lines) => {
                    self.log_lines = lines;
                }
            }
        }
    }

    fn handle_pull_result(&mut self, result: Result<Box<PullResult>, String>) {
        match result {
            Ok(pull_result) => {
                // Close the prompt on success, but show warning if already exists
                let error = if pull_result.insert_result == InsertResult::AlreadyExists {
                    Some("Warning: package already exists in local store".to_string())
                } else {
                    None
                };
                self.input_mode = if let Some(e) = error {
                    InputMode::PullPrompt(PullPromptState {
                        input: String::new(),
                        error: Some(e),
                        in_progress: false,
                    })
                } else {
                    InputMode::Normal
                };
                // Refresh known packages and WIT interfaces
                let _ = self.event_sender.try_send(AppEvent::RequestKnownPackages);
                let _ = self.event_sender.try_send(AppEvent::RequestWitTypes);
            }
            Err(e) => {
                // Keep the prompt open with the error
                if let InputMode::PullPrompt(ref mut state) = self.input_mode {
                    state.error = Some(e);
                    state.in_progress = false;
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        match &self.input_mode {
            InputMode::PullPrompt(_) => self.handle_pull_prompt_key(key, modifiers),
            InputMode::SearchInput => self.handle_search_key(key, modifiers),
            InputMode::FilterInput => self.handle_filter_key(key, modifiers),
            InputMode::PackageDetail(_) => self.handle_package_detail_key(key, modifiers),
            InputMode::TypeDetail => self.handle_type_detail_key(key, modifiers),
            InputMode::Normal => self.handle_normal_key(key, modifiers),
        }
    }

    fn handle_package_detail_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        match key {
            KeyCode::Esc | KeyCode::Backspace => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('c') if modifiers == KeyModifiers::CONTROL => self.running = false,
            _ => {}
        }
    }

    fn handle_type_detail_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        match key {
            KeyCode::Esc | KeyCode::Backspace => {
                self.types_view_state.viewing_detail = false;
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.types_view_state.scroll_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.types_view_state.scroll_down();
            }
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('c') if modifiers == KeyModifiers::CONTROL => self.running = false,
            _ => {}
        }
    }

    fn handle_normal_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        match (key, modifiers) {
            (KeyCode::Char('q') | KeyCode::Esc, _)
            | (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.running = false,
            // Tab navigation
            (KeyCode::Tab | KeyCode::Right, _) => {
                self.current_tab = self.current_tab.next();
            }
            (KeyCode::BackTab | KeyCode::Left, _) => {
                self.current_tab = self.current_tab.prev();
            }
            (KeyCode::Char('1'), _) => self.current_tab = Tab::Local,
            (KeyCode::Char('2'), _) => self.current_tab = Tab::Components,
            (KeyCode::Char('3'), _) => self.current_tab = Tab::Interfaces,
            (KeyCode::Char('4'), _) => self.current_tab = Tab::Search,
            (KeyCode::Char('5'), _) => self.current_tab = Tab::Settings,
            (KeyCode::Char('6'), _) => {
                self.current_tab = Tab::Log;
                let _ = self.event_sender.try_send(AppEvent::RequestLogLines);
            }
            // Pull prompt - 'p' to open (only on Components tab, and not in offline mode)
            (KeyCode::Char('p'), _)
                if self.current_tab == Tab::Components && self.can_use_network() =>
            {
                self.input_mode = InputMode::PullPrompt(PullPromptState::default());
            }
            // Activate filter input with '/' on Components tab
            (KeyCode::Char('/'), _) if self.current_tab == Tab::Components => {
                self.input_mode = InputMode::FilterInput;
                self.packages_view_state.filter_active = true;
            }
            // Package list navigation (when on Components tab)
            (KeyCode::Up | KeyCode::Char('k'), _) if self.current_tab == Tab::Components => {
                self.packages_view_state
                    .select_prev(self.filtered_packages().len());
            }
            (KeyCode::Down | KeyCode::Char('j'), _) if self.current_tab == Tab::Components => {
                self.packages_view_state
                    .select_next(self.filtered_packages().len());
            }
            (KeyCode::Enter, _) if self.current_tab == Tab::Components => {
                let filtered = self.filtered_packages();
                if let Some(selected) = self.packages_view_state.selected()
                    && let Some(package) = filtered.get(selected)
                {
                    // Find the actual index in the unfiltered list for package detail view
                    if let Some(actual_idx) = self.packages.iter().position(|p| {
                        p.ref_repository == package.ref_repository
                            && p.ref_registry == package.ref_registry
                            && p.ref_tag == package.ref_tag
                            && p.ref_digest == package.ref_digest
                    }) {
                        self.input_mode = InputMode::PackageDetail(actual_idx);
                    }
                }
            }
            // Delete selected package
            (KeyCode::Char('d'), _)
                if self.current_tab == Tab::Components && self.is_manager_ready() =>
            {
                let filtered = self.filtered_packages();
                if let Some(selected) = self.packages_view_state.selected()
                    && let Some(package) = filtered.get(selected)
                {
                    let _ = self
                        .event_sender
                        .try_send(AppEvent::Delete(package.reference()));
                    // Adjust selection if we're deleting the last item
                    if selected > 0 && selected >= filtered.len() - 1 {
                        self.packages_view_state
                            .table_state
                            .select(Some(selected - 1));
                    }
                }
            }
            // Search tab navigation
            (KeyCode::Up | KeyCode::Char('k'), _) if self.current_tab == Tab::Search => {
                self.search_view_state
                    .select_prev(self.known_packages.len());
            }
            (KeyCode::Down | KeyCode::Char('j'), _) if self.current_tab == Tab::Search => {
                self.search_view_state
                    .select_next(self.known_packages.len());
            }
            // Activate search input with '/'
            (KeyCode::Char('/'), _) if self.current_tab == Tab::Search => {
                self.input_mode = InputMode::SearchInput;
            }
            // Interfaces tab navigation
            (KeyCode::Up | KeyCode::Char('k'), _) if self.current_tab == Tab::Interfaces => {
                self.types_view_state.select_prev(self.wit_types.len());
            }
            (KeyCode::Down | KeyCode::Char('j'), _) if self.current_tab == Tab::Interfaces => {
                self.types_view_state.select_next(self.wit_types.len());
            }
            (KeyCode::Enter, _) if self.current_tab == Tab::Interfaces => {
                if !self.wit_types.is_empty() {
                    self.types_view_state.viewing_detail = true;
                    self.types_view_state.detail_scroll = 0;
                    self.input_mode = InputMode::TypeDetail;
                }
            }
            // Pull selected package from search results (not in offline mode)
            (KeyCode::Char('p'), _)
                if self.current_tab == Tab::Search && self.can_use_network() =>
            {
                if let Some(selected) = self.search_view_state.selected()
                    && let Some(package) = self.known_packages.get(selected)
                {
                    // Pull the package with the most recent tag (or latest if none)
                    let reference = package.reference_with_tag();
                    let _ = self.event_sender.try_send(AppEvent::Pull(reference));
                }
            }
            // Refresh tags for selected package from registry (not in offline mode)
            (KeyCode::Char('r'), _)
                if self.current_tab == Tab::Search && self.can_use_network() =>
            {
                if let Some(selected) = self.search_view_state.selected()
                    && let Some(package) = self.known_packages.get(selected)
                {
                    let _ = self.event_sender.try_send(AppEvent::RefreshTags(
                        package.registry.clone(),
                        package.repository.clone(),
                    ));
                }
            }
            // Log tab navigation
            (KeyCode::Up | KeyCode::Char('k'), _) if self.current_tab == Tab::Log => {
                self.log_scroll = self.log_scroll.saturating_sub(1);
            }
            (KeyCode::Down | KeyCode::Char('j'), _) if self.current_tab == Tab::Log => {
                if self.log_scroll < self.log_lines.len().saturating_sub(1) {
                    self.log_scroll += 1;
                }
            }
            (KeyCode::Char('G'), KeyModifiers::SHIFT) if self.current_tab == Tab::Log => {
                self.log_scroll = self.log_lines.len().saturating_sub(1);
            }
            _ => {}
        }
    }

    fn handle_filter_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.packages_view_state.filter_active = false;
            }
            KeyCode::Enter => {
                // Just exit filter input mode, filter is applied live
                self.input_mode = InputMode::Normal;
                self.packages_view_state.filter_active = false;
            }
            KeyCode::Backspace => {
                self.packages_view_state.filter_query.pop();
                // Reset selection when filter changes
                self.packages_view_state.table_state.select(Some(0));
            }
            KeyCode::Char(c) => {
                if modifiers == KeyModifiers::CONTROL && c == 'c' {
                    self.running = false;
                } else {
                    self.packages_view_state.filter_query.push(c);
                    // Reset selection when filter changes
                    self.packages_view_state.table_state.select(Some(0));
                }
            }
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Enter => {
                // Execute search
                self.input_mode = InputMode::Normal;
                if self.search_view_state.search_query.is_empty() {
                    let _ = self.event_sender.try_send(AppEvent::RequestKnownPackages);
                } else {
                    let _ = self.event_sender.try_send(AppEvent::SearchPackages(
                        self.search_view_state.search_query.clone(),
                    ));
                }
            }
            KeyCode::Backspace => {
                self.search_view_state.search_query.pop();
            }
            KeyCode::Char(c) => {
                if modifiers == KeyModifiers::CONTROL && c == 'c' {
                    self.running = false;
                } else {
                    self.search_view_state.search_query.push(c);
                }
            }
            _ => {}
        }
    }

    fn handle_pull_prompt_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        let InputMode::PullPrompt(ref mut state) = self.input_mode else {
            return;
        };

        // Don't allow input while pull is in progress
        if state.in_progress {
            return;
        }

        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Enter => {
                if !state.input.is_empty() {
                    // Send pull request to manager
                    let input = state.input.clone();
                    state.in_progress = true;
                    state.error = None;
                    let _ = self.event_sender.try_send(AppEvent::Pull(input));
                }
            }
            KeyCode::Backspace => {
                state.input.pop();
                state.error = None;
            }
            KeyCode::Char(c) => {
                if modifiers == KeyModifiers::CONTROL && c == 'c' {
                    self.running = false;
                } else {
                    state.input.push(c);
                    state.error = None;
                }
            }
            _ => {}
        }
    }

    fn is_manager_ready(&self) -> bool {
        self.manager_state == ManagerState::Ready
    }

    /// Returns true if network operations are allowed (manager ready and not offline)
    fn can_use_network(&self) -> bool {
        self.is_manager_ready() && !self.offline
    }

    fn filtered_packages(&self) -> Vec<&ImageEntry> {
        let query = self.packages_view_state.filter_query.to_lowercase();
        if query.is_empty() {
            self.packages.iter().collect()
        } else {
            self.packages
                .iter()
                .filter(|p| {
                    p.ref_repository.to_lowercase().contains(&query)
                        || p.ref_registry.to_lowercase().contains(&query)
                        || p.ref_tag
                            .as_ref()
                            .is_some_and(|t| t.to_lowercase().contains(&query))
                })
                .collect()
        }
    }
}
