use std::sync::mpsc;
use std::thread;
use std::time::Instant;
use crate::app::{App, InstallMsg, ViewState};
use crate::installer::install_software;

pub fn start_resolution(app: &mut App) {
    if app.is_resolving { return; }
    
    app.logs.push("[resolve] Spawning background resolution thread...".to_string());
    let (tx, rx) = mpsc::channel();
    app.resolution_rx = Some(rx);
    app.is_resolving = true;
    app.progress.done = 0;
    app.progress.total = app.tools.len();
    app.progress.current = "Resolving...".to_string();

    let catalog = app.catalog.clone();
    let distro = app.distro.clone();
    let client = app.client.clone();
    let tools_keys: Vec<String> = app.tools.iter().map(|t| t.key.clone()).collect();

    thread::spawn(move || {
        for key in tools_keys {
            let res = if let Some(spec) = catalog.software.get(&key) {
                crate::resolver::resolve_asset(&client, spec, &distro)
                    .map_err(|e| e.to_string())
            } else {
                Err("Missing spec".to_string())
            };
            let _ = tx.send((key, res));
        }
    });
}

pub fn install_selected(app: &mut App) {
    if app.state == ViewState::Installing { return; }
    
    let selected_items: Vec<(String, Option<crate::resolver::ResolvedAsset>)> = app.tools.iter()
        .filter(|it| it.selected)
        .map(|it| (it.key.clone(), it.resolved.clone()))
        .collect();

    if selected_items.is_empty() {
        app.logs.push("[warn] No tools selected for installation".to_string());
        return;
    }

    app.state = ViewState::Installing;
    app.install_start = Some(Instant::now());
    let (tx, rx) = mpsc::channel();
    app.installation_rx = Some(rx);
    
    app.progress.total = selected_items.len();
    app.progress.done = 0;
    app.progress.succeeded = 0;
    app.progress.failed = 0;
    app.progress.skipped = 0;

    let (cancel_tx, cancel_rx) = mpsc::channel();
    app.cancel_tx = Some(cancel_tx);

    let catalog = app.catalog.clone();
    let distro = app.distro.clone();
    let client = app.client.clone();
    let dry_run = app.dry_run;

    thread::spawn(move || {
        for (key, resolved_opt) in selected_items {
            let _ = tx.send(InstallMsg::Progress(key.clone(), "Preparing".to_string(), None));
            
            let spec = match catalog.software.get(&key) {
                Some(s) => s,
                None => {
                    let _ = tx.send(InstallMsg::Done(key, Err("Missing spec".to_string())));
                    continue;
                }
            };

            let resolved = match resolved_opt {
                Some(r) => r,
                None => {
                    let _ = tx.send(InstallMsg::Progress(key.clone(), "Resolving".to_string(), None));
                    match crate::resolver::resolve_asset(&client, spec, &distro) {
                        Ok(asset) => asset,
                        Err(e) => {
                            let _ = tx.send(InstallMsg::Done(key, Err(format!("Resolve failed: {}", e))));
                            continue;
                        }
                    }
                }
            };

            let _ = tx.send(InstallMsg::Progress(key.clone(), "Installing".to_string(), Some("BUSY".to_string())));
            let result = install_software(&client, &key, spec, &resolved, &distro, dry_run, &tx, &cancel_rx)
                .map(|outcome| outcome.logs);
            
            let is_cancelled = match &result {
                Err(e) if e.contains("cancelled") => true,
                _ => false,
            };

            let _ = tx.send(InstallMsg::Done(key, result));
            
            if is_cancelled {
                break;
            }
        }
        let _ = tx.send(InstallMsg::Finished);
    });
}

pub fn update_file_picker(app: &mut App, dir: std::path::PathBuf) {
    let mut entries = Vec::new();
    
    if dir.parent().is_some() {
        entries.push(std::path::PathBuf::from("")); // Special entry for ".."
    }

    if let Ok(iter) = std::fs::read_dir(&dir) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in iter.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().map_or(false, |e| e == "json") {
                files.push(path);
            }
        }
        dirs.sort();
        files.sort();
        entries.extend(dirs);
        entries.extend(files);
    }
    app.state = ViewState::FilePicker { current_dir: dir, entries, cursor: 0 };
}

pub fn start_restore_from_file(app: &mut App, json_file: std::path::PathBuf) {
    app.state = ViewState::Restoring;
    app.install_start = Some(Instant::now());
    let (tx, rx) = mpsc::channel();
    app.installation_rx = Some(rx);
    
    app.progress.operation = "Restore".to_string();
    app.progress.current = "User Files".to_string();
    app.progress.total = 1;
    app.progress.done = 0;
    app.progress.succeeded = 0;
    app.progress.failed = 0;
    app.progress.skipped = 0;

    let (cancel_tx, _cancel_rx) = mpsc::channel();
    app.cancel_tx = Some(cancel_tx);

    app.logs.push(format!("[restore] Starting restore using metadata: {}", json_file.display()));

    thread::spawn(move || {
        let backup_dir = match json_file.parent() {
            Some(p) => p,
            None => {
                let _ = tx.send(InstallMsg::Done("Restore".to_string(), Err("Invalid JSON path".to_string())));
                let _ = tx.send(InstallMsg::Finished);
                return;
            }
        };

        let _ = tx.send(InstallMsg::Progress("Restore".to_string(), "Restoring Files".to_string(), Some("BUSY".to_string())));
        
        let result = crate::restorer::restore_backup(backup_dir);
        let logs_vec = match result {
            Ok(l) => Ok(l),
            Err(e) => Err(e),
        };

        let _ = tx.send(InstallMsg::Done("Restore".to_string(), logs_vec));
        let _ = tx.send(InstallMsg::Finished);
    });
}
