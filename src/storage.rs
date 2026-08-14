use std::fs;

pub fn cleanup_old_files(save_dir: &str, max_size_mb: u64) {
    if max_size_mb == 0 { return; }
    let max_bytes = max_size_mb * 1024 * 1024;

    let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();
    for sub in &["photos", "videos", "motion", "auto", "alerts"] {
        let dir = format!("{}/{}", save_dir, sub);
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                        files.push((entry.path(), meta.len(), modified));
                    }
                }
            }
        }
    }

    let total: u64 = files.iter().map(|(_, s, _)| s).sum();
    if total <= max_bytes { return; }

    files.sort_by_key(|(_, _, t)| *t);
    let mut freed = 0u64;
    let need = total - max_bytes;
    for (path, size, _) in &files {
        if freed >= need { break; }
        if fs::remove_file(path).is_ok() {
            freed += size;
            tracing::info!("自动清理: {:?}", path);
        }
    }
    if freed > 0 {
        tracing::info!("存储清理完成，释放 {:.1} MB", freed as f64 / 1024.0 / 1024.0);
    }
}
