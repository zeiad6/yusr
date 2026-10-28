//! Where the desktop app keeps its data.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// PostgreSQL on Windows has failed to initialise data folders whose path is not plain
/// ASCII, which happens whenever the Windows user name is Arabic.
pub fn is_ascii_path(p: &Path) -> bool {
    p.to_str().is_some_and(|s| s.is_ascii())
}

/// Cleans Windows paths by removing Win32 verbatim extended-length prefixes (`\\?\` and `//?/`)
/// and normalizing all slashes to backslashes on Windows. External console tools like PostgreSQL's
/// `initdb.exe` and `pg_ctl.exe` spawn sub-tools via `cmd.exe` / `popen`, which fail with
/// "The system cannot find the path specified" when encountering verbatim prefixes.
pub fn clean_path(p: impl AsRef<Path>) -> PathBuf {
    let s = p.as_ref().to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\UNC\") {
        let normalized = format!(r"\\{stripped}").replace('/', "\\");
        return PathBuf::from(normalized);
    }
    if let Some(stripped) = s.strip_prefix("//?/UNC/") {
        let normalized = format!(r"\\{stripped}").replace('/', "\\");
        return PathBuf::from(normalized);
    }
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        #[cfg(windows)]
        {
            return PathBuf::from(stripped.replace('/', "\\"));
        }
        #[cfg(not(windows))]
        {
            return PathBuf::from(stripped);
        }
    }
    if let Some(stripped) = s.strip_prefix("//?/") {
        #[cfg(windows)]
        {
            return PathBuf::from(stripped.replace('/', "\\"));
        }
        #[cfg(not(windows))]
        {
            return PathBuf::from(stripped);
        }
    }

    #[cfg(windows)]
    {
        PathBuf::from(s.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        p.as_ref().to_path_buf()
    }
}

/// `%LOCALAPPDATA%\Yusr` when that path is ASCII, else `%PROGRAMDATA%\Yusr\u-<hash of the
/// user's folder>` (users may create folders under ProgramData; the hash keeps two
/// Windows accounts apart without putting their names in the path). Elsewhere, the XDG
/// data folder.
pub fn data_dir() -> PathBuf {
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty());
    if cfg!(windows) {
        let local = var("LOCALAPPDATA").unwrap_or_else(|| PathBuf::from(r"C:\Users\Public\AppData\Local"));
        let preferred = local.join("Yusr");
        if is_ascii_path(&preferred) {
            return preferred;
        }
        let shared = var("PROGRAMDATA").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        return shared.join("Yusr").join(user_key(&local));
    }
    var("XDG_DATA_HOME")
        .or_else(|| var("HOME").map(|h| h.join(".local").join("share")))
        .unwrap_or_else(std::env::temp_dir)
        .join("yusr")
}

fn user_key(home: &Path) -> String {
    let digest = Sha256::digest(home.as_os_str().to_string_lossy().as_bytes());
    format!("u-{}", &hex::encode(digest)[..12])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_ascii_paths_are_detected() {
        assert!(is_ascii_path(Path::new("/home/ali/.local/share/yusr")));
        assert!(!is_ascii_path(Path::new("/home/علي/.local/share/yusr")));
    }

    #[test]
    fn user_keys_are_stable_ascii_and_distinct() {
        let a = user_key(Path::new(r"C:\Users\علي\AppData\Local"));
        assert_eq!(a, user_key(Path::new(r"C:\Users\علي\AppData\Local")));
        assert_ne!(a, user_key(Path::new(r"C:\Users\منى\AppData\Local")));
        assert!(a.is_ascii() && a.starts_with("u-") && a.len() == 14);
    }

    #[test]
    fn cleans_extended_verbatim_path_prefixes() {
        let p1 = clean_path(r"\\?\C:\Program Files\Yusr\postgresql\bin");
        assert_eq!(p1, PathBuf::from(r"C:\Program Files\Yusr\postgresql\bin"));

        let p2 = clean_path("//?/C:/Program Files/Yusr/postgresql/bin");
        assert_eq!(p2, PathBuf::from(if cfg!(windows) { r"C:\Program Files\Yusr\postgresql\bin" } else { "C:/Program Files/Yusr/postgresql/bin" }));
    }
}
