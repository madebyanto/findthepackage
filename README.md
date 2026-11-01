# FindThePackage

A neon terminal UI for Linux that shows **everything installed on your system and the package it belongs to**.

- **Packages tab** – every installed package with version, size, description, the commands it ships and its full file list.
- **Hardware tab** – every PCI and USB device with its kernel driver, the module file, and the **package that provides that driver**.
- **Find owner** (`f`) – type a command (`ls`) or a path (`/etc/hosts`) and jump straight to the package that owns it.

Works with **dpkg** (Debian/Ubuntu/Mint), **rpm** (Fedora/RHEL/openSUSE), **pacman** (Arch), **apk** (Alpine) and **xbps** (Void).

## Build & run

```bash
cargo build --release
./target/release/findthepackage
# optional: install it
sudo install -m755 target/release/findthepackage /usr/local/bin/
```

Needs Rust 1.85+ and a truecolor terminal for the full gradient look. `lspci` (pciutils) is optional: without it, PCI devices are shown by vendor:device ID.

## Keys

| Key | Action |
| --- | --- |
| `↑ ↓` / `j k`, `PgUp PgDn`, `g G` | move |
| `Tab`, `1`, `2` | switch Packages / Hardware |
| `/` | filter (several words = AND), `Esc` clears |
| `f` | find the owner of a command or file |
| `Enter` | (Hardware) open the owning package |
| `s` | sort packages by name / size |
| `J` `K` | scroll the details pane |
| `y` | copy the package name to the clipboard (OSC 52, works over SSH) |
| `?` | help |
| `q` / `Ctrl-C` | quit |

## Scripting

```bash
findthepackage --owner ls                 # /usr/bin/ls    coreutils
findthepackage --plain packages           # tab-separated: name, version, bytes, summary
findthepackage --plain hardware           # bus, slot, device, driver, module, package
```
