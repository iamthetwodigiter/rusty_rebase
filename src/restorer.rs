use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::read::ZipArchive;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackupIndexEntry {
    pub relative_path: String,
    pub original_size: u64,
    pub sha256_hash: String,
    pub zip_file: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BackupInfo {
    pub source_path: String,
    pub backup_time: String,
    pub zip_files: Vec<String>,
    pub index: Option<Vec<BackupIndexEntry>>,
}

use crate::app::InstallMsg;
use std::sync::mpsc::Sender;

pub fn restore_backup(backup_dir: &Path, tx: Option<&Sender<InstallMsg>>) -> Result<Vec<String>, String> {
    let mut logs = Vec::new();
    let info_path = backup_dir.join(".rusty_sync_info.json");
    if !info_path.exists() {
        return Err(format!("Backup info file not found at: {}", info_path.display()));
    }

    let contents = fs::read_to_string(&info_path).map_err(|e| format!("Failed to read info file: {}", e))?;
    let info: BackupInfo = serde_json::from_str(&contents).map_err(|e| format!("Failed to parse info file: {}", e))?;

    let dest_dir = PathBuf::from(&info.source_path);
    if let Some(s) = tx {
        let _ = s.send(InstallMsg::Log(format!("Restoring backup from '{}' to '{}'", backup_dir.display(), dest_dir.display())));
    }
    logs.push(format!("Restoring backup from '{}' to '{}'", backup_dir.display(), dest_dir.display()));

    if !dest_dir.exists() {
        fs::create_dir_all(&dest_dir).map_err(|e| format!("Failed to create destination dir: {}", e))?;
    }

    if info.zip_files.is_empty() {
        logs.push("No zip files found in metadata.".to_string());
        return Ok(logs);
    }

    // Pre-calculate total files for progress bar
    let mut total_files = 0;
    for zip_name in &info.zip_files {
        let zip_path = backup_dir.join(zip_name);
        if let Ok(file) = File::open(&zip_path) {
            if let Ok(archive) = ZipArchive::new(file) {
                total_files += archive.len();
            }
        }
    }
    let total_archives = info.zip_files.len();
    if let Some(s) = tx {
        let _ = s.send(InstallMsg::Log(format!("[info] Found {} files across {} archives.", total_files, total_archives)));
    }

    let mut restored_count = 0;
    for (archive_idx, zip_name) in info.zip_files.iter().enumerate() {
        let zip_path = backup_dir.join(zip_name);
        if !zip_path.exists() {
            let msg = format!("[error] Zip archive missing: {}", zip_name);
            if let Some(s) = tx { let _ = s.send(InstallMsg::Log(msg.clone())); }
            logs.push(msg);
            continue;
        }

        if let Some(s) = tx {
            let _ = s.send(InstallMsg::Progress("Restoring Files".to_string(), format!("Extracting {} ({}/{})", zip_name, archive_idx + 1, total_archives), Some("BUSY".to_string())));
            let _ = s.send(InstallMsg::SubProgress((archive_idx as f64) / (total_archives as f64)));
        }

        let file = File::open(&zip_path).map_err(|e| format!("Failed to open zip: {}", e))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("Failed to read zip: {}", e))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| format!("Failed to read file from zip: {}", e))?;
            let rel_path = file.name().to_string();
            let outpath = match file.enclosed_name() {
                Some(path) => dest_dir.join(path),
                None => continue,
            };

            if rel_path.ends_with('/') {
                fs::create_dir_all(&outpath).map_err(|e| format!("Failed to create dir: {}", e))?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p).map_err(|e| format!("Failed to create parent dir: {}", e))?;
                    }
                }
                
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).map_err(|e| format!("Failed to read zip file contents: {}", e))?;
                
                let mut outfile = File::create(&outpath).map_err(|e| format!("Failed to create outfile: {}", e))?;
                outfile.write_all(&buffer).map_err(|e| format!("Failed to write outfile: {}", e))?;
                
                // Integrity check
                if let Some(ref index) = info.index {
                    if let Some(entry) = index.iter().find(|e| e.relative_path == rel_path) {
                        let mut hasher = Sha256::new();
                        hasher.update(&buffer);
                        let current_hash = format!("{:x}", hasher.finalize());
                        if current_hash != entry.sha256_hash {
                            let msg = format!("[WARNING] Integrity check FAILED for {}", rel_path);
                            if let Some(s) = tx { let _ = s.send(InstallMsg::Log(msg.clone())); }
                            logs.push(msg);
                        }
                    }
                }
                restored_count += 1;
                if let Some(s) = tx {
                    let _ = s.send(InstallMsg::Progress("Restoring Files".to_string(), format!("{} ({})", zip_name, rel_path), None));
                    let _ = s.send(InstallMsg::SubProgress((restored_count as f64) / (total_files as f64)));
                }
            }
        }
        let msg = format!("[done] Restored archive: {}", zip_name);
        if let Some(s) = tx { let _ = s.send(InstallMsg::Log(msg.clone())); }
        logs.push(msg);
    }

    if let Some(s) = tx {
        let _ = s.send(InstallMsg::SubProgress(1.0));
        let _ = s.send(InstallMsg::Log("✓ Restore completed successfully!".to_string()));
    }
    logs.push("Restore completed successfully!".to_string());
    Ok(logs)
}
