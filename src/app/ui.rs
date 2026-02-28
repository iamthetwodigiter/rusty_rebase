use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Gauge, Wrap};
use ratatui::Frame;
use crate::app::{App, ViewState};

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    render_header(app, frame, main_layout[0]);
    render_body(app, frame, main_layout[1]);
    render_footer(app, frame, main_layout[2]);
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(area);

    let ascii = vec![
        " ██████╗ ██╗   ██╗███████╗████████╗██╗   ██╗    ██████╗ ███████╗██████╗  █████╗ ███████╗███████╗",
        " ██╔══██╗██║   ██║██╔════╝╚══██╔══╝╚██╗ ██╔╝    ██╔══██╗██╔════╝██╔══██╗██╔══██╗██╔════╝██╔════╝",
        " ██████╔╝██║   ██║███████╗   ██║    ╚████╔╝     ██████╔╝█████╗  ██████╔╝███████║███████╗█████╗  ",
        " ██╔══██╗██║   ██║╚════██║   ██║     ╚██╔╝      ██╔══██╗██╔════╝██╔══██╗██╔══██║╚════██║██╔════╝",
        " ██║  ██║╚██████╔╝███████║   ██║      ██║       ██║  ██║███████╗██████╔╝██║  ██║███████║███████╗",
        " ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝      ╚═╝       ╚═╝  ╚═╝╚══════╝╚═════╝ ╚═╝  ╚═╝╚══════╝╚══════╝",
    ];
    let banner: Vec<Line> = ascii.into_iter().map(|l| Line::from(Span::styled(l, Style::default().fg(Color::Cyan)))).collect();
    frame.render_widget(Paragraph::new(banner), chunks[0]);

    let stats_block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray))
        .padding(ratatui::widgets::Padding::horizontal(1));
    let stats_inner = stats_block.inner(chunks[1]);
    frame.render_widget(stats_block, chunks[1]);

    let cpu_use = app.sys.global_cpu_usage();
    let total_mem = app.sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let used_mem = app.sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let mem_percent = (used_mem / total_mem * 100.0) as u16;
    
    let stats_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .split(stats_inner);

    let cpu_gauge = Gauge::default()
        .block(Block::default().title(" CPU ").title_style(Style::default().fg(Color::Gray)))
        .gauge_style(Style::default().fg(Color::Magenta))
        .percent(cpu_use as u16)
        .label(format!("{:.1}%", cpu_use));
    frame.render_widget(cpu_gauge, stats_layout[0]);

    let mem_gauge = Gauge::default()
        .block(Block::default().title(" RAM ").title_style(Style::default().fg(Color::Gray)))
        .gauge_style(Style::default().fg(Color::Yellow))
        .percent(mem_percent)
        .label(format!("{:.1} / {:.1} GB", used_mem, total_mem));
    frame.render_widget(mem_gauge, stats_layout[1]);

    let distro_info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(" OS: ", Style::default().fg(Color::Gray)),
            Span::styled(&app.distro.id, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" | PACKAGE-MANAGER: ", Style::default().fg(Color::Gray)),
            Span::styled(app.distro.pkg_manager.to_string(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" | DRY-RUN: ", Style::default().fg(Color::Gray)),
            Span::styled(if app.dry_run { "ON" } else { "OFF" }, Style::default().fg(if app.dry_run { Color::Yellow } else { Color::Green }).add_modifier(Modifier::BOLD)),
        ])
    ]);
    frame.render_widget(distro_info, stats_layout[2]);
}

fn render_body(app: &App, frame: &mut Frame, area: Rect) {
    match app.state {
        ViewState::Browsing => render_browsing(app, frame, area),
        ViewState::Installing | ViewState::Completed => render_progress(app, frame, area),
    }
}

pub fn render_logs(app: &App, frame: &mut Frame, area: Rect, title: &str, border_color: Color) {
    let logs: Vec<ListItem> = app.logs.iter().rev().take(area.height as usize).map(|l| {
        let color = if l.contains("[error]") || l.contains("failed") || l.contains("Error") { Color::Red }
                    else if l.contains("[done]") || l.contains("succeeded") || l.contains("status 0") { Color::Green }
                    else if l.contains("[resolve]") || l.starts_with("==") { Color::Cyan }
                    else { Color::Gray };
        ListItem::new(Line::from(Span::styled(l, Style::default().fg(color))))
    }).collect();

    let logs_list = List::new(logs)
        .block(Block::default().borders(Borders::ALL).title(format!("  {}  ", title)).border_style(Style::default().fg(border_color)));
    frame.render_widget(logs_list, area);
}

fn render_browsing(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(75),
        ])
        .split(area);

    let items: Vec<ListItem> = app.tools.iter().enumerate().map(|(idx, tool)| {
        let spec = app.catalog.software.get(&tool.key);
        let name = spec.map(|s| s.display_name.as_str()).unwrap_or(&tool.key);
        
        let is_cursor = idx == app.cursor;
        let symbol = if tool.selected { "[x] " } else { "[ ] " };
        let base_style = if tool.selected { Style::default().fg(Color::Green) } else { Style::default().fg(Color::White) };
        let final_style = if is_cursor { base_style.bg(Color::Rgb(40, 40, 40)).add_modifier(Modifier::BOLD).fg(Color::Blue) } else { base_style };

        ListItem::new(vec![
            Line::from(vec![Span::styled(symbol, final_style), Span::styled(name, final_style)]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    tool.resolved.as_ref().map(|r| r.version.as_str()).unwrap_or("unresolved"),
                    Style::default().fg(if tool.resolved.is_some() { Color::LightCyan } else { Color::DarkGray })
                )
            ])
        ])
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("  Software Catalog  ").border_style(Style::default().fg(Color::Cyan)));
    let mut state = ListState::default();
    state.select(Some(app.cursor));
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .split(chunks[1]);

    if let Some(tool) = app.tools.get(app.cursor) {
        let spec = app.catalog.software.get(&tool.key);
        let name = spec.map(|s| s.display_name.as_str()).unwrap_or(&tool.key);
        let desc = spec.and_then(|s| s.description.as_deref()).unwrap_or("No description available.");
        
        let mut info_text = vec![
            Line::from(vec![Span::styled(" > Download: ", Style::default().fg(Color::Cyan)), Span::styled(name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD))]),
            Line::from(vec![Span::styled(" # Description: ", Style::default().fg(Color::Cyan)), Span::styled(desc, Style::default().fg(Color::Gray))]),
        ];

        if let Some(spec) = spec {
            let readable_source = match spec.source.kind_key() {
                "flutter_latest" => "Official Google Distribution",
                "android_studio_latest" => "Official Android Distribution",
                "vscode_latest" => "Microsoft VS Code Binary",
                "github_latest" => "GitHub Release Asset",
                "package_only" => "Distro Package Manager",
                "static_url" => "Universal Static URL",
                "generic_scraper" => "Web Scraper Resolution",
                _ => spec.source.kind_key(),
            };
            info_text.push(Line::from(vec![Span::styled(" * Source: ", Style::default().fg(Color::Cyan)), Span::styled(readable_source, Style::default().fg(Color::Yellow))]));
            if let Some(dir) = &spec.install_dir {
                info_text.push(Line::from(vec![Span::styled(" @ Path: ", Style::default().fg(Color::Cyan)), Span::styled(dir, Style::default().fg(Color::DarkGray))]));
                info_text.push(Line::from(vec![Span::styled("   (Tip: Edit software_catalog.toml to change this path)", Style::default().fg(Color::Rgb(80, 80, 80)).add_modifier(Modifier::ITALIC))]));
            }
        }

        let info_box = Paragraph::new(info_text)
            .block(Block::default().borders(Borders::ALL).title("  Item Details  ").border_style(Style::default().fg(Color::Cyan)))
            .wrap(Wrap { trim: true });
        frame.render_widget(info_box, right_chunks[0]);

        let mut preview_text = vec![Line::from(Span::styled(" The following actions will be performed:", Style::default().fg(Color::DarkGray)))];
        if let Some(spec) = spec {
            for step in &spec.setup_steps {
                match step {
                    crate::catalog::SetupStep::Package { packages } => {
                        if let Some(cmd) = app.distro.pkg_manager.install_command(packages) {
                            preview_text.push(Line::from(vec![Span::styled(format!("  $ {}", cmd), Style::default().fg(Color::Green))]));
                        }
                    }
                    crate::catalog::SetupStep::Note { value } => {
                        preview_text.push(Line::from(vec![Span::styled(format!("  # Note: {}", value), Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC))]));
                    }
                    crate::catalog::SetupStep::PathHint { value } => {
                        preview_text.push(Line::from(vec![Span::styled(format!("  + Path: {}", value), Style::default().fg(Color::Blue))]));
                    }
                    crate::catalog::SetupStep::Shell { command } => {
                        preview_text.push(Line::from(vec![Span::styled(format!("  $ Shell: {}", command), Style::default().fg(Color::Magenta))]));
                    }
                }
            }
        }
        let preview_box = Paragraph::new(preview_text)
            .block(Block::default().borders(Borders::ALL).title("  Action Preview  ").border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(preview_box, right_chunks[1]);

        render_logs(app, frame, right_chunks[3], "Live Activity", Color::Cyan);

        let guide_text = vec![
            Line::from(vec![Span::styled(" ? Quick Guide", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))]),
            Line::from(vec![
                Span::styled("  [Space] Select ", Style::default().fg(Color::Yellow)), Span::raw("| "),
                Span::styled("[r] Resolve ", Style::default().fg(Color::Yellow)), Span::raw("| "),
                Span::styled("[d] Dry-run ", Style::default().fg(Color::Yellow)), Span::raw("| "),
                Span::styled("[i] Install ", Style::default().fg(Color::Yellow)), Span::raw("| "),
                Span::styled("[c] Clear Logs", Style::default().fg(Color::Yellow)),
            ]),
        ];
        let guide_box = Paragraph::new(guide_text)
            .block(Block::default().borders(Borders::ALL).title("  Usage  ").border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(guide_box, right_chunks[2]);
    }
}

fn render_progress(app: &App, frame: &mut Frame, area: Rect) {
    let top_bottom = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(0)])
        .split(area);

    let bars_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3)])
        .split(top_bottom[0]);

    let total_ratio = if app.progress.total > 0 { 
        (app.progress.done as f64 + app.progress.sub_ratio) / app.progress.total as f64 
    } else { 
        0.0 
    };
    let total_ratio = total_ratio.min(1.0).max(0.0);
    let eta_label = app.progress.eta.as_ref().map(|e| format!(" | ETA: {}", e)).unwrap_or_default();
    let total_label = format!("Total: {:.1}% ({} / {}){}", total_ratio * 100.0, app.progress.done, app.progress.total, eta_label);
    let total_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("  Overall Progress  ").border_style(Style::default().fg(Color::Cyan)))
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black).add_modifier(Modifier::BOLD))
        .ratio(total_ratio)
        .label(total_label);
    frame.render_widget(total_gauge, bars_layout[0]);

    let is_done = app.state == crate::app::ViewState::Completed;
    let sub_ratio = if is_done { 1.0 } else { app.progress.sub_ratio.min(1.0).max(0.0) };
    let sub_label = if is_done { "100.0%".to_string() } else { format!("{:.1}%", sub_ratio * 100.0) };
    let sub_title = if is_done { 
        "  Done  ".to_string() 
    } else { 
        format!("  {} - {}  ", app.progress.operation, app.progress.current) 
    };
    
    let sub_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(sub_title).border_style(Style::default().fg(Color::Green)))
        .gauge_style(Style::default().fg(Color::Green).bg(Color::Black).add_modifier(Modifier::BOLD))
        .ratio(sub_ratio)
        .label(sub_label);
    frame.render_widget(sub_gauge, bars_layout[1]);

    let bottom_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(80), Constraint::Percentage(20)])
        .split(top_bottom[1]);

    render_logs(app, frame, bottom_layout[0], "Terminal Output", Color::Magenta);

    let selected_items: Vec<ListItem> = app.tools.iter().filter(|t| t.selected).map(|tool| {
        let is_done = app.progress.done_items.contains(&tool.key);
        let symbol = if is_done { "[*] " } else { "[ ] " };
        let color = if is_done { Color::Green } else { Color::DarkGray };
        let spec = app.catalog.software.get(&tool.key);
        let name = spec.map(|s| s.display_name.as_str()).unwrap_or(&tool.key);
        ListItem::new(Line::from(vec![
            Span::styled(symbol, Style::default().fg(color)),
            Span::styled(name, Style::default().fg(color)),
        ]))
    }).collect();

    let items_list = List::new(selected_items)
        .block(Block::default().borders(Borders::ALL).title("  Queue  ").border_style(Style::default().fg(Color::Yellow)));
    frame.render_widget(items_list, bottom_layout[1]);
}

fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    let help_lines = match app.state {
        ViewState::Browsing => vec![
            Line::from(vec![
                Span::styled("Keys: ", Style::default().fg(Color::Cyan)),
                Span::raw("Arrows: Move • Space: Select/Deselect • A/N All/None • R: Resolve • I: Install • D: Dry-run • C: Clear • Q: Quit"),
            ]),
            Line::from(vec![
                Span::styled("[Resolve] ", Style::default().fg(Color::Yellow)), Span::raw("Fetch latest metadata from network sources   "),
                Span::styled("[Dry-run] ", Style::default().fg(Color::Yellow)), Span::raw("Preview actions without making system changes"),
            ]),
        ],
        ViewState::Installing => vec![Line::from("installation in progress • please wait...")],
        ViewState::Completed => vec![Line::from("Done! Press [Enter] or [Esc] to return to catalog • [q] to exit")],
    };

    let mut help_para = Paragraph::new(help_lines).alignment(ratatui::layout::Alignment::Center);

    if app.is_resolving {
        help_para = help_para.block(Block::default().title(format!(" [Resolving: {}/{}] ", app.progress.done, app.progress.total)).title_style(Style::default().fg(Color::Cyan)));
    }

    frame.render_widget(help_para, area);
}
