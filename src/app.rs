use crate::hw::{self, Device, Resolution};
use crate::pm::{self, Manager, Package};
use ratatui::widgets::TableState;
use std::collections::HashMap;
use std::fs;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Packages,
    Hardware,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Find,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Name,
    Size,
}

pub struct SysInfo {
    pub distro: String,
    pub kernel: String,
    pub host: String,
}

impl SysInfo {
    pub fn load() -> Self {
        let rd = |p: &str| fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default();
        let distro = fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
            })
            .unwrap_or_else(|| "Linux".into());
        SysInfo {
            distro,
            kernel: rd("/proc/sys/kernel/osrelease"),
            host: rd("/proc/sys/kernel/hostname"),
        }
    }
}

pub struct App {
    pub manager: Option<Manager>,
    pub packages: Vec<Package>,
    pub devices: Vec<Device>,
    pub sys: SysInfo,
    pub tab: Tab,
    pub mode: Mode,
    pub sort: Sort,
    pub query: String,
    pub filtered: Vec<usize>,
    pub table: TableState,
    pub files_cache: HashMap<String, Vec<String>>,
    pub res_cache: HashMap<usize, Resolution>,
    pub detail_scroll: u16,
    pub detail_len: u16,
    pub find_input: String,
    pub show_help: bool,
    pub status: String,
    pub quit: bool,
}

impl App {
    pub fn new(manager: Option<Manager>, packages: Vec<Package>, devices: Vec<Device>) -> Self {
        let mut app = App {
            manager,
            packages,
            devices,
            sys: SysInfo::load(),
            tab: Tab::Packages,
            mode: Mode::Normal,
            sort: Sort::Name,
            query: String::new(),
            filtered: vec![],
            table: TableState::default(),
            files_cache: HashMap::new(),
            res_cache: HashMap::new(),
            detail_scroll: 0,
            detail_len: 0,
            find_input: String::new(),
            show_help: false,
            status: String::new(),
            quit: false,
        };
        app.refilter();
        app
    }

    pub fn total(&self) -> usize {
        match self.tab {
            Tab::Packages => self.packages.len(),
            Tab::Hardware => self.devices.len(),
        }
    }

    pub fn refilter(&mut self) {
        let terms: Vec<String> = self
            .query
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();
        let mut f: Vec<usize> = match self.tab {
            Tab::Packages => self
                .packages
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    let hay = format!("{} {}", p.name, p.summary).to_lowercase();
                    terms.iter().all(|t| hay.contains(t))
                })
                .map(|(i, _)| i)
                .collect(),
            Tab::Hardware => self
                .devices
                .iter()
                .enumerate()
                .filter(|(_, d)| {
                    let hay = format!(
                        "{} {} {} {} {}",
                        d.bus.label(),
                        d.name,
                        d.class,
                        d.driver.clone().unwrap_or_default(),
                        d.modules.join(" ")
                    )
                    .to_lowercase();
                    terms.iter().all(|t| hay.contains(t))
                })
                .map(|(i, _)| i)
                .collect(),
        };
        if self.tab == Tab::Packages && self.sort == Sort::Size {
            let pk = &self.packages;
            f.sort_by(|&a, &b| pk[b].size_bytes.cmp(&pk[a].size_bytes));
        }
        self.filtered = f;
        self.detail_scroll = 0;
        self.table.select(if self.filtered.is_empty() { None } else { Some(0) });
    }

    pub fn selected(&self) -> Option<usize> {
        self.table.selected().and_then(|i| self.filtered.get(i).copied())
    }

    pub fn move_sel(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            return;
        }
        let cur = self.table.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, self.filtered.len() as isize - 1);
        self.table.select(Some(next as usize));
        self.detail_scroll = 0;
    }

    pub fn jump_top(&mut self) {
        if !self.filtered.is_empty() {
            self.table.select(Some(0));
            self.detail_scroll = 0;
        }
    }

    pub fn jump_bottom(&mut self) {
        if !self.filtered.is_empty() {
            self.table.select(Some(self.filtered.len() - 1));
            self.detail_scroll = 0;
        }
    }

    pub fn set_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.refilter();
        }
    }

    pub fn next_tab(&mut self) {
        let t = if self.tab == Tab::Packages { Tab::Hardware } else { Tab::Packages };
        self.set_tab(t);
    }

    pub fn toggle_sort(&mut self) {
        if self.tab != Tab::Packages {
            self.status = "Sorting is only available on the Packages tab".into();
            return;
        }
        self.sort = if self.sort == Sort::Name { Sort::Size } else { Sort::Name };
        self.refilter();
        self.status = match self.sort {
            Sort::Name => "Sorted by name".into(),
            Sort::Size => "Sorted by installed size (largest first)".into(),
        };
    }

    pub fn prepare_detail(&mut self) {
        let (Some(idx), Some(m)) = (self.selected(), self.manager) else {
            return;
        };
        match self.tab {
            Tab::Packages => {
                let name = self.packages[idx].name.clone();
                if !self.files_cache.contains_key(&name) {
                    let files = pm::files_of(m, &name);
                    self.files_cache.insert(name, files);
                }
            }
            Tab::Hardware => {
                if !self.res_cache.contains_key(&idx) {
                    let r = hw::resolve(&self.devices[idx], m);
                    self.res_cache.insert(idx, r);
                }
            }
        }
    }

    pub fn copy_text(&self) -> Option<String> {
        let idx = self.selected()?;
        match self.tab {
            Tab::Packages => Some(self.packages[idx].name.clone()),
            Tab::Hardware => self.res_cache.get(&idx).and_then(|r| r.package.clone()),
        }
    }

    pub fn scroll_detail(&mut self, delta: i32) {
        let max = self.detail_len.saturating_sub(1) as i32;
        self.detail_scroll = (self.detail_scroll as i32 + delta).clamp(0, max) as u16;
    }

    pub fn jump_to_package(&mut self, name: &str) {
        self.tab = Tab::Packages;
        self.query.clear();
        self.refilter();
        let base = name.split(':').next().unwrap_or(name);
        let pos = self.filtered.iter().position(|&i| {
            let n = self.packages[i].name.as_str();
            n == name || n.split(':').next() == Some(base)
        });
        if let Some(p) = pos {
            self.table.select(Some(p));
        }
    }

    pub fn open_owner(&mut self) {
        if self.tab != Tab::Hardware {
            return;
        }
        let Some(idx) = self.selected() else { return };
        match self.res_cache.get(&idx).and_then(|r| r.package.clone()) {
            Some(pkg) => {
                self.status = format!("Opened {pkg}");
                self.jump_to_package(&pkg);
            }
            None => self.status = "No owning package known for this device".into(),
        }
    }

    pub fn run_find(&mut self) {
        let input = self.find_input.trim().to_string();
        self.find_input.clear();
        self.mode = Mode::Normal;
        if input.is_empty() {
            return;
        }
        let Some(m) = self.manager else {
            self.status = "No package manager detected".into();
            return;
        };
        let path = if input.contains('/') { Some(input.clone()) } else { which(&input) };
        let Some(path) = path else {
            self.status = format!("'{input}' was not found in PATH");
            return;
        };
        match pm::owner_of(m, &path) {
            Some(pkg) => {
                self.status = format!("{path} → {pkg}");
                self.jump_to_package(&pkg);
            }
            None => self.status = format!("{path} is not owned by any package"),
        }
    }
}

pub fn which(cmd: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(cmd))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}
