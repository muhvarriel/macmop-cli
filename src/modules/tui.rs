use crate::cli::{
    AppsArgs, AppsCommand, CleanupArgs, MaintenanceArgs, MaintenanceCommand, PrivacyArgs,
    PrivacyCommand, ProtectArgs, ProtectCommand, StartupArgs, StartupCommand,
};
use crate::core::{AppContext, StatusSummary};
use crate::modules::{apps, cleanup, maintenance, privacy, protect, startup, status};
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

pub const TUI_DETAIL_LIMIT: usize = 100;

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

    // Load live data summaries on startup
    let data = TuiData::load(ctx);
    let mut state = TuiState::new(data);

    loop {
        terminal.draw(|f| ui(f, &state, ctx))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Up | KeyCode::Char('k') => match state.current_view {
                        TuiView::Sidebar => state.previous_sidebar(),
                        TuiView::Detail => state.previous_detail(),
                    },
                    KeyCode::Down | KeyCode::Char('j') => match state.current_view {
                        TuiView::Sidebar => state.next_sidebar(),
                        TuiView::Detail => state.next_detail(),
                    },
                    KeyCode::Enter => {
                        if let TuiView::Sidebar = state.current_view {
                            state.enter_detail();
                        }
                    }
                    KeyCode::Backspace => {
                        if let TuiView::Detail = state.current_view {
                            state.back_to_sidebar();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub struct TuiDetailItem {
    pub title: String,
    pub subtitle: String,
    pub meta: String,
}

pub struct TuiData {
    pub status_summary: Option<StatusSummary>,
    pub cleanup_findings: Option<Vec<TuiDetailItem>>,
    pub cleanup_findings_size: Option<u64>,
    pub apps: Option<Vec<TuiDetailItem>>,
    pub startup_items: Option<Vec<TuiDetailItem>>,
    pub protect_findings: Option<Vec<TuiDetailItem>>,
    pub privacy_findings: Option<Vec<TuiDetailItem>>,
    pub maintenance_tasks: Option<Vec<TuiDetailItem>>,
    pub warnings: Vec<String>,
}

impl TuiData {
    pub fn load(ctx: &AppContext) -> Self {
        let mut warnings = Vec::new();

        // 1. Load Status
        let status_summary = match status::run(ctx) {
            Ok(env) => match serde_json::from_value::<StatusSummary>(
                env.payload.get("summary").cloned().unwrap_or_default(),
            ) {
                Ok(sum) => Some(sum),
                Err(e) => {
                    warnings.push(format!("Failed to parse status summary: {e}"));
                    None
                }
            },
            Err(e) => {
                warnings.push(format!("Failed to load status summary: {e}"));
                None
            }
        };

        // 2. Load Cleanup (Bounded: logs and temp categories only)
        let (cleanup_findings, cleanup_findings_size) = match cleanup::run(
            ctx,
            CleanupArgs {
                category: vec!["logs".into(), "temp".into()],
                older_than_days: 30,
            },
        ) {
            Ok(env) => {
                let list = env.payload.get("findings").and_then(|f| f.as_array());
                let items = list.map(|arr| {
                    arr.iter()
                        .take(TUI_DETAIL_LIMIT)
                        .map(|val| TuiDetailItem {
                            title: val["path"].as_str().unwrap_or_default().to_string(),
                            subtitle: val["reason"].as_str().unwrap_or_default().to_string(),
                            meta: format!("{} bytes", val["size_bytes"].as_u64().unwrap_or(0)),
                        })
                        .collect()
                });
                let size = env
                    .payload
                    .get("action_plan")
                    .and_then(|p| p.get("total_size_bytes").and_then(|s| s.as_u64()));
                (items, size)
            }
            Err(e) => {
                warnings.push(format!("Failed to scan cleanup items: {e}"));
                (None, None)
            }
        };

        // 3. Load Apps List
        let apps = match apps::run(
            ctx,
            AppsArgs {
                command: AppsCommand::List,
            },
        ) {
            Ok(env) => env
                .payload
                .get("items")
                .and_then(|i| i.as_array())
                .map(|arr| {
                    arr.iter()
                        .take(TUI_DETAIL_LIMIT)
                        .map(|val| TuiDetailItem {
                            title: val["name"].as_str().unwrap_or_default().to_string(),
                            subtitle: val["path"].as_str().unwrap_or_default().to_string(),
                            meta: val["bundle_id"]
                                .as_str()
                                .unwrap_or("Unknown ID")
                                .to_string(),
                        })
                        .collect()
                }),
            Err(e) => {
                warnings.push(format!("Failed to list apps: {e}"));
                None
            }
        };

        // 4. Load Startup Items
        let startup_items = match startup::run(
            ctx,
            StartupArgs {
                command: StartupCommand::List,
            },
        ) {
            Ok(env) => env
                .payload
                .get("items")
                .and_then(|i| i.as_array())
                .map(|arr| {
                    arr.iter()
                        .take(TUI_DETAIL_LIMIT)
                        .map(|val| TuiDetailItem {
                            title: val["label"].as_str().unwrap_or_default().to_string(),
                            subtitle: val["path"].as_str().unwrap_or_default().to_string(),
                            meta: format!(
                                "Source: {}",
                                val["source"].as_str().unwrap_or("Unknown")
                            ),
                        })
                        .collect()
                }),
            Err(e) => {
                warnings.push(format!("Failed to list startup items: {e}"));
                None
            }
        };

        // 5. Load Protect Findings
        let protect_findings = match protect::run(
            ctx,
            ProtectArgs {
                command: ProtectCommand::Scan,
            },
        ) {
            Ok(env) => env
                .payload
                .get("findings")
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .take(TUI_DETAIL_LIMIT)
                        .map(|val| TuiDetailItem {
                            title: val["label"].as_str().unwrap_or_default().to_string(),
                            subtitle: val["evidence"]
                                .as_array()
                                .map(|evs| {
                                    evs.iter()
                                        .map(|ev| ev.as_str().unwrap_or_default())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default(),
                            meta: format!("Risk: {}", val["severity"].as_str().unwrap_or("Low")),
                        })
                        .collect()
                }),
            Err(e) => {
                warnings.push(format!("Failed to scan protection risks: {e}"));
                None
            }
        };

        // 6. Load Privacy Findings
        let privacy_findings = match privacy::run(
            ctx,
            PrivacyArgs {
                command: PrivacyCommand::Scan,
            },
        ) {
            Ok(env) => env
                .payload
                .get("findings")
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .take(TUI_DETAIL_LIMIT)
                        .map(|val| TuiDetailItem {
                            title: val["path"].as_str().unwrap_or_default().to_string(),
                            subtitle: val["detail"].as_str().unwrap_or_default().to_string(),
                            meta: format!(
                                "Category: {}, Size: {} bytes",
                                val["category"].as_str().unwrap_or("Unknown"),
                                val["size_bytes"].as_u64().unwrap_or(0)
                            ),
                        })
                        .collect()
                }),
            Err(e) => {
                warnings.push(format!("Failed to scan privacy items: {e}"));
                None
            }
        };

        // 7. Load Maintenance Tasks
        let maintenance_tasks = match maintenance::run(
            ctx,
            MaintenanceArgs {
                command: MaintenanceCommand::List,
            },
        ) {
            Ok(env) => env
                .payload
                .get("items")
                .and_then(|i| i.as_array())
                .map(|arr| {
                    arr.iter()
                        .take(TUI_DETAIL_LIMIT)
                        .map(|val| TuiDetailItem {
                            title: val["name"].as_str().unwrap_or_default().to_string(),
                            subtitle: val["description"].as_str().unwrap_or_default().to_string(),
                            meta: if val["available"].as_bool().unwrap_or(false) {
                                "Available".to_string()
                            } else {
                                "Unavailable".to_string()
                            },
                        })
                        .collect()
                }),
            Err(e) => {
                warnings.push(format!("Failed to list maintenance tasks: {e}"));
                None
            }
        };

        Self {
            status_summary,
            cleanup_findings,
            cleanup_findings_size,
            apps,
            startup_items,
            protect_findings,
            privacy_findings,
            maintenance_tasks,
            warnings,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TuiView {
    Sidebar,
    Detail,
}

struct TuiModule {
    name: &'static str,
    description: &'static str,
}

struct TuiState {
    modules: Vec<TuiModule>,
    list_state: ListState,
    detail_list_state: ListState,
    current_view: TuiView,
    data: TuiData,
}

impl TuiState {
    fn new(data: TuiData) -> Self {
        let modules = vec![
            TuiModule {
                name: "Overview",
                description: "System cleaning and maintenance summary",
            },
            TuiModule {
                name: "Cleanup",
                description: "Safe cache, logs, and temp cleaning",
            },
            TuiModule {
                name: "Clutter",
                description: "Large downloads and clutter scan",
            },
            TuiModule {
                name: "Disk",
                description: "Interactive storage space mapping",
            },
            TuiModule {
                name: "Apps",
                description: "Application leftovers report",
            },
            TuiModule {
                name: "Startup",
                description: "LaunchAgent and Daemon inventory",
            },
            TuiModule {
                name: "Protect",
                description: "Persistence and security risk check",
            },
            TuiModule {
                name: "Privacy",
                description: "Browser and local privacy metadata",
            },
            TuiModule {
                name: "Maintenance",
                description: "Preflight system optimization checklist",
            },
        ];

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut detail_list_state = ListState::default();
        detail_list_state.select(Some(0));

        Self {
            modules,
            list_state,
            detail_list_state,
            current_view: TuiView::Sidebar,
            data,
        }
    }

    fn enter_detail(&mut self) {
        // Only allow entering details if there's actual scrollable content on the page
        if self.list_state.selected().unwrap_or(0) != 2
            && self.list_state.selected().unwrap_or(0) != 3
        {
            self.current_view = TuiView::Detail;
            self.detail_list_state.select(Some(0));
        }
    }

    fn back_to_sidebar(&mut self) {
        self.current_view = TuiView::Sidebar;
    }

    fn next_sidebar(&mut self) {
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

    fn previous_sidebar(&mut self) {
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

    fn get_current_detail_len(&self) -> usize {
        let idx = self.list_state.selected().unwrap_or(0);
        match idx {
            1 => self
                .data
                .cleanup_findings
                .as_ref()
                .map(|l| l.len())
                .unwrap_or(0),
            4 => self.data.apps.as_ref().map(|l| l.len()).unwrap_or(0),
            5 => self
                .data
                .startup_items
                .as_ref()
                .map(|l| l.len())
                .unwrap_or(0),
            6 => self
                .data
                .protect_findings
                .as_ref()
                .map(|l| l.len())
                .unwrap_or(0),
            7 => self
                .data
                .privacy_findings
                .as_ref()
                .map(|l| l.len())
                .unwrap_or(0),
            8 => self
                .data
                .maintenance_tasks
                .as_ref()
                .map(|l| l.len())
                .unwrap_or(0),
            _ => 0,
        }
    }

    fn next_detail(&mut self) {
        let len = self.get_current_detail_len();
        if len == 0 {
            return;
        }
        let i = match self.detail_list_state.selected() {
            Some(i) => {
                if i >= len - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.detail_list_state.select(Some(i));
    }

    fn previous_detail(&mut self) {
        let len = self.get_current_detail_len();
        if len == 0 {
            return;
        }
        let i = match self.detail_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    len - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.detail_list_state.select(Some(i));
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

    let sidebar_border_color = if let TuiView::Sidebar = state.current_view {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let sidebar = List::new(items)
        .block(
            Block::default()
                .title(" Modules ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(sidebar_border_color)),
        )
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

    let detail_border_color = if let TuiView::Detail = state.current_view {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let detail_block = Block::default()
        .title(format!(" {} Details ", selected_module.name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(detail_border_color));

    if let TuiView::Detail = state.current_view {
        // Render scrollable details list
        let list_data = match selected_idx {
            1 => state.data.cleanup_findings.as_ref(),
            4 => state.data.apps.as_ref(),
            5 => state.data.startup_items.as_ref(),
            6 => state.data.protect_findings.as_ref(),
            7 => state.data.privacy_findings.as_ref(),
            8 => state.data.maintenance_tasks.as_ref(),
            _ => None,
        };

        if let Some(arr) = list_data {
            if arr.is_empty() {
                let placeholder = Paragraph::new("No items found or scanned in this module.")
                    .block(detail_block)
                    .fg(Color::DarkGray);
                f.render_widget(placeholder, body_chunks[1]);
            } else {
                let detail_items: Vec<ListItem> = arr
                    .iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        let bullet = if state.detail_list_state.selected() == Some(idx) {
                            "▶ "
                        } else {
                            "  "
                        };
                        ListItem::new(vec![
                            Line::from(vec![
                                Span::raw(bullet).fg(Color::Cyan),
                                Span::raw(&item.title).bold(),
                                Span::raw("  ").fg(Color::DarkGray),
                                Span::raw(&item.meta).italic().fg(Color::Cyan),
                            ]),
                            Line::from(vec![
                                Span::raw("    ").fg(Color::DarkGray),
                                Span::raw(&item.subtitle).fg(Color::DarkGray),
                            ]),
                        ])
                    })
                    .collect();

                let details_list = List::new(detail_items)
                    .block(detail_block)
                    .highlight_style(Style::default().bg(Color::Rgb(30, 30, 45)));

                let mut det_state = state.detail_list_state;
                f.render_stateful_widget(details_list, body_chunks[1], &mut det_state);
            }
        } else {
            let placeholder = Paragraph::new("Module data is currently unavailable.")
                .block(detail_block)
                .fg(Color::Red);
            f.render_widget(placeholder, body_chunks[1]);
        }
    } else {
        // Render summary page/overview text
        let mut details_text = match selected_idx {
            0 => {
                let mut overview_lines = vec![
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
                ];

                if let Some(ref sum) = state.data.status_summary {
                    overview_lines.push(Line::from(""));
                    overview_lines.push(Line::from("System Storage Status:".bold()));
                    overview_lines.push(Line::from(vec![
                        Span::raw("  Sampled files count: ").fg(Color::DarkGray),
                        Span::raw(sum.home_summary.sampled_file_count.to_string()),
                    ]));
                    overview_lines.push(Line::from(vec![
                        Span::raw("  Sampled directories count: ").fg(Color::DarkGray),
                        Span::raw(sum.home_summary.sampled_dir_count.to_string()),
                    ]));
                    overview_lines.push(Line::from(vec![
                        Span::raw("  Sampled home size: ").fg(Color::DarkGray),
                        Span::raw(format!("{} bytes", sum.home_summary.sampled_size_bytes)),
                    ]));
                }

                overview_lines.push(Line::from(""));
                overview_lines.push(Line::from(
                    "Use [Up/Down] or [j/k] to navigate through the left sidebar.",
                ));
                overview_lines.push(Line::from(
                    "Press [q] or [Esc] to exit the TUI dashboard at any time.",
                ));

                overview_lines
            }
            1 => {
                let count_str = state
                    .data
                    .cleanup_findings
                    .as_ref()
                    .map(|c| c.len().to_string())
                    .unwrap_or_else(|| "Unavailable".to_string());
                let size_str = state
                    .data
                    .cleanup_findings_size
                    .map(|s| format!("{s} bytes"))
                    .unwrap_or_else(|| "Unavailable".to_string());
                vec![
                    Line::from("Cleanup Module (Read-Only)".bold().fg(Color::Cyan)),
                    Line::from(""),
                    Line::from("Identifies safe-to-remove files including:"),
                    Line::from("- System and User Cache files"),
                    Line::from("- System Logs and Diagnostic reports"),
                    Line::from("- Temporary directory files"),
                    Line::from("- Xcode derived data and build artifacts"),
                    Line::from(""),
                    Line::from("Live Statistics (Safe pre-scan):".bold()),
                    Line::from(vec![
                        Span::raw("  Safe candidate count: ").fg(Color::DarkGray),
                        Span::raw(count_str),
                    ]),
                    Line::from(vec![
                        Span::raw("  Reclaimable size: ").fg(Color::DarkGray),
                        Span::raw(size_str),
                    ]),
                    Line::from(""),
                    Line::from("Press [Enter] to inspect safe files catalog list.".fg(Color::Cyan)),
                ]
            }
            2 => {
                vec![
                    Line::from("Clutter Module (Read-Only)".bold().fg(Color::Cyan)),
                    Line::from(""),
                    Line::from("Scans user directories (e.g., ~/Downloads) for large, forgotten files, installer archives (.dmg, .pkg), and download clutter exceeding the target size threshold."),
                    Line::from(""),
                    Line::from("Note: Full scan triggers are not wired in this dashboard view yet."),
                ]
            }
            3 => {
                vec![
                    Line::from("Disk Module (Read-Only)".bold().fg(Color::Cyan)),
                    Line::from(""),
                    Line::from("Traverses and catalogs directories to build an interactive, sorted breakdown of the largest folders and files consuming space on your system."),
                    Line::from(""),
                    Line::from("Note: Storage mapping triggers are not wired in this dashboard view yet."),
                ]
            }
            4 => {
                let count_str = state
                    .data
                    .apps
                    .as_ref()
                    .map(|c| c.len().to_string())
                    .unwrap_or_else(|| "Unavailable".to_string());
                vec![
                    Line::from("Apps Module (Read-Only)".bold().fg(Color::Cyan)),
                    Line::from(""),
                    Line::from("Scans /Applications and ~/Applications to report bundle metadata and identify orphaned application support, cache, and preference files left behind from previous uninstalls."),
                    Line::from(""),
                    Line::from("Live Statistics:".bold()),
                    Line::from(vec![
                        Span::raw("  Installed applications detected: ").fg(Color::DarkGray),
                        Span::raw(count_str),
                    ]),
                    Line::from(""),
                    Line::from("Press [Enter] to inspect applications inventory list.".fg(Color::Cyan)),
                ]
            }
            5 => {
                let count_str = state
                    .data
                    .startup_items
                    .as_ref()
                    .map(|c| c.len().to_string())
                    .unwrap_or_else(|| "Unavailable".to_string());
                vec![
                    Line::from("Startup Module (Read-Only)".bold().fg(Color::Cyan)),
                    Line::from(""),
                    Line::from("Catalogs and parses plist metadata for all active user and system LaunchAgents/LaunchDaemons. Highlights missing executables and malformed startup configurations."),
                    Line::from(""),
                    Line::from("Live Statistics:".bold()),
                    Line::from(vec![
                        Span::raw("  Startup services detected: ").fg(Color::DarkGray),
                        Span::raw(count_str),
                    ]),
                    Line::from(""),
                    Line::from("Press [Enter] to inspect LaunchAgents/LaunchDaemons list.".fg(Color::Cyan)),
                ]
            }
            6 => {
                let count_str = state
                    .data
                    .protect_findings
                    .as_ref()
                    .map(|c| c.len().to_string())
                    .unwrap_or_else(|| "Unavailable".to_string());
                let is_zero = count_str == "0" || count_str == "Unavailable";
                vec![
                    Line::from("Protect Module (Read-Only)".bold().fg(Color::Cyan)),
                    Line::from(""),
                    Line::from("Analyzes startup plists and binaries for security/persistence risks. Flags items executing dynamic shell commands or utilizing temp/network execution paths."),
                    Line::from(""),
                    Line::from("Live Statistics:".bold()),
                    Line::from(vec![
                        Span::raw("  Security/persistence risk alerts: ").fg(Color::DarkGray),
                        Span::raw(count_str).fg(if is_zero {
                            Color::Green
                        } else {
                            Color::Red
                        }),
                    ]),
                    Line::from(""),
                    Line::from("Press [Enter] to inspect security alerts catalog.".fg(Color::Cyan)),
                ]
            }
            7 => {
                let count_str = state
                    .data
                    .privacy_findings
                    .as_ref()
                    .map(|c| c.len().to_string())
                    .unwrap_or_else(|| "Unavailable".to_string());
                vec![
                    Line::from("Privacy Module (Read-Only)".bold().fg(Color::Cyan)),
                    Line::from(""),
                    Line::from("Identifies browser caches (Safari, Chrome, Firefox), recent item plist databases, QuickLook thumbnail caches, and shell history presence without reading personal command history."),
                    Line::from(""),
                    Line::from("Live Statistics:".bold()),
                    Line::from(vec![
                        Span::raw("  Privacy-sensitive paths identified: ").fg(Color::DarkGray),
                        Span::raw(count_str),
                    ]),
                    Line::from(""),
                    Line::from("Press [Enter] to inspect privacy artifacts list.".fg(Color::Cyan)),
                ]
            }
            8 => {
                let count_str = state
                    .data
                    .maintenance_tasks
                    .as_ref()
                    .map(|c| c.len().to_string())
                    .unwrap_or_else(|| "Unavailable".to_string());
                vec![
                    Line::from("Maintenance Module (Read-Only)".bold().fg(Color::Cyan)),
                    Line::from(""),
                    Line::from("Lists available system maintenance tasks (e.g., Flush DNS, Rebuild Spotlight, Thin Time Machine Snapshots) and performs dry-run verification checks."),
                    Line::from(""),
                    Line::from("Live Statistics:".bold()),
                    Line::from(vec![
                        Span::raw("  Supported maintenance tasks: ").fg(Color::DarkGray),
                        Span::raw(count_str),
                    ]),
                    Line::from(""),
                    Line::from("Press [Enter] to inspect available maintenance tasks.".fg(Color::Cyan)),
                ]
            }
            _ => vec![],
        };

        // If there are warnings, append them at the bottom
        if !state.data.warnings.is_empty() {
            details_text.push(Line::from(""));
            details_text.push(Line::from("Warnings:".bold().fg(Color::Yellow)));
            for warning in &state.data.warnings {
                details_text.push(Line::from(vec![
                    Span::raw("  ⚠ ").fg(Color::Yellow),
                    Span::raw(warning).fg(Color::Yellow),
                ]));
            }
        }

        let detail_panel = Paragraph::new(details_text)
            .block(detail_block)
            .wrap(ratatui::widgets::Wrap { trim: true });
        f.render_widget(detail_panel, body_chunks[1]);
    }

    // 3. Footer
    let footer_text = match state.current_view {
        TuiView::Sidebar => vec![Line::from(vec![
            Span::raw("  [q/Esc] Quit  ").bold().fg(Color::Red),
            Span::raw("  [↑/↓/j/k] Navigate  ").bold().fg(Color::Cyan),
            Span::raw("  [Enter] Details  ").bold().fg(Color::Cyan),
        ])],
        TuiView::Detail => vec![Line::from(vec![
            Span::raw("  [q/Esc] Quit  ").bold().fg(Color::Red),
            Span::raw("  [↑/↓/j/k] Scroll  ").bold().fg(Color::Cyan),
            Span::raw("  [Backspace] Back  ").bold().fg(Color::Cyan),
        ])],
    };

    let footer = Paragraph::new(footer_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(footer, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn test_tui_data_load_from_fixture() {
        let base = std::env::temp_dir().join(format!("macmop-test-tui-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&base);
        std::env::set_var("MACMOP_TEST_MODE", "1");
        std::env::set_var("MACMOP_HOME", base.to_str().unwrap());
        std::env::set_var("MACMOP_DATA_DIR", base.join(".macmop").to_str().unwrap());
        std::env::set_var("MACMOP_TRASH_DIR", base.join(".Trash").to_str().unwrap());
        std::env::set_var(
            "MACMOP_AUDIT_FILE",
            base.join("audit.json").to_str().unwrap(),
        );
        std::env::set_var(
            "MACMOP_ROLLBACK_FILE",
            base.join("rollback.json").to_str().unwrap(),
        );

        let ctx = AppContext::load(
            None,
            crate::core::ExecutionMode::DryRun,
            crate::core::OutputFormat::Json,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        let data = TuiData::load(&ctx);
        assert!(data.status_summary.is_some());
        // Verify lists load successfully
        assert!(data.apps.is_some());
        assert!(data.startup_items.is_some());
        assert!(data.protect_findings.is_some());
        assert!(data.privacy_findings.is_some());
        assert!(data.maintenance_tasks.is_some());

        // Verify capping (TUI_DETAIL_LIMIT)
        if let Some(ref list) = data.apps {
            assert!(list.len() <= TUI_DETAIL_LIMIT);
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_tui_data_load_creates_no_audit_or_rollback_files() {
        let base =
            std::env::temp_dir().join(format!("macmop-test-tui-regression-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&base);
        std::env::set_var("MACMOP_TEST_MODE", "1");
        std::env::set_var("MACMOP_HOME", base.to_str().unwrap());
        std::env::set_var("MACMOP_DATA_DIR", base.join(".macmop").to_str().unwrap());
        std::env::set_var("MACMOP_TRASH_DIR", base.join(".Trash").to_str().unwrap());
        std::env::set_var(
            "MACMOP_AUDIT_FILE",
            base.join("audit.json").to_str().unwrap(),
        );
        std::env::set_var(
            "MACMOP_ROLLBACK_FILE",
            base.join("rollback.json").to_str().unwrap(),
        );

        let ctx = AppContext::load(
            None,
            crate::core::ExecutionMode::DryRun,
            crate::core::OutputFormat::Json,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        // Ensure audit and rollback files do not exist initially
        assert!(!ctx.paths.audit_file.exists());
        assert!(!ctx.paths.rollback_file.exists());

        // Perform loading
        let _data = TuiData::load(&ctx);

        // Assert no audit/rollback files were created during load
        assert!(!ctx.paths.audit_file.exists());
        assert!(!ctx.paths.rollback_file.exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_tui_state_navigation_transitions() {
        let data = TuiData {
            status_summary: None,
            cleanup_findings: Some(vec![TuiDetailItem {
                title: "a".to_string(),
                subtitle: "b".to_string(),
                meta: "c".to_string(),
            }]),
            cleanup_findings_size: None,
            apps: Some(vec![]),
            startup_items: None,
            protect_findings: None,
            privacy_findings: None,
            maintenance_tasks: None,
            warnings: vec![],
        };
        let mut state = TuiState::new(data);

        // Default state
        assert_eq!(state.current_view, TuiView::Sidebar);
        assert_eq!(state.list_state.selected(), Some(0));

        // Navigate to Cleanup (index 1)
        state.next_sidebar();
        assert_eq!(state.list_state.selected(), Some(1));

        // Enter detail view
        state.enter_detail();
        assert_eq!(state.current_view, TuiView::Detail);
        assert_eq!(state.detail_list_state.selected(), Some(0));

        // Go back to sidebar
        state.back_to_sidebar();
        assert_eq!(state.current_view, TuiView::Sidebar);
    }
}
