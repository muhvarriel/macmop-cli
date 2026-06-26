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

pub struct TuiData {
    pub status_summary: Option<StatusSummary>,
    pub cleanup_findings_count: Option<usize>,
    pub cleanup_findings_size: Option<u64>,
    pub app_count: Option<usize>,
    pub startup_item_count: Option<usize>,
    pub protect_finding_count: Option<usize>,
    pub privacy_artifact_count: Option<usize>,
    pub maintenance_task_count: Option<usize>,
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
        let (cleanup_findings_count, cleanup_findings_size) = match cleanup::run(
            ctx,
            CleanupArgs {
                category: vec!["logs".into(), "temp".into()],
                older_than_days: 30,
            },
        ) {
            Ok(env) => {
                let count = env
                    .payload
                    .get("findings")
                    .and_then(|f| f.as_array().map(|a| a.len()));
                let size = env
                    .payload
                    .get("action_plan")
                    .and_then(|p| p.get("total_size_bytes").and_then(|s| s.as_u64()));
                (count, size)
            }
            Err(e) => {
                warnings.push(format!("Failed to scan cleanup items: {e}"));
                (None, None)
            }
        };

        // 3. Load Apps List
        let app_count = match apps::run(
            ctx,
            AppsArgs {
                command: AppsCommand::List,
            },
        ) {
            Ok(env) => env
                .payload
                .get("items")
                .and_then(|i| i.as_array().map(|a| a.len())),
            Err(e) => {
                warnings.push(format!("Failed to list apps: {e}"));
                None
            }
        };

        // 4. Load Startup Items
        let startup_item_count = match startup::run(
            ctx,
            StartupArgs {
                command: StartupCommand::List,
            },
        ) {
            Ok(env) => env
                .payload
                .get("items")
                .and_then(|i| i.as_array().map(|a| a.len())),
            Err(e) => {
                warnings.push(format!("Failed to list startup items: {e}"));
                None
            }
        };

        // 5. Load Protect Findings
        let protect_finding_count = match protect::run(
            ctx,
            ProtectArgs {
                command: ProtectCommand::Scan,
            },
        ) {
            Ok(env) => env.payload.get("summary").and_then(|s| {
                s.get("finding_count")
                    .and_then(|f| f.as_u64().map(|v| v as usize))
            }),
            Err(e) => {
                warnings.push(format!("Failed to scan protection risks: {e}"));
                None
            }
        };

        // 6. Load Privacy Findings
        let privacy_artifact_count = match privacy::run(
            ctx,
            PrivacyArgs {
                command: PrivacyCommand::Scan,
            },
        ) {
            Ok(env) => env
                .payload
                .get("findings")
                .and_then(|f| f.as_array().map(|a| a.len())),
            Err(e) => {
                warnings.push(format!("Failed to scan privacy items: {e}"));
                None
            }
        };

        // 7. Load Maintenance Tasks
        let maintenance_task_count = match maintenance::run(
            ctx,
            MaintenanceArgs {
                command: MaintenanceCommand::List,
            },
        ) {
            Ok(env) => env.payload.get("summary").and_then(|s| {
                s.get("task_count")
                    .and_then(|t| t.as_u64().map(|v| v as usize))
            }),
            Err(e) => {
                warnings.push(format!("Failed to list maintenance tasks: {e}"));
                None
            }
        };

        Self {
            status_summary,
            cleanup_findings_count,
            cleanup_findings_size,
            app_count,
            startup_item_count,
            protect_finding_count,
            privacy_artifact_count,
            maintenance_task_count,
            warnings,
        }
    }
}

struct TuiState {
    modules: Vec<TuiModule>,
    list_state: ListState,
    data: TuiData,
}

struct TuiModule {
    name: &'static str,
    description: &'static str,
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

        Self {
            modules,
            list_state,
            data,
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

    let mut details_text = match selected_idx {
        0 => {
            // Overview page
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
            // Cleanup page
            let count_str = state
                .data
                .cleanup_findings_count
                .map(|c| c.to_string())
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
            ]
        }
        2 => {
            // Clutter page
            vec![
                Line::from("Clutter Module (Read-Only)".bold().fg(Color::Cyan)),
                Line::from(""),
                Line::from("Scans user directories (e.g., ~/Downloads) for large, forgotten files, installer archives (.dmg, .pkg), and download clutter exceeding the target size threshold."),
                Line::from(""),
                Line::from("Note: Full scan triggers are not wired in this dashboard view yet."),
            ]
        }
        3 => {
            // Disk page
            vec![
                Line::from("Disk Module (Read-Only)".bold().fg(Color::Cyan)),
                Line::from(""),
                Line::from("Traverses and catalogs directories to build an interactive, sorted breakdown of the largest folders and files consuming space on your system."),
                Line::from(""),
                Line::from("Note: Storage mapping triggers are not wired in this dashboard view yet."),
            ]
        }
        4 => {
            // Apps page
            let count_str = state
                .data
                .app_count
                .map(|c| c.to_string())
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
            ]
        }
        5 => {
            // Startup page
            let count_str = state
                .data
                .startup_item_count
                .map(|c| c.to_string())
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
            ]
        }
        6 => {
            // Protect page
            let count_str = state
                .data
                .protect_finding_count
                .map(|c| c.to_string())
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
            ]
        }
        7 => {
            // Privacy page
            let count_str = state
                .data
                .privacy_artifact_count
                .map(|c| c.to_string())
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
            ]
        }
        8 => {
            // Maintenance page
            let count_str = state
                .data
                .maintenance_task_count
                .map(|c| c.to_string())
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

        let ctx = AppContext::load(
            None,
            crate::core::ExecutionMode::DryRun,
            crate::core::OutputFormat::Json,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        let data = TuiData::load(&ctx);
        assert!(data.status_summary.is_some());
        // Verify no crash on missing directories (e.g. startup_dirs)
        assert!(data.app_count.is_some());
        assert!(data.startup_item_count.is_some());
        let _ = std::fs::remove_dir_all(&base);
    }
}
