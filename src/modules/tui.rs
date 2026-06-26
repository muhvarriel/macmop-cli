use crate::core::AppContext;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::io;

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run(ctx: &AppContext) -> Result<()> {
    let _guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut state = TuiState::new();

    loop {
        terminal.draw(|f| ui(f, &state, ctx))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.previous();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.next();
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

struct TuiState {
    modules: Vec<TuiModule>,
    list_state: ListState,
}

struct TuiModule {
    name: &'static str,
    description: &'static str,
    details: &'static str,
}

impl TuiState {
    fn new() -> Self {
        let modules = vec![
            TuiModule {
                name: "Overview",
                description: "System cleaning and maintenance summary",
                details: "Welcome to MacMop!\n\nThis TUI provides a read-only overview of your system's cleaning opportunities and safety configurations. Navigate using the sidebar on the left.",
            },
            TuiModule {
                name: "Cleanup",
                description: "Safe cache, logs, and temp cleaning",
                details: "Cleanup Module (Read-Only)\n\nIdentifies safe-to-remove files including:\n- System and User Cache files\n- System Logs and Diagnostic reports\n- Temporary directory files\n- Xcode derived data and build artifacts\n\nNo modifications are performed in this dashboard view.",
            },
            TuiModule {
                name: "Clutter",
                description: "Large downloads and clutter scan",
                details: "Clutter Module (Read-Only)\n\nScans user directories (e.g., ~/Downloads) for large, forgotten files, installer archives (.dmg, .pkg), and download clutter exceeding the target size threshold.",
            },
            TuiModule {
                name: "Disk",
                description: "Interactive storage space mapping",
                details: "Disk Module (Read-Only)\n\nTraverses and catalogs directories to build an interactive, sorted breakdown of the largest folders and files consuming space on your system.",
            },
            TuiModule {
                name: "Apps",
                description: "Application leftovers report",
                details: "Apps Module (Read-Only)\n\nScans /Applications and ~/Applications to report bundle metadata and identify orphaned application support, cache, and preference files left behind from previous uninstalls.",
            },
            TuiModule {
                name: "Startup",
                description: "LaunchAgent and Daemon inventory",
                details: "Startup Module (Read-Only)\n\nCatalogs and parses plist metadata for all active user and system LaunchAgents/LaunchDaemons. Highlights missing executables and malformed startup configurations.",
            },
            TuiModule {
                name: "Protect",
                description: "Persistence and security risk check",
                details: "Protect Module (Read-Only)\n\nAnalyzes startup plists and binaries for security/persistence risks. Flags items executing dynamic shell commands or utilizing temp/network execution paths.",
            },
            TuiModule {
                name: "Privacy",
                description: "Browser and local privacy metadata",
                details: "Privacy Module (Read-Only)\n\nIdentifies browser caches (Safari, Chrome, Firefox), recent item plist databases, QuickLook thumbnail caches, and shell history presence without reading personal command history.",
            },
            TuiModule {
                name: "Maintenance",
                description: "Preflight system optimization checklist",
                details: "Maintenance Module (Read-Only)\n\nLists available system maintenance tasks (e.g., Flush DNS, Rebuild Spotlight, Thin Time Machine Snapshots) and performs dry-run verification checks.",
            },
        ];

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            modules,
            list_state,
        }
    }

    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.modules.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.modules.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }
}

fn ui(f: &mut ratatui::Frame, state: &TuiState, ctx: &AppContext) {
    // Overall layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main body
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    // 1. Header
    let safety_indicator = if ctx.mode.is_destructive() {
        Span::raw("MUTATION (guarded)").fg(Color::Red)
    } else {
        Span::raw("SAFE (read-only)").fg(Color::Green)
    };

    let header_text = vec![Line::from(vec![
        Span::raw(" MacMop Dashboard  ").bold().fg(Color::Cyan),
        Span::raw(" |  Version: ").fg(Color::DarkGray),
        Span::raw(env!("CARGO_PKG_VERSION")).bold(),
        Span::raw("  |  Mode: ").fg(Color::DarkGray),
        safety_indicator,
    ])];
    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(header, chunks[0]);

    // 2. Main Body Split (Sidebar vs Detail Panel)
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // Sidebar
            Constraint::Percentage(70), // Detail Panel
        ])
        .split(chunks[1]);

    // Sidebar List
    let items: Vec<ListItem> = state
        .modules
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let prefix = if state.list_state.selected() == Some(idx) {
                "▶ "
            } else {
                "  "
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::raw(prefix).fg(Color::Cyan),
                    Span::raw(m.name).bold(),
                ]),
                Line::from(vec![
                    Span::raw("    ").fg(Color::DarkGray),
                    Span::raw(m.description).fg(Color::DarkGray),
                ]),
            ])
        })
        .collect();

    let sidebar = List::new(items)
        .block(Block::default().title(" Modules ").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(30, 30, 45))
                .add_modifier(Modifier::BOLD),
        );

    // We need to render the stateful list, so we mutably select state
    let mut list_state = state.list_state;
    f.render_stateful_widget(sidebar, body_chunks[0], &mut list_state);

    // Detail Panel
    let selected_idx = state.list_state.selected().unwrap_or(0);
    let selected_module = &state.modules[selected_idx];

    let detail_block = Block::default()
        .title(format!(" {} Overview ", selected_module.name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let details_text = if selected_idx == 0 {
        // Special formatting for Overview
        vec![
            Line::from(
                "MacMop Storage & Maintenance Dashboard"
                    .bold()
                    .fg(Color::Cyan),
            ),
            Line::from(""),
            Line::from(vec![
                Span::raw("Safety Level: ").fg(Color::DarkGray),
                if ctx.mode.is_destructive() {
                    "MUTATION ACTIVE".red().bold()
                } else {
                    "READ-ONLY MODE (No files will be modified)".green().bold()
                },
            ]),
            Line::from(vec![
                Span::raw("Home Directory: ").fg(Color::DarkGray),
                Span::raw(ctx.paths.home.display().to_string()).italic(),
            ]),
            Line::from(vec![
                Span::raw("Trash Directory: ").fg(Color::DarkGray),
                Span::raw(ctx.paths.trash.display().to_string()).italic(),
            ]),
            Line::from(""),
            Line::from("Use [Up/Down] or [j/k] to navigate through the left sidebar."),
            Line::from("Press [q] or [Esc] to exit the TUI dashboard at any time."),
        ]
    } else {
        vec![Line::from(selected_module.details)]
    };

    let detail_panel = Paragraph::new(details_text)
        .block(detail_block)
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(detail_panel, body_chunks[1]);

    // 3. Footer
    let footer_text = vec![Line::from(vec![
        Span::raw("  [q/Esc] Quit  ").bold().fg(Color::Red),
        Span::raw("  [↑/↓/j/k] Navigate  ").bold().fg(Color::Cyan),
        Span::raw("  [Enter] View  ").bold().fg(Color::DarkGray),
    ])];
    let footer = Paragraph::new(footer_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(footer, chunks[2]);
}
