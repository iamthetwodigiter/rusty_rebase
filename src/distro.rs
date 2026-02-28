use std::collections::HashMap;
use std::fs;
use std::process::Command;


#[derive(Debug, Clone)]
pub struct DistroInfo {
    pub id: String,
    pub pkg_manager: PackageManager,
}

#[derive(Debug, Clone)]
pub enum PackageManager {
    Apt,
    Dnf,
    Pacman,
    Unknown,
}

impl std::fmt::Display for PackageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            PackageManager::Apt => "apt",
            PackageManager::Dnf => "dnf",
            PackageManager::Pacman => "pacman",
            PackageManager::Unknown => "unknown",
        };
        write!(f, "{}", name)
    }
}

impl PackageManager {
    pub fn install_command(&self, packages: &[String]) -> Option<String> {
        if packages.is_empty() {
            return None;
        }
        let joined = packages.join(" ");
        match self {
            PackageManager::Apt => Some(format!("sudo apt update && sudo apt install -y {joined}")),
            PackageManager::Dnf => Some(format!("sudo dnf install -y {joined}")),
            PackageManager::Pacman => Some(format!("sudo pacman -Sy --noconfirm {joined}")),
            PackageManager::Unknown => None,
        }
    }

    pub fn get_package_version(&self, package: &str) -> Option<String> {
        match self {
            PackageManager::Apt => {
                let output = Command::new("apt-cache")
                    .args(["policy", package])
                    .output()
                    .ok()?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.contains("Candidate:") {
                        let v = line.split(':').nth(1)?.trim().to_string();
                        if v == "(none)" { return None; }
                        return Some(v);
                    }
                }
                None
            }
            PackageManager::Dnf => {
                let output = Command::new("dnf")
                    .args(["info", "-q", package])
                    .output()
                    .ok()?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.contains("Version") {
                        return Some(line.split(':').nth(1)?.trim().to_string());
                    }
                }
                None
            }
            PackageManager::Pacman => {
                let output = Command::new("pacman")
                    .args(["-Si", package])
                    .output()
                    .ok()?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.contains("Version") {
                        return Some(line.split(':').nth(1)?.trim().to_string());
                    }
                }
                None
            }
            PackageManager::Unknown => None,
        }
    }
}

pub fn detect_distro() -> Result<DistroInfo, String> {
    let content = fs::read_to_string("/etc/os-release").map_err(|e| format!("failed to read /etc/os-release: {e}"))?;
    let mut pairs = HashMap::new();

    for line in content.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let cleaned = value.trim_matches('"').to_string();
            pairs.insert(key.to_string(), cleaned);
        }
    }

    let id = pairs
        .get("ID")
        .cloned()
        .unwrap_or_default();

    let id_like = pairs
        .get("ID_LIKE")
        .cloned()
        .unwrap_or_default();

    let pkg_manager = detect_package_manager(&id, &id_like);

    Ok(DistroInfo { id, pkg_manager })
}

fn detect_package_manager(id: &str, id_like: &str) -> PackageManager {
    let debian_ids = ["ubuntu", "debian", "linuxmint", "pop", "ubuntu-budgie", "kdeneon"];
    let fedora_ids = ["fedora", "rhel", "centos", "rocky"];
    let arch_ids = ["arch", "manjaro", "endeavouros", "artix"];

    if debian_ids.iter().any(|&d| id == d) {
        return PackageManager::Apt;
    }
    if fedora_ids.iter().any(|&f| id == f) {
        return PackageManager::Dnf;
    }
    if arch_ids.iter().any(|&a| id == a) {
        return PackageManager::Pacman;
    }

    if id_like.contains("debian") || id_like.contains("ubuntu") {
        return PackageManager::Apt;
    }
    if id_like.contains("fedora") || id_like.contains("rhel") {
        return PackageManager::Dnf;
    }
    if id_like.contains("arch") {
        return PackageManager::Pacman;
    }

    detect_package_manager_runtime()
}

fn detect_package_manager_runtime() -> PackageManager {
    let managers = [("apt", PackageManager::Apt),
                    ("dnf", PackageManager::Dnf),
                    ("pacman", PackageManager::Pacman),
                    ("yum", PackageManager::Dnf),
                    ("zypper", PackageManager::Unknown)];

    for (cmd, manager) in &managers {
        if Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return manager.clone();
        }
    }

    PackageManager::Unknown
}