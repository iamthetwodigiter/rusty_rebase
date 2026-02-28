use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};


#[derive(Serialize, Deserialize, Debug)]
pub struct BackupInfo {
    pub source_path: String,
    pub backup_time: String,
}

pub fn restore_backup(backup_dir: &Path) -> Result<Vec<String>, String> {
    let mut logs = Vec::new();
    let info_path = backup_dir.join(".rusty_sync_info.json");
    if !info_path.exists() {
        return Err(format!("Backup info file not found at: {}", info_path.display()));
    }

    let contents = fs::read_to_string(&info_path).map_err(|e| format!("Failed to read info file: {}", e))?;
    let info: BackupInfo = serde_json::from_str(&contents).map_err(|e| format!("Failed to parse info file: {}", e))?;

    let dest_dir = PathBuf::from(&info.source_path);

    logs.push(format!("Restoring backup from '{}' to '{}'", backup_dir.display(), dest_dir.display()));

    copy_dir_recursive(backup_dir, &dest_dir, &info_path)?;

    logs.push("Restore completed successfully!".to_string());
    Ok(logs)
}

fn copy_dir_recursive(src: &Path, dst: &Path, skip_file: &Path) -> Result<(), String> {
    if !dst.exists() {
        fs::create_dir_all(dst).map_err(|e| format!("Failed to create directory {}: {}", dst.display(), e))?;
    }

    let entries = fs::read_dir(src).map_err(|e| format!("Failed to read directory {}: {}", src.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if path == skip_file {
            continue;
        }

        let file_name = entry.file_name();
        let dest_path = dst.join(file_name);

        let file_type = entry.file_type().map_err(|e| format!("Failed to get file type: {}", e))?;

        if file_type.is_dir() {
            copy_dir_recursive(&path, &dest_path, skip_file)?;
        } else {
            fs::copy(&path, &dest_path).map_err(|e| format!("Failed to copy file from {} to {}: {}", path.display(), dest_path.display(), e))?;
        }
    }

    Ok(())
}
