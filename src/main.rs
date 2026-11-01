mod app;
mod hw;
mod pm;
mod ui;

use app::{App, Mode, Tab};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::io::Write;
use std::time::Duration;

const HELP: &str = "\
FindThePackage — every component of your system, traced to its package

USAGE:
    findthepackage                     launch the TUI
    findthepackage --plain [WHAT]      print a tab-separated report (WHAT: packages | hardware | all)
    findthepackage --owner <CMD|PATH>  print the package that owns a command or file
    findthepackage --help | --version

Supported package managers: dpkg, rpm, pacman, apk, xbps
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-h") | Some("--help") => print!("{HELP}"),
        Some("-V") | Some("--version") => println!("findthepackage {}", env!("CARGO_PKG_VERSION")),
        Some("--owner") => owner_cli(args.get(1)),
        Some("--plain") => plain_cli(args.get(1).map(String::as_str).unwrap_or("all")),
        Some(other) => {
            eprintln!("unknown option: {other}\n\n{HELP}");
            std::process::exit(2);
        }
        None => {
            if let Err(e) = run_tui() {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn need_manager() -> pm::Manager {
    pm::detect().unwrap_or_else(|| {
        eprintln!("No supported package manager found (dpkg, rpm, pacman, apk, xbps).");
        std::process::exit(1);
    })
}

fn owner_cli(target: Option<&String>) {
    let Some(t) = target else {
        eprintln!("usage: findthepackage --owner <command|path>");
        std::process::exit(2);
    };
    let m = need_manager();
    let path = if t.contains('/') { Some(t.clone()) } else { app::which(t) };
    let Some(path) = path else {
        eprintln!("'{t}' was not found in PATH");
        std::process::exit(1);
    };
    match pm::owner_of(m, &path) {
        Some(pkg) => println!("{path}\t{pkg}"),
        None => {
            eprintln!("{path} is not owned by any package");
            std::process::exit(1);
        }
    }
}

fn plain_cli(what: &str) {
    let m = need_manager();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    if what == "packages" || what == "all" {
        for p in pm::list_packages(m) {
            if writeln!(out, "package\t{}\t{}\t{}\t{}", p.name, p.version, p.size_bytes, p.summary).is_err() {
                return;
            }
        }
    }
    if what == "hardware" || what == "all" {
        for d in hw::scan() {
            let r = hw::resolve(&d, m);
            let line = writeln!(
                out,
                "hardware\t{}\t{}\t{}\t{}\t{}\t{}",
                d.bus.label(),
                d.slot,
                d.name,
                d.driver.as_deref().unwrap_or("-"),
                r.module.as_deref().unwrap_or("-"),
                r.package.as_deref().unwrap_or("-"),
            );
            if line.is_err() {
                return;
            }
        }
    }
    let _ = out.flush();
}

fn run_tui() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let result = tui_main(&mut terminal);
    ratatui::restore();
    result
}

fn tui_main(terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
    let manager = pm::detect();
    let worker = std::thread::spawn(move || {
        let pkgs = manager.map(pm::list_packages).unwrap_or_default();
        let devs = hw::scan();
        (pkgs, devs)
    });
    let mut frame = 0usize;
    while !worker.is_finished() {
        terminal.draw(|f| ui::draw_splash(f, "Scanning packages and hardware…", frame))?;
        frame += 1;
        std::thread::sleep(Duration::from_millis(80));
    }
    let (packages, devices) = worker.join().unwrap_or_default();

    let mut app = App::new(manager, packages, devices);
    if manager.is_none() {
        app.status = "No supported package manager detected".into();
    }

    while !app.quit {
        app.prepare_detail();
        terminal.draw(|f| ui::draw(f, &mut app))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    handle_key(&mut app, k.code, k.modifiers);
                }
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    if ctrl && code == KeyCode::Char('c') {
        app.quit = true;
        return;
    }
    if app.show_help {
        app.show_help = false;
        return;
    }
    app.status.clear();

    match app.mode {
        Mode::Search => match code {
            KeyCode::Esc => {
                app.query.clear();
                app.mode = Mode::Normal;
                app.refilter();
            }
            KeyCode::Enter => app.mode = Mode::Normal,
            KeyCode::Backspace => {
                app.query.pop();
                app.refilter();
            }
            KeyCode::Char('u') if ctrl => {
                app.query.clear();
                app.refilter();
            }
            KeyCode::Down => app.move_sel(1),
            KeyCode::Up => app.move_sel(-1),
            KeyCode::Char(c) if !ctrl => {
                app.query.push(c);
                app.refilter();
            }
            _ => {}
        },
        Mode::Find => match code {
            KeyCode::Esc => {
                app.find_input.clear();
                app.mode = Mode::Normal;
            }
            KeyCode::Enter => app.run_find(),
            KeyCode::Backspace => {
                app.find_input.pop();
            }
            KeyCode::Char(c) if !ctrl => app.find_input.push(c),
            _ => {}
        },
        Mode::Normal => match code {
            KeyCode::Char('q') => app.quit = true,
            KeyCode::Esc => {
                if !app.query.is_empty() {
                    app.query.clear();
                    app.refilter();
                } else {
                    app.quit = true;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => app.move_sel(1),
            KeyCode::Up | KeyCode::Char('k') => app.move_sel(-1),
            KeyCode::PageDown => app.move_sel(10),
            KeyCode::PageUp => app.move_sel(-10),
            KeyCode::Home | KeyCode::Char('g') => app.jump_top(),
            KeyCode::End | KeyCode::Char('G') => app.jump_bottom(),
            KeyCode::Tab | KeyCode::BackTab => app.next_tab(),
            KeyCode::Char('1') => app.set_tab(Tab::Packages),
            KeyCode::Char('2') => app.set_tab(Tab::Hardware),
            KeyCode::Char('/') => app.mode = Mode::Search,
            KeyCode::Char('f') => app.mode = Mode::Find,
            KeyCode::Char('s') => app.toggle_sort(),
            KeyCode::Char('J') => app.scroll_detail(3),
            KeyCode::Char('K') => app.scroll_detail(-3),
            KeyCode::Char('?') => app.show_help = true,
            KeyCode::Enter => app.open_owner(),
            KeyCode::Char('y') => match app.copy_text() {
                Some(t) => {
                    copy_osc52(&t);
                    app.status = format!("Copied “{t}”");
                }
                None => app.status = "Nothing to copy".into(),
            },
            _ => {}
        },
    }
}

fn copy_osc52(text: &str) {
    let seq = format!("\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let mut out = std::io::stdout();
    let _ = out.write_all(seq.as_bytes());
    let _ = out.flush();
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}
