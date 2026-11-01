use crate::app::{App, Mode, Sort, Tab};
use crate::hw::Bus;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, Tabs, Wrap,
    },
    Frame,
};

const CYAN: Color = Color::Rgb(0, 215, 255);
const PINK: Color = Color::Rgb(255, 95, 215);
const VIOLET: Color = Color::Rgb(175, 135, 255);
const GREEN: Color = Color::Rgb(95, 255, 135);
const AMBER: Color = Color::Rgb(255, 200, 87);
const RED: Color = Color::Rgb(255, 95, 110);
const FG: Color = Color::Rgb(215, 220, 240);
const DIM: Color = Color::Rgb(110, 115, 140);
const SEL_BG: Color = Color::Rgb(38, 42, 66);

const MAX_FILE_LINES: usize = 400;

pub fn human(bytes: u64) -> String {
    if bytes == 0 {
        return "—".into();
    }
    let b = bytes as f64;
    if b < 1024.0 * 1024.0 {
        format!("{:.0} KiB", (b / 1024.0).max(1.0))
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MiB", b / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GiB", b / 1024.0 / 1024.0 / 1024.0)
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

fn gradient(text: &str) -> Vec<Span<'static>> {
    let n = text.chars().count().max(2) as f32 - 1.0;
    text.chars()
        .enumerate()
        .map(|(i, c)| {
            let t = i as f32 / n;
            let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
            Span::styled(
                c.to_string(),
                Style::new()
                    .fg(Color::Rgb(lerp(0, 255), lerp(215, 95), lerp(255, 215)))
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect()
}

fn border(color: Color) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(color))
}

fn kv(label: &str, value: String, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::new().fg(DIM)),
        Span::styled(value, Style::new().fg(color)),
    ])
}

fn section(title: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("▍", Style::new().fg(PINK)),
        Span::styled(
            format!(" {title}"),
            Style::new().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn centered(w: u16, h: u16, area: Rect) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .split(area);

    draw_header(f, rows[0], app);
    draw_controls(f, rows[1], app);
    draw_body(f, rows[2], app);
    draw_footer(f, rows[3], app);

    if app.mode == Mode::Find {
        draw_find(f, area, app);
    }
    if app.show_help {
        draw_help(f, area);
    }
}

pub fn draw_splash(f: &mut Frame, msg: &str, frame: usize) {
    let area = f.area();
    let spin = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'][frame % 10];
    let mut title = vec![Span::styled("◆  ", Style::new().fg(CYAN))];
    title.extend(gradient("F i n d T h e P a c k a g e"));
    title.push(Span::styled("  ◆", Style::new().fg(PINK)));
    let lines = vec![
        Line::from(title),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("{spin} "), Style::new().fg(GREEN)),
            Span::styled(msg.to_string(), Style::new().fg(DIM)),
        ]),
    ];
    let r = centered(60, 3, area);
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), r);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let block = border(VIOLET);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mgr = app.manager.map(|m| m.short()).unwrap_or("no package manager");
    let right = format!(
        "{}@{} · kernel {} · {} ",
        app.sys.distro, app.sys.host, app.sys.kernel, mgr
    );
    let rw = (right.chars().count() as u16).min(inner.width / 2);
    let cols = Layout::horizontal([Constraint::Min(10), Constraint::Length(rw)]).split(inner);

    let mut left = vec![Span::styled(" ◆ ", Style::new().fg(CYAN))];
    left.extend(gradient("FindThePackage"));
    left.push(Span::styled(
        "  every component, traced to its package",
        Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
    ));
    f.render_widget(Paragraph::new(Line::from(left)), cols[0]);
    f.render_widget(
        Paragraph::new(Span::styled(trunc(&right, rw as usize), Style::new().fg(DIM)))
            .alignment(Alignment::Right),
        cols[1],
    );
}

fn draw_controls(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::horizontal([Constraint::Length(38), Constraint::Min(10)]).split(area);

    let tabs = Tabs::new(vec![
        format!(" 1 Packages {} ", app.packages.len()),
        format!(" 2 Hardware {} ", app.devices.len()),
    ])
    .select(if app.tab == Tab::Packages { 0 } else { 1 })
    .style(Style::new().fg(DIM))
    .highlight_style(Style::new().fg(Color::Black).bg(CYAN).add_modifier(Modifier::BOLD))
    .divider("│")
    .block(border(DIM));
    f.render_widget(tabs, cols[0]);

    let searching = app.mode == Mode::Search;
    let content = if app.query.is_empty() && !searching {
        Line::from(Span::styled(
            " press / to filter · several words = AND",
            Style::new().fg(DIM),
        ))
    } else {
        let mut v = vec![
            Span::styled(" ", Style::new()),
            Span::styled(app.query.clone(), Style::new().fg(FG).add_modifier(Modifier::BOLD)),
        ];
        if searching {
            v.push(Span::styled("▏", Style::new().fg(PINK).add_modifier(Modifier::SLOW_BLINK)));
        }
        Line::from(v)
    };
    let title = Span::styled(
        " Search ",
        Style::new()
            .fg(if searching { PINK } else { DIM })
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(
        Paragraph::new(content).block(border(if searching { PINK } else { DIM }).title(title)),
        cols[1],
    );
}

fn draw_body(f: &mut Frame, area: Rect, app: &mut App) {
    let cols = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).split(area);
    draw_list(f, cols[0], app);
    draw_details(f, cols[1], app);
}

fn draw_list(f: &mut Frame, area: Rect, app: &mut App) {
    let tab = app.tab;
    let sort = app.sort;
    let shown = app.filtered.len();
    let total = app.total();

    let title = match tab {
        Tab::Packages => format!(
            " Installed packages · {shown}/{total} · sort: {} ",
            if sort == Sort::Name { "name" } else { "size" }
        ),
        Tab::Hardware => format!(" Detected hardware · {shown}/{total} "),
    };
    let block = border(CYAN).title(Span::styled(
        title,
        Style::new().fg(CYAN).add_modifier(Modifier::BOLD),
    ));

    if shown == 0 {
        let msg = if total == 0 {
            "Nothing found here.\n\nIs a supported package manager installed?"
        } else {
            "No matches.\n\nEsc clears the filter."
        };
        f.render_widget(
            Paragraph::new(msg)
                .style(Style::new().fg(DIM))
                .alignment(Alignment::Center)
                .block(block),
            area,
        );
        return;
    }

    let head_style = Style::new().fg(VIOLET).add_modifier(Modifier::BOLD);
    let sel_style = Style::new().bg(SEL_BG).add_modifier(Modifier::BOLD);

    let App { packages, devices, filtered, table, .. } = app;
    let selected = table.selected().unwrap_or(0);

    let widget = match tab {
        Tab::Packages => {
            let rows: Vec<Row> = filtered
                .iter()
                .map(|&i| {
                    let p = &packages[i];
                    Row::new(vec![
                        Cell::from(p.name.as_str()).style(Style::new().fg(FG)),
                        Cell::from(trunc(&p.version, 20)).style(Style::new().fg(DIM)),
                        Cell::from(human(p.size_bytes)).style(Style::new().fg(AMBER)),
                    ])
                })
                .collect();
            Table::new(
                rows,
                [Constraint::Min(18), Constraint::Length(20), Constraint::Length(10)],
            )
            .header(Row::new(vec!["PACKAGE", "VERSION", "SIZE"]).style(head_style))
        }
        Tab::Hardware => {
            let rows: Vec<Row> = filtered
                .iter()
                .map(|&i| {
                    let d = &devices[i];
                    let bus_color = if d.bus == Bus::Pci { CYAN } else { AMBER };
                    let (drv, drv_color) = match &d.driver {
                        Some(x) => (trunc(x, 16), GREEN),
                        None => ("—".to_string(), DIM),
                    };
                    Row::new(vec![
                        Cell::from(d.bus.label()).style(Style::new().fg(bus_color)),
                        Cell::from(d.name.as_str()).style(Style::new().fg(FG)),
                        Cell::from(drv).style(Style::new().fg(drv_color)),
                    ])
                })
                .collect();
            Table::new(
                rows,
                [Constraint::Length(4), Constraint::Min(20), Constraint::Length(16)],
            )
            .header(Row::new(vec!["BUS", "DEVICE", "DRIVER"]).style(head_style))
        }
    }
    .block(block)
    .row_highlight_style(sel_style)
    .highlight_symbol("▌ ");

    f.render_stateful_widget(widget, area, table);

    let mut sb = ScrollbarState::new(shown).position(selected);
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .style(Style::new().fg(DIM)),
        area.inner(Margin { vertical: 1, horizontal: 0 }),
        &mut sb,
    );
}

fn draw_details(f: &mut Frame, area: Rect, app: &mut App) {
    let mut lines: Vec<Line> = vec![];

    if app.manager.is_none() {
        lines.push(Line::from(Span::styled(
            "No supported package manager found (dpkg, rpm, pacman, apk, xbps).",
            Style::new().fg(RED),
        )));
    }

    if let Some(idx) = app.selected() {
        match app.tab {
            Tab::Packages => {
                let p = &app.packages[idx];
                lines.push(Line::from(Span::styled(
                    p.name.clone(),
                    Style::new().fg(GREEN).add_modifier(Modifier::BOLD),
                )));
                if !p.summary.is_empty() {
                    lines.push(Line::from(Span::styled(
                        p.summary.clone(),
                        Style::new().fg(FG).add_modifier(Modifier::ITALIC),
                    )));
                }
                lines.push(Line::raw(""));
                lines.push(kv("Version", p.version.clone(), FG));
                if !p.arch.is_empty() {
                    lines.push(kv("Arch", p.arch.clone(), FG));
                }
                lines.push(kv("Size", human(p.size_bytes), AMBER));
                if let Some(m) = app.manager {
                    lines.push(kv("Manager", m.label().to_string(), DIM));
                }

                if let Some(files) = app.files_cache.get(&p.name) {
                    let bins: Vec<&str> = files
                        .iter()
                        .filter(|x| {
                            ["/usr/bin/", "/bin/", "/usr/sbin/", "/sbin/", "/usr/local/bin/"]
                                .iter()
                                .any(|pre| x.starts_with(pre))
                        })
                        .filter_map(|x| x.rsplit('/').next())
                        .take(60)
                        .collect();
                    if !bins.is_empty() {
                        lines.push(Line::raw(""));
                        lines.push(section(&format!("Commands ({})", bins.len())));
                        lines.push(Line::from(Span::styled(
                            bins.join("  "),
                            Style::new().fg(GREEN),
                        )));
                    }
                    lines.push(Line::raw(""));
                    lines.push(section(&format!("Files ({})", files.len())));
                    for x in files.iter().take(MAX_FILE_LINES) {
                        let color = if x.starts_with("/etc") {
                            AMBER
                        } else if x.contains("/bin/") {
                            GREEN
                        } else if x.contains("/modules/") {
                            PINK
                        } else if x.contains("/share/doc") || x.contains("/share/man") {
                            DIM
                        } else {
                            FG
                        };
                        lines.push(Line::from(Span::styled(x.clone(), Style::new().fg(color))));
                    }
                    if files.len() > MAX_FILE_LINES {
                        lines.push(Line::from(Span::styled(
                            format!("… and {} more", files.len() - MAX_FILE_LINES),
                            Style::new().fg(DIM),
                        )));
                    }
                }
            }
            Tab::Hardware => {
                let d = &app.devices[idx];
                lines.push(Line::from(Span::styled(
                    d.name.clone(),
                    Style::new().fg(FG).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(Span::styled(d.class.clone(), Style::new().fg(DIM))));
                lines.push(Line::raw(""));
                lines.push(kv("Bus", format!("{} {}", d.bus.label(), d.slot), CYAN));
                lines.push(match &d.driver {
                    Some(x) => kv("Driver", x.clone(), GREEN),
                    None => kv("Driver", "none bound".into(), DIM),
                });
                if let Some(r) = app.res_cache.get(&idx) {
                    if let Some(m) = &r.module {
                        lines.push(kv("Module", m.clone(), PINK));
                    }
                    if let Some(p) = &r.path {
                        lines.push(kv("File", p.clone(), DIM));
                    }
                    lines.push(Line::raw(""));
                    lines.push(section("Owning package"));
                    match &r.package {
                        Some(pkg) => {
                            lines.push(Line::from(vec![
                                Span::styled("▶ ", Style::new().fg(PINK)),
                                Span::styled(
                                    pkg.clone(),
                                    Style::new().fg(GREEN).add_modifier(Modifier::BOLD),
                                ),
                            ]));
                            lines.push(Line::raw(""));
                            lines.push(Line::from(Span::styled(
                                "Enter → open it in the Packages tab",
                                Style::new().fg(DIM),
                            )));
                        }
                        None => lines.push(Line::from(Span::styled(
                            "unknown",
                            Style::new().fg(AMBER).add_modifier(Modifier::BOLD),
                        ))),
                    }
                    if !r.note.is_empty() {
                        lines.push(Line::raw(""));
                        lines.push(Line::from(Span::styled(
                            r.note.clone(),
                            Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
                        )));
                    }
                }
            }
        }
    } else {
        lines.push(Line::from(Span::styled("Nothing selected.", Style::new().fg(DIM))));
    }

    app.detail_len = lines.len().min(u16::MAX as usize) as u16;
    let block = border(VIOLET).title(Span::styled(
        " Details ",
        Style::new().fg(VIOLET).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        area,
    );
}

fn key(k: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {k} "),
            Style::new().fg(Color::Black).bg(VIOLET).add_modifier(Modifier::BOLD),
        ),
    ]
}

fn hint(k: &str, text: &str) -> Vec<Span<'static>> {
    let mut v = key(k);
    v.push(Span::styled(format!(" {text}  "), Style::new().fg(DIM)));
    v
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = vec![];
    match app.mode {
        Mode::Search => {
            spans.extend(hint("Enter", "keep filter"));
            spans.extend(hint("Esc", "clear"));
            spans.extend(hint("^U", "wipe"));
        }
        Mode::Find => {
            spans.extend(hint("Enter", "find"));
            spans.extend(hint("Esc", "cancel"));
        }
        Mode::Normal => {
            spans.extend(hint("↑↓", "move"));
            spans.extend(hint("/", "filter"));
            spans.extend(hint("f", "find owner"));
            spans.extend(hint("Tab", "switch"));
            spans.extend(hint("s", "sort"));
            spans.extend(hint("y", "copy"));
            spans.extend(hint("?", "help"));
            spans.extend(hint("q", "quit"));
        }
    }
    if !app.status.is_empty() {
        spans.push(Span::styled(
            format!("● {}", app.status),
            Style::new().fg(GREEN),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_find(f: &mut Frame, area: Rect, app: &App) {
    let r = centered(64, 5, area);
    f.render_widget(Clear, r);
    let block = border(PINK).title(Span::styled(
        " Find the package ",
        Style::new().fg(PINK).add_modifier(Modifier::BOLD),
    ));
    let lines = vec![
        Line::from(Span::styled(
            "Type a command (ls) or an absolute path (/etc/hosts)",
            Style::new().fg(DIM),
        )),
        Line::from(vec![
            Span::styled("❯ ", Style::new().fg(PINK)),
            Span::styled(app.find_input.clone(), Style::new().fg(FG).add_modifier(Modifier::BOLD)),
            Span::styled("▏", Style::new().fg(PINK)),
        ]),
    ];
    f.render_widget(Paragraph::new(lines).block(block), r);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let r = centered(58, 19, area);
    f.render_widget(Clear, r);
    let rows = [
        ("↑ ↓  j k", "move selection"),
        ("PgUp PgDn", "move by 10"),
        ("g  G", "jump to top / bottom"),
        ("Tab  1  2", "switch Packages / Hardware"),
        ("/", "filter (several words = AND)"),
        ("f", "find owner of a command or file"),
        ("Enter", "hardware: open its package"),
        ("s", "sort packages by name / size"),
        ("J  K", "scroll the details pane"),
        ("y", "copy package name (OSC 52)"),
        ("Esc", "clear filter / close popup"),
        ("q  Ctrl-C", "quit"),
    ];
    let mut lines = vec![Line::raw("")];
    for (k, d) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("  {k:<12}"), Style::new().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(d, Style::new().fg(FG)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  press any key to close",
        Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
    )));
    let block = border(VIOLET).title(Span::styled(
        " ◆ FindThePackage ",
        Style::new().fg(VIOLET).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(Paragraph::new(lines).block(block), r);
}
