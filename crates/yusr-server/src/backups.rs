//! Backup files on disk: the automatic daily copy and the safety copy taken before a restore.

use std::path::{Path, PathBuf};
use std::time::Duration;

use yusr_infra::Store;

/// Automatic backups kept; older ones are deleted. Manual and pre-restore copies are never pruned.
const KEEP_AUTO: usize = 14;
const AUTO_EVERY: Duration = Duration::from_secs(24 * 3600);
const CHECK_EVERY: Duration = Duration::from_secs(3600);
const AUTO_PREFIX: &str = "yusr-auto-";

/// Writes a full backup into `dir` and returns its path. The file appears under its final
/// name only once complete, so a crash never leaves a half-written backup that looks valid.
pub async fn write_to(store: &Store, dir: &Path, label: &str) -> Result<PathBuf, String> {
    let backup = store.backup(&format!("system:{label}")).await.map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = format!("yusr-{label}-{}.yusrbak", backup.manifest.created_at.format("%Y%m%d-%H%M%S"));
    let path = dir.join(name);
    let tmp = path.with_extension("partial");
    std::fs::write(&tmp, &backup.bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn auto_backups(dir: &Path) -> Vec<(std::time::SystemTime, PathBuf)> {
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(AUTO_PREFIX) && e.path().extension().is_some_and(|x| x == "yusrbak"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    found.sort();
    found
}

/// Takes a backup whenever the newest automatic one is a day old, and prunes the oldest.
pub fn spawn_daily(store: Store, dir: PathBuf) {
    tokio::spawn(async move {
        loop {
            let due = auto_backups(&dir).last().is_none_or(|(t, _)| t.elapsed().map_or(true, |age| age >= AUTO_EVERY));
            if due {
                match write_to(&store, &dir, "auto").await {
                    Ok(p) => tracing::info!(path = %p.display(), "automatic backup written"),
                    Err(e) => tracing::error!(error = %e, "automatic backup failed"),
                }
                let all = auto_backups(&dir);
                for (_, old) in all.iter().take(all.len().saturating_sub(KEEP_AUTO)) {
                    let _ = std::fs::remove_file(old);
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}
