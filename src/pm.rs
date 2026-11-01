use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manager {
    Dpkg,
    Rpm,
    Pacman,
    Apk,
    Xbps,
}

impl Manager {
    pub fn label(self) -> &'static str {
        match self {
            Manager::Dpkg => "dpkg (Debian / Ubuntu)",
            Manager::Rpm => "rpm (Fedora / RHEL / SUSE)",
            Manager::Pacman => "pacman (Arch)",
            Manager::Apk => "apk (Alpine)",
            Manager::Xbps => "xbps (Void)",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Manager::Dpkg => "dpkg",
            Manager::Rpm => "rpm",
            Manager::Pacman => "pacman",
            Manager::Apk => "apk",
            Manager::Xbps => "xbps",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub size_bytes: u64,
    pub summary: String,
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn have(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
        .unwrap_or(false)
}

pub fn detect() -> Option<Manager> {
    if have("dpkg-query") && Path::new("/var/lib/dpkg/status").exists() {
        Some(Manager::Dpkg)
    } else if have("pacman") && Path::new("/var/lib/pacman/local").exists() {
        Some(Manager::Pacman)
    } else if have("apk") && Path::new("/lib/apk/db/installed").exists() {
        Some(Manager::Apk)
    } else if have("xbps-query") {
        Some(Manager::Xbps)
    } else if have("rpm") {
        Some(Manager::Rpm)
    } else {
        None
    }
}

pub fn list_packages(m: Manager) -> Vec<Package> {
    let mut pkgs = match m {
        Manager::Dpkg => list_dpkg(),
        Manager::Rpm => list_rpm(),
        Manager::Pacman => list_pacman(),
        Manager::Apk => list_apk(),
        Manager::Xbps => list_xbps(),
    };
    pkgs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    pkgs
}

fn list_dpkg() -> Vec<Package> {
    let fmt = "${db:Status-Abbrev}\\t${binary:Package}\\t${Version}\\t${Installed-Size}\\t${Architecture}\\t${binary:Summary}\\n";
    let Some(out) = run("dpkg-query", &["-W", "-f", fmt]) else {
        return vec![];
    };
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(6, '\t').collect();
            if f.len() < 6 || !(f[0].starts_with("ii") || f[0].starts_with("hi")) {
                return None;
            }
            Some(Package {
                name: f[1].to_string(),
                version: f[2].to_string(),
                size_bytes: f[3].trim().parse::<u64>().unwrap_or(0) * 1024,
                arch: f[4].to_string(),
                summary: f[5].to_string(),
            })
        })
        .collect()
}

fn list_rpm() -> Vec<Package> {
    let fmt = "%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{SIZE}\\t%{ARCH}\\t%{SUMMARY}\\n";
    let Some(out) = run("rpm", &["-qa", "--qf", fmt]) else {
        return vec![];
    };
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(5, '\t').collect();
            if f.len() < 5 {
                return None;
            }
            Some(Package {
                name: f[0].to_string(),
                version: f[1].to_string(),
                size_bytes: f[2].trim().parse().unwrap_or(0),
                arch: f[3].to_string(),
                summary: f[4].to_string(),
            })
        })
        .collect()
}

fn parse_size(s: &str) -> u64 {
    let mut it = s.split_whitespace();
    let n: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    let mult = match it.next().unwrap_or("B") {
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (n * mult) as u64
}

fn list_pacman() -> Vec<Package> {
    let Some(out) = run("pacman", &["-Qi"]) else {
        return vec![];
    };
    let mut pkgs = vec![];
    for block in out.split("\n\n") {
        let (mut name, mut version, mut arch, mut size, mut desc) =
            (String::new(), String::new(), String::new(), 0u64, String::new());
        for line in block.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let v = v.trim();
                match k.trim() {
                    "Name" => name = v.to_string(),
                    "Version" => version = v.to_string(),
                    "Architecture" => arch = v.to_string(),
                    "Installed Size" => size = parse_size(v),
                    "Description" => desc = v.to_string(),
                    _ => {}
                }
            }
        }
        if !name.is_empty() {
            pkgs.push(Package { name, version, arch, size_bytes: size, summary: desc });
        }
    }
    pkgs
}

fn list_apk() -> Vec<Package> {
    let Some(out) = run("apk", &["list", "-I"]) else {
        return vec![];
    };
    out.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let nv = it.next()?;
            let arch = it.next().unwrap_or("").to_string();
            let mut parts = nv.rsplitn(3, '-');
            let rel = parts.next()?;
            let ver = parts.next()?;
            let name = parts.next()?;
            Some(Package {
                name: name.to_string(),
                version: format!("{ver}-{rel}"),
                arch,
                size_bytes: 0,
                summary: String::new(),
            })
        })
        .collect()
}

fn list_xbps() -> Vec<Package> {
    let Some(out) = run("xbps-query", &["-l"]) else {
        return vec![];
    };
    out.lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, char::is_whitespace);
            let _state = it.next()?;
            let nv = it.next()?;
            let desc = it.next().unwrap_or("").trim().to_string();
            let (name, ver) = nv.rsplit_once('-')?;
            Some(Package {
                name: name.to_string(),
                version: ver.to_string(),
                arch: String::new(),
                size_bytes: 0,
                summary: desc,
            })
        })
        .collect()
}

pub fn owner_of(m: Manager, path: &str) -> Option<String> {
    let canon = std::fs::canonicalize(path)
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let mut candidates = vec![path.to_string()];
    if let Some(c) = canon {
        if c != path {
            candidates.push(c);
        }
    }
    candidates.iter().find_map(|p| owner_raw(m, p))
}

fn owner_raw(m: Manager, p: &str) -> Option<String> {
    match m {
        Manager::Dpkg => {
            let out = run("dpkg", &["-S", p])?;
            let line = out.lines().next()?;
            let (pk, _) = line.split_once(": ")?;
            Some(pk.split(", ").next()?.to_string())
        }
        Manager::Rpm => {
            let out = run("rpm", &["-qf", "--qf", "%{NAME}\\n", p])?;
            out.lines().next().map(|s| s.to_string())
        }
        Manager::Pacman => {
            let out = run("pacman", &["-Qqo", p])?;
            out.lines().next().map(|s| s.to_string())
        }
        Manager::Apk => {
            let out = run("apk", &["info", "-W", p])?;
            let tail = out.split("owned by ").nth(1)?.trim().to_string();
            let mut parts = tail.rsplitn(3, '-');
            parts.next()?;
            parts.next()?;
            Some(parts.next()?.to_string())
        }
        Manager::Xbps => {
            let out = run("xbps-query", &["-o", p])?;
            let line = out.lines().next()?;
            let (nv, _) = line.split_once(": ")?;
            Some(nv.rsplit_once('-')?.0.to_string())
        }
    }
}

pub fn files_of(m: Manager, pkg: &str) -> Vec<String> {
    let out = match m {
        Manager::Dpkg => run("dpkg", &["-L", pkg]),
        Manager::Rpm => run("rpm", &["-ql", pkg]),
        Manager::Pacman => run("pacman", &["-Qlq", pkg]),
        Manager::Apk => run("apk", &["info", "-L", pkg]),
        Manager::Xbps => run("xbps-query", &["-f", pkg]),
    };
    out.map(|o| {
        o.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| l.starts_with('/') && l.len() > 1 && !l.ends_with('/') && !l.ends_with("/."))
            .filter(|l| {
                !std::fs::symlink_metadata(l).map(|m| m.is_dir()).unwrap_or(false)
            })
            .collect()
    })
    .unwrap_or_default()
}
