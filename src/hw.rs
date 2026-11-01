use crate::pm::{self, Manager};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bus {
    Pci,
    Usb,
}

impl Bus {
    pub fn label(self) -> &'static str {
        match self {
            Bus::Pci => "PCI",
            Bus::Usb => "USB",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub bus: Bus,
    pub slot: String,
    pub class: String,
    pub name: String,
    pub driver: Option<String>,
    pub modules: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Resolution {
    pub module: Option<String>,
    pub path: Option<String>,
    pub package: Option<String>,
    pub note: String,
}

fn cmd_out(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).env("LC_ALL", "C").output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn read(p: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn link_name(p: impl AsRef<Path>) -> Option<String> {
    fs::read_link(p)
        .ok()?
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
}

fn push_unique(v: &mut Vec<String>, s: String) {
    if !s.is_empty() && !v.contains(&s) {
        v.push(s);
    }
}

pub fn scan() -> Vec<Device> {
    let mut v = scan_pci();
    v.extend(scan_usb());
    v
}

fn pci_class_name(code: &str) -> &'static str {
    let hex = code.trim_start_matches("0x");
    match hex.get(0..2).unwrap_or("") {
        "00" => "Unclassified device",
        "01" => "Storage controller",
        "02" => "Network controller",
        "03" => "Display controller",
        "04" => "Multimedia controller",
        "05" => "Memory controller",
        "06" => "Bridge",
        "07" => "Communication controller",
        "08" => "System peripheral",
        "09" => "Input device",
        "0c" => "Serial bus controller",
        "0d" => "Wireless controller",
        "0e" => "Intelligent controller",
        "10" => "Encryption controller",
        "11" => "Signal processing controller",
        _ => "PCI device",
    }
}

fn pci_sysfs(slot: &str) -> PathBuf {
    let full = if slot.matches(':').count() >= 2 {
        slot.to_string()
    } else {
        format!("0000:{slot}")
    };
    PathBuf::from("/sys/bus/pci/devices").join(full)
}

fn sysfs_driver_info(dir: &Path) -> (Option<String>, Option<String>) {
    let driver = link_name(dir.join("driver"));
    let module = link_name(dir.join("driver/module"));
    (driver, module)
}

fn scan_pci() -> Vec<Device> {
    let mut devs = vec![];
    if let Some(out) = cmd_out("lspci", &["-vmm", "-k"]) {
        for block in out.split("\n\n") {
            let (mut slot, mut class, mut vendor, mut device) =
                (String::new(), String::new(), String::new(), String::new());
            let mut driver: Option<String> = None;
            let mut modules: Vec<String> = vec![];
            for line in block.lines() {
                let Some((k, v)) = line.split_once(':') else { continue };
                let v = v.trim().to_string();
                match k.trim() {
                    "Slot" => slot = v,
                    "Class" => class = v,
                    "Vendor" => vendor = v,
                    "Device" => device = v,
                    "Driver" => driver = Some(v),
                    "Module" => push_unique(&mut modules, v),
                    _ => {}
                }
            }
            if slot.is_empty() {
                continue;
            }
            let (sd, sm) = sysfs_driver_info(&pci_sysfs(&slot));
            if driver.is_none() {
                driver = sd;
            }
            let mut mods = vec![];
            if let Some(m) = sm {
                push_unique(&mut mods, m);
            }
            if let Some(d) = &driver {
                push_unique(&mut mods, d.clone());
            }
            for m in modules {
                push_unique(&mut mods, m);
            }
            devs.push(Device {
                bus: Bus::Pci,
                slot,
                class,
                name: format!("{vendor} {device}").trim().to_string(),
                driver,
                modules: mods,
            });
        }
        return devs;
    }

    let Ok(rd) = fs::read_dir("/sys/bus/pci/devices") else {
        return devs;
    };
    for e in rd.flatten() {
        let dir = e.path();
        let slot = e.file_name().to_string_lossy().into_owned();
        let vid = read(dir.join("vendor")).unwrap_or_default();
        let did = read(dir.join("device")).unwrap_or_default();
        let class = read(dir.join("class")).unwrap_or_default();
        let (driver, module) = sysfs_driver_info(&dir);
        let mut mods = vec![];
        if let Some(m) = module {
            push_unique(&mut mods, m);
        }
        if let Some(d) = &driver {
            push_unique(&mut mods, d.clone());
        }
        devs.push(Device {
            bus: Bus::Pci,
            slot,
            class: pci_class_name(&class).to_string(),
            name: format!("PCI device {}:{}", vid.trim_start_matches("0x"), did.trim_start_matches("0x")),
            driver,
            modules: mods,
        });
    }
    devs.sort_by(|a, b| a.slot.cmp(&b.slot));
    devs
}

fn scan_usb() -> Vec<Device> {
    let mut devs = vec![];
    let base = Path::new("/sys/bus/usb/devices");
    let Ok(rd) = fs::read_dir(base) else {
        return devs;
    };
    let mut names: Vec<String> = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names.iter().filter(|n| !n.contains(':')) {
        let dir = base.join(name);
        let vid = read(dir.join("idVendor")).unwrap_or_default();
        let pid = read(dir.join("idProduct")).unwrap_or_default();
        let manu = read(dir.join("manufacturer")).unwrap_or_default();
        let prod = read(dir.join("product")).unwrap_or_default();
        let title = format!("{manu} {prod}").trim().to_string();
        let title = if title.is_empty() {
            format!("USB device {vid}:{pid}")
        } else {
            format!("{title} [{vid}:{pid}]")
        };
        let class = if read(dir.join("bDeviceClass")).as_deref() == Some("09") {
            "USB hub"
        } else {
            "USB device"
        };
        let prefix = format!("{name}:");
        let mut drivers: Vec<String> = vec![];
        let mut mods: Vec<String> = vec![];
        for iface in names.iter().filter(|n| n.starts_with(&prefix)) {
            let (d, m) = sysfs_driver_info(&base.join(iface));
            if let Some(d) = d {
                push_unique(&mut drivers, d.clone());
                push_unique(&mut mods, m.unwrap_or(d));
            }
        }
        devs.push(Device {
            bus: Bus::Usb,
            slot: name.clone(),
            class: class.to_string(),
            name: title,
            driver: if drivers.is_empty() { None } else { Some(drivers.join(", ")) },
            modules: mods,
        });
    }
    devs
}

fn norm(s: &str) -> String {
    s.replace('-', "_")
}

fn find_ko(dir: &Path, want: &str, depth: u8) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    for e in fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if let Some(f) = find_ko(&p, want, depth - 1) {
                return Some(f);
            }
        } else if let Some(stem) = name.split(".ko").next() {
            if name.contains(".ko") && norm(stem) == want {
                return Some(p);
            }
        }
    }
    None
}

fn module_path(md: &str) -> Option<String> {
    if let Some(out) = cmd_out("modinfo", &["-n", md]) {
        let p = out.trim().to_string();
        if !p.is_empty() {
            return Some(p);
        }
    }
    let rel = read("/proc/sys/kernel/osrelease")?;
    let want = norm(md);
    for root in [format!("/usr/lib/modules/{rel}"), format!("/lib/modules/{rel}")] {
        let root = PathBuf::from(root);
        if !root.is_dir() {
            continue;
        }
        if let Some(builtin) = read(root.join("modules.builtin")) {
            let hit = builtin.lines().any(|l| {
                l.rsplit('/')
                    .next()
                    .and_then(|f| f.strip_suffix(".ko"))
                    .map(|s| norm(s) == want)
                    .unwrap_or(false)
            });
            if hit {
                return Some("(builtin)".into());
            }
        }
        if let Some(f) = find_ko(&root.join("kernel"), &want, 8) {
            return Some(f.to_string_lossy().into_owned());
        }
    }
    None
}

pub fn resolve(dev: &Device, m: Manager) -> Resolution {
    if dev.modules.is_empty() {
        return Resolution {
            note: "No kernel driver is bound to this device.".into(),
            ..Default::default()
        };
    }
    for md in &dev.modules {
        let Some(path) = module_path(md) else { continue };
        if path == "(builtin)" {
            let rel = read("/proc/sys/kernel/osrelease").unwrap_or_default();
            let candidates = [
                format!("/boot/vmlinuz-{rel}"),
                format!("/usr/lib/modules/{rel}/vmlinuz"),
                "/boot/vmlinuz".to_string(),
            ];
            let pkg = candidates.iter().find_map(|c| pm::owner_of(m, c));
            return Resolution {
                module: Some(md.clone()),
                path: Some("(built into the kernel image)".into()),
                package: pkg,
                note: "Driver is compiled straight into the kernel.".into(),
            };
        }
        let package = pm::owner_of(m, &path);
        return Resolution {
            module: Some(md.clone()),
            path: Some(path),
            package,
            note: String::new(),
        };
    }
    Resolution {
        module: dev.modules.first().cloned(),
        note: "Module file not found (modinfo missing or module not installed).".into(),
        ..Default::default()
    }
}
