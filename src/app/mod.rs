pub mod state;
pub mod ui;
pub mod actions;

use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::sync::mpsc;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, ExecutableCommand};
use reqwest::blocking::Client;
use ratatui::{backend::CrosstermBackend, Terminal};
use sysinfo::System;

use crate::catalog::{load_catalog, CatalogFile};
use crate::distro::{detect_distro, DistroInfo};
use crate::resolver::ResolvedAsset;

pub use state::{ProgressInfo, ToolItem, ViewState, InstallMsg};

pub struct App {
    pub(crate) catalog: CatalogFile,
    pub(crate) distro: DistroInfo,
    pub(crate) client: Client,
    pub(crate) tools: Vec<ToolItem>,
    pub(crate) cursor: usize,
    pub(crate) logs: Vec<String>,
    pub(crate) dry_run: bool,
    pub(crate) progress: ProgressInfo,
    pub(crate) state: ViewState,
    pub(crate) sys: System,
    pub(crate) resolution_rx: Option<mpsc::Receiver<(String, Result<ResolvedAsset, String>)>>,
    pub(crate) installation_rx: Option<mpsc::Receiver<InstallMsg>>,
    pub(crate) cancel_tx: Option<mpsc::Sender<()>>,
    pub(crate) install_start: Option<Instant>,
    pub(crate) is_resolving: bool,
}

impl App {
    pub fn new() -> Result<Self, String> {
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let catalog_path: PathBuf = root.join("software_catalog.toml");
        let catalog = load_catalog(&catalog_path).map_err(|e| e.to_string())?;
        let distro = detect_distro().map_err(|e| e.to_string())?;
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("rusty_rebase/0.1")
            .build()
            .map_err(|e| e.to_string())?;

        let tools = catalog
            .software
            .iter()
            .map(|(key, spec)| ToolItem {
                key: key.clone(),
                selected: spec.enabled_by_default,
                resolved: None,
            })
            .collect();

        let mut sys = System::new_all();
        sys.refresh_all();

        Ok(Self {
            catalog,
            distro,
            client,
            tools,
            cursor: 0,
            logs: vec!["Ready. Press 'r' to resolve versions or 'i' to install selected tools.".to_string()],
            dry_run: true,
            progress: ProgressInfo::default(),
            state: ViewState::Browsing,
            sys,
            resolution_rx: None,
            installation_rx: None,
            cancel_tx: None,
            install_start: None,
            is_resolving: false,
        })
    }

    pub fn run(&mut self) -> Result<(), String> {
        if let Err(e) = enable_raw_mode() {
            return Err(format!("failed to enable raw mode: {e}"));
        }
        let mut stdout = std::io::stdout();
        if let Err(e) = execute!(stdout, EnterAlternateScreen) {
            return Err(format!("failed to enter alternate screen: {e}"));
        }

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = match Terminal::new(backend) {
            Ok(t) => t,
            Err(e) => return Err(format!("failed to create terminal: {e}")),
        };

        let result = self.event_loop(&mut terminal);

        disable_raw_mode().ok();
        terminal.backend_mut().execute(LeaveAlternateScreen).ok();
        terminal.show_cursor().ok();

        result
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<(), String> {
        loop {
            self.sys.refresh_cpu_all();
            self.sys.refresh_memory();

            if let Some(ref rx) = self.resolution_rx {
                while let Ok((key, result)) = rx.try_recv() {
                    match result {
                        Ok(asset) => {
                            self.logs.push(format!("[done] Resolved {} to {}", key, asset.version));
                            if let Some(tool) = self.tools.iter_mut().find(|t| t.key == key) {
                                tool.resolved = Some(asset);
                            }
                        }
                        Err(err) => {
                            self.logs.push(format!("[error] Failed to resolve {}: {}", key, err));
                        }
                    }
                    self.progress.done += 1;
                }
                
                if self.progress.done >= self.progress.total {
                    self.resolution_rx = None;
                    self.is_resolving = false;
                    self.progress.current = "Resolution complete".to_string();
                }
            }

            let mut finished = false;
            if let Some(ref rx) = self.installation_rx {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        InstallMsg::Progress(key, op, speed) => {
                            self.progress.current = key;
                            self.progress.operation = op;
                            self.progress.speed = speed;
                        }
                        InstallMsg::SubProgress(ratio) => {
                            self.progress.sub_ratio = ratio;
                        }
                        InstallMsg::Log(log) => {
                            self.logs.push(log.clone());
                            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open("rusty_rebase_install.log") {
                                use std::io::Write;
                                let _ = writeln!(file, "{}", log);
                            }
                        }
                        InstallMsg::Done(key, result) => {
                            self.progress.done_items.push(key.clone());
                            match result {
                                Ok(logs) => {
                                    for log in &logs {
                                        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open("rusty_rebase_install.log") {
                                            use std::io::Write;
                                            let _ = writeln!(file, "{}", log);
                                        }
                                    }
                                    self.logs.extend(logs);
                                    self.progress.succeeded += 1;
                                }
                                Err(err) => {
                                    let msg = format!("[error] {} failed: {}", key, err);
                                    self.logs.push(msg.clone());
                                    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open("rusty_rebase_install.log") {
                                        use std::io::Write;
                                        let _ = writeln!(file, "{}", msg);
                                    }
                                    self.progress.failed += 1;
                                }
                            }
                            self.progress.done += 1;
                            self.progress.sub_ratio = 0.0;

                            if let Some(start) = self.install_start {
                                let elapsed = start.elapsed().as_secs_f64();
                                if self.progress.done > 0 {
                                    let time_per_item = elapsed / self.progress.done as f64;
                                    let remaining_items = self.progress.total.saturating_sub(self.progress.done);
                                    let eta_secs = (remaining_items as f64 * time_per_item) as u64;
                                    
                                    if eta_secs > 0 {
                                        let mins = eta_secs / 60;
                                        let secs = eta_secs % 60;
                                        self.progress.eta = Some(if mins > 0 {
                                            format!("~{}m {}s", mins, secs)
                                        } else {
                                            format!("~{}s", secs)
                                        });
                                    } else {
                                        self.progress.eta = Some("finishing...".to_string());
                                    }
                                }
                            }
                        }
                        InstallMsg::Finished => {
                            self.state = ViewState::Completed;
                            finished = true;
                            self.progress.eta = None;
                        }
                    }
                }
            }
            if finished {
                self.installation_rx = None;
                self.cancel_tx = None;
            }

            if let Err(e) = terminal.draw(|f| ui::render(self, f)) {
                return Err(format!("failed to draw frame: {e}"));
            }

            match event::poll(Duration::from_millis(200)) {
                Ok(true) => {
                    let key_event = match event::read() {
                        Ok(Event::Key(k)) => k,
                        Ok(_) => continue,
                        Err(e) => return Err(format!("failed to read event: {e}")),
                    };

                    match key_event.code {
                        KeyCode::Char('q') => {
                            if self.state == ViewState::Installing || self.state == ViewState::Restoring {
                                if let Some(ref tx) = self.cancel_tx {
                                    let _ = tx.send(());
                                    self.logs.push("[User] Process cancelled. Waiting to abort...".to_string());
                                }
                            } else {
                                break;
                            }
                        }
                        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                            if let Some(ref tx) = self.cancel_tx {
                                let _ = tx.send(());
                            }
                            break;
                        }
                        KeyCode::Esc => {
                            if self.state == ViewState::Completed {
                                self.state = ViewState::Browsing;
                                self.progress = ProgressInfo::default();
                                self.logs.push("Returned to browsing. Select more tools or resolve again.".to_string());
                            } else if let ViewState::FilePicker { .. } = self.state {
                                self.state = ViewState::Browsing;
                                self.logs.push("File picker cancelled.".to_string());
                            }
                        }
                        KeyCode::Enter => {
                            if self.state == ViewState::Completed {
                                self.state = ViewState::Browsing;
                                self.progress = ProgressInfo::default();
                                self.logs.push("Returned to browsing. Select more tools or resolve again.".to_string());
                            } else if let ViewState::FilePicker { ref mut current_dir, ref mut entries, ref mut cursor } = self.state.clone() {
                                if let Some(path) = entries.get(*cursor) {
                                    if path.file_name().unwrap_or_default().is_empty() {
                                        if let Some(parent) = current_dir.parent() {
                                            actions::update_file_picker(self, parent.to_path_buf());
                                        }
                                    } else if path.is_dir() {
                                        actions::update_file_picker(self, path.clone());
                                    } else if path.is_file() && path.extension().map_or(false, |e| e == "json") {
                                        actions::start_restore_from_file(self, path.clone());
                                    } else {
                                        self.logs.push("Please select a JSON metadata file or a folder.".to_string());
                                    }
                                }
                            }
                        }
                        KeyCode::Down => {
                            if let ViewState::FilePicker { ref mut cursor, ref entries, .. } = self.state {
                                if *cursor + 1 < entries.len() { *cursor += 1; }
                            } else if self.state == ViewState::Browsing && self.cursor + 1 < self.tools.len() {
                                self.cursor += 1;
                            }
                        }
                        KeyCode::Up => {
                            if let ViewState::FilePicker { ref mut cursor, .. } = self.state {
                                if *cursor > 0 { *cursor -= 1; }
                            } else if self.state == ViewState::Browsing && self.cursor > 0 {
                                self.cursor -= 1;
                            }
                        }
                        KeyCode::Char(' ') => {
                            if let Some(item) = self.tools.get_mut(self.cursor) {
                                item.selected = !item.selected;
                            }
                        }
                        KeyCode::Char('a') => {
                            for item in &mut self.tools {
                                item.selected = true;
                            }
                        }
                        KeyCode::Char('n') => {
                            for item in &mut self.tools {
                                item.selected = false;
                            }
                        }
                        KeyCode::Char('d') => {
                            self.dry_run = !self.dry_run;
                            self.logs.push(format!("dry-run = {}", self.dry_run));
                        }
                        KeyCode::Char('r') => {
                            actions::start_resolution(self);
                        }
                        KeyCode::Char('u') => {
                            if self.state == ViewState::Browsing {
                                actions::update_file_picker(self, std::env::current_dir().unwrap_or_default());
                            }
                        }
                        KeyCode::Char('i') => {
                            if !self.dry_run {
                                disable_raw_mode().ok();
                                std::io::stdout().execute(LeaveAlternateScreen).ok();
                                println!("\n[Sudo] Authenticating for system installation...");
                                let _ = std::process::Command::new("sudo").arg("-v").status();
                                std::io::stdout().execute(EnterAlternateScreen).ok();
                                enable_raw_mode().ok();
                                terminal.clear().ok();
                                terminal.hide_cursor().ok();
                            }
                            actions::install_selected(self)
                        }
                        KeyCode::Char('c') => {
                            if self.state == ViewState::Installing {
                                if let Some(ref tx) = self.cancel_tx {
                                    let _ = tx.send(());
                                    self.logs.push("[User] Cancellation signal sent...".to_string());
                                }
                            } else {
                                self.logs.clear();
                            }
                        }
                        _ => {}
                    }
                }
                Ok(false) => {}
                Err(e) => return Err(format!("event poll failed: {e}")),
            }
        }
        Ok(())
    }
}
