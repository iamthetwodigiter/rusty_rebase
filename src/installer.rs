use std::fs;
use std::sync::mpsc;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use reqwest::blocking::Client;

use crate::catalog::{SetupStep, SoftwareSpec, SourceSpec};
use crate::distro::{DistroInfo, PackageManager};
use crate::resolver::ResolvedAsset;

#[derive(Debug)]
pub struct InstallOutcome {
    pub logs: Vec<String>,
}

fn home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "home directory not found".to_string())
}

fn expand_tilde(input: &str) -> Result<PathBuf, String> {
    if input == "~" {
        return home_dir();
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return Ok(home_dir()?.join(rest));
    }
    Ok(PathBuf::from(input))
}


pub fn install_software(
    client: &Client,
    name: &str,
    spec: &SoftwareSpec,
    resolved: &ResolvedAsset,
    distro: &DistroInfo,
    dry_run: bool,
    tx: &mpsc::Sender<crate::app::InstallMsg>,
    cancel_rx: &mpsc::Receiver<()>,
) -> Result<InstallOutcome, String> {
    let mut logs = Vec::new();

    let pipe_log = |msg: String, tx: &mpsc::Sender<crate::app::InstallMsg>, logs: &mut Vec<String>| {
        let _ = tx.send(crate::app::InstallMsg::Log(msg.clone()));
        logs.push(msg);
    };

    pipe_log(format!("== {name} ({}) ==", spec.display_name), tx, &mut logs);
    pipe_log(format!("resolved version: {}", resolved.version), tx, &mut logs);

    let download_dir = home_dir().map_err(|e| e.to_string())?.join("Downloads/rusty_rebase");
    if !dry_run {
        fs::create_dir_all(&download_dir).map_err(|e| e.to_string())?;
    }

    for step in &spec.setup_steps {
        if cancel_rx.try_recv().is_ok() {
            return Err("Installation cancelled by user".to_string());
        }
        match step {
            SetupStep::Package { packages } => {
                if let Some(cmd) = distro.pkg_manager.install_command(packages) {
                    if dry_run {
                        pipe_log(format!("[dry-run] {cmd}"), tx, &mut logs);
                    } else {
                        pipe_log(format!("running: {cmd}"), tx, &mut logs);
                        let status = run_piped(&cmd, tx, cancel_rx)
                            .map_err(|e| e.to_string())?;
                        pipe_log(format!("package install exit status: {status}"), tx, &mut logs);
                    }
                } else {
                    logs.push("package manager unknown, skipped package setup step".to_string());
                }
            }
            SetupStep::PathHint { value } => {
                let install_root = match spec.install_dir.as_deref() {
                    Some(dir) => expand_tilde(dir).map_err(|e| e.to_string())?,
                    None => home_dir().map_err(|e| e.to_string())?,
                };
                let rendered = value.replace("<install_root>", &install_root.to_string_lossy());
                
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                let profile_name = if shell.contains("zsh") {
                    ".zshrc"
                } else if shell.contains("fish") {
                    ".config/fish/config.fish"
                } else {
                    ".bashrc"
                };
                
                let profile_path = home_dir().map_err(|e| e.to_string())?.join(profile_name);
                
                let export_line = if shell.contains("fish") {
                    format!("fish_add_path {}", rendered)
                } else {
                    format!("export PATH=\"$PATH:{}\"", rendered)
                };

                if dry_run {
                    pipe_log(format!("[dry-run] append to {}: {}", profile_path.display(), export_line), tx, &mut logs);
                } else {
                    let content = fs::read_to_string(&profile_path).unwrap_or_default();
                    if content.contains(&export_line) {
                        pipe_log(format!("path already configured in {}", profile_path.display()), tx, &mut logs);
                    } else {
                        match std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&profile_path)
                        {
                            Ok(mut file) => {
                                if let Err(e) = writeln!(file, "\n# Added by rusty_rebase\n{}", export_line) {
                                    logs.push(format!("failed to write to profile: {e}"));
                                } else {
                                    pipe_log(format!("added {} to {}", rendered, profile_path.display()), tx, &mut logs);
                                }
                            }
                            Err(e) => {
                                logs.push(format!("failed to open profile: {e}"));
                            }
                        }
                    }
                }
            }
            SetupStep::Note { value } => {
                logs.push(format!("note: {value}"));
            }
            SetupStep::Shell { command } => {
                let sys_arch = match std::env::consts::ARCH {
                    "x86_64" => "amd64",
                    "aarch64" => "arm64",
                    "x86" => "386",
                    other => other,
                };
                let processed_command = command.replace("{arch}", sys_arch).replace("{xarch}", std::env::consts::ARCH);

                if dry_run {
                    pipe_log(format!("[dry-run] shell: {}", processed_command), tx, &mut logs);
                } else {
                    pipe_log(format!("running shell: {}", processed_command), tx, &mut logs);
                    let status = run_piped(&processed_command, tx, cancel_rx)
                        .map_err(|e| e.to_string())?;
                    pipe_log(format!("shell command exit status: {status}"), tx, &mut logs);
                }
            }
        }
    }

    if !matches!(spec.source, SourceSpec::PackageManager) {
        let archive_path = download_dir.join(&resolved.file_name);
        if dry_run {
            pipe_log(format!("[dry-run] download {} -> {}", resolved.url, archive_path.display()), tx, &mut logs);
        } else {
            pipe_log(format!("downloading from {}", resolved.url), tx, &mut logs);
            
            download_to_file(client, &resolved.url, &archive_path, tx, cancel_rx)?;
            
            pipe_log(format!("downloaded to {}", archive_path.display()), tx, &mut logs);
        }

        let install_root = match spec.install_dir.as_deref() {
            Some(dir) => expand_tilde(dir).map_err(|e| e.to_string())?,
            None => home_dir().map_err(|e| e.to_string())?,
        };

        if !dry_run {
            fs::create_dir_all(&install_root).map_err(|e| e.to_string())?;
        }

        let is_vscode = match &spec.source {
            SourceSpec::OfficialSource { id: Some(v), .. } if v == "vscode" => true,
            _ => false,
        };
        if is_vscode {
            let res = handle_vscode_install(&archive_path, distro, dry_run, tx, cancel_rx)?;
            pipe_log(res, tx, &mut logs);
        } else {
            let extracted = extract_archive(&archive_path, &install_root, dry_run, tx, cancel_rx)?;
            pipe_log(extracted, tx, &mut logs);
        }
    } else {
        logs.push("source is package-only, skipping download/extract".to_string());
    }

    Ok(InstallOutcome { logs })
}

fn download_to_file(
    client: &Client,
    url: &str,
    dest: &Path,
    tx: &mpsc::Sender<crate::app::InstallMsg>,
    cancel_rx: &mpsc::Receiver<()>,
) -> Result<(), String> {
    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("failed to download from {url}: {e}"))?;
 
    let total_size = response.content_length();
    let mut file = fs::File::create(dest)
        .map_err(|e| format!("failed to create destination {}: {e}", dest.display()))?;
 
    let mut buffer = [0; 8192];
    let mut downloaded: u64 = 0;
    
    loop {
        if cancel_rx.try_recv().is_ok() {
            return Err("Download cancelled by user".to_string());
        }
        let n = response.read(&mut buffer).map_err(|e| format!("failed to read from response: {e}"))?;
        if n == 0 { break; }
        file.write_all(&buffer[..n]).map_err(|e| format!("failed to write to file: {e}"))?;
        downloaded += n as u64;

        if let Some(t) = total_size {
            let ratio = downloaded as f64 / t as f64;
            let _ = tx.send(crate::app::InstallMsg::SubProgress(ratio));
            let msg = format!("Downloading ({:.1}/{:.1} MB)", downloaded as f64 / 1024.0 / 1024.0, t as f64 / 1024.0 / 1024.0);
            let _ = tx.send(crate::app::InstallMsg::Progress("".to_string(), msg, None));
        } else {
            let msg = format!("Downloading ({:.1} MB)", downloaded as f64 / 1024.0 / 1024.0);
            let _ = tx.send(crate::app::InstallMsg::Progress("".to_string(), msg, None));
        }
    }
 
    Ok(())
}

fn extract_archive(
    path: &Path,
    install_root: &Path,
    dry_run: bool,
    tx: &mpsc::Sender<crate::app::InstallMsg>,
    cancel_rx: &mpsc::Receiver<()>,
) -> Result<String, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "invalid archive file name".to_string())?;
 
    if dry_run {
        return Ok(format!(
            "[dry-run] extract {} into {}",
            path.display(),
            install_root.display()
        ));
    }
 
    let command = if name.ends_with(".tar.gz") {
        format!("tar -xzf '{}' -C '{}'", path.display(), install_root.display())
    } else if name.ends_with(".tar.xz") {
        format!("tar -xJf '{}' -C '{}'", path.display(), install_root.display())
    } else if name.ends_with(".zip") {
        format!("unzip -o -q '{}' -d '{}'", path.display(), install_root.display())
    } else {
        return Ok(format!("downloaded artifact at {}, extraction skipped", path.display()));
    };
 
    let status = run_piped(&command, tx, cancel_rx).map_err(|e| e.to_string())?;
 
    Ok(format!(
        "extraction command exit status {} ({command})",
        status
    ))
}

fn handle_vscode_install(
    path: &Path,
    distro: &DistroInfo,
    dry_run: bool,
    tx: &mpsc::Sender<crate::app::InstallMsg>,
    cancel_rx: &mpsc::Receiver<()>,
) -> Result<String, String> {
    let cmd = match distro.pkg_manager {
        PackageManager::Apt => Some(format!("sudo apt install -y '{}'", path.display())),
        PackageManager::Dnf => Some(format!("sudo dnf install -y '{}'", path.display())),
        PackageManager::Pacman => Some(format!(
            "mkdir -p \"$HOME\"/.local/opt && tar -xzf '{}' -C \"$HOME\"/.local/opt",
            path.display()
        )),
        PackageManager::Unknown => None,
    };
 
    if let Some(cmd) = cmd {
        if dry_run {
            Ok(format!("[dry-run] {cmd}"))
        } else {
            let status = run_piped(&cmd, tx, cancel_rx)?;
            Ok(format!("vscode install exit status {} ({cmd})", status))
        }
    } else {
        Ok("unknown package manager: please install vscode artifact manually".to_string())
    }
}

fn run_piped(
    cmd: &str,
    tx: &mpsc::Sender<crate::app::InstallMsg>,
    cancel_rx: &mpsc::Receiver<()>,
) -> Result<std::process::ExitStatus, String> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn command: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (pipe_tx, pipe_rx) = std::sync::mpsc::channel();

    let tx_stdout = pipe_tx.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                let _ = tx_stdout.send(line);
            }
        }
    });

    let tx_stderr = pipe_tx;
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                let _ = tx_stderr.send(format!("[stderr] {}", line));
            }
        }
    });

    while let Ok(line) = pipe_rx.recv() {
        if cancel_rx.try_recv().is_ok() {
            let _ = child.kill();
            return Err("Operation cancelled by user".to_string());
        }
        let _ = tx.send(crate::app::InstallMsg::Log(line));
    }

    let status = child.wait().map_err(|e| format!("failed to wait for child: {e}"))?;
    Ok(status)
}