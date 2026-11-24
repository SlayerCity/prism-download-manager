//! Well-known locations of Prism's per-user data.
//!
//! The layout must stay in sync with `prism-native-host`, which reads `ipc.json`
//! from the same directory to find the running app.

use std::path::{Path, PathBuf};

/// Environment variable that overrides the data directory (tests, portable mode).
pub const DATA_DIR_ENV: &str = "PRISM_DATA_DIR";

/// Per-user application data directory.
///
/// * Windows: `%APPDATA%\Prism`
/// * Linux:   `$XDG_DATA_HOME/prism` (default `~/.local/share/prism`)
/// * macOS:   `~/Library/Application Support/Prism`
///
/// A `portable.txt` file next to the executable switches to `<exe dir>/data`.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        return PathBuf::from(dir);
    }
    if let Some(portable) = portable_dir() {
        return portable;
    }
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "linux") {
        base.join("prism")
    } else {
        base.join("Prism")
    }
}

fn portable_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    portable_root(&exe).map(|dir| dir.join("data"))
}

fn portable_root(exe: &Path) -> Option<&Path> {
    let dir = exe.parent()?;
    dir.join("portable.txt").exists().then_some(dir)
}

/// The built-in portable destination follows the executable, not the host profile.
pub(crate) fn portable_download_dir() -> Option<PathBuf> {
    portable_download_dir_for(&std::env::current_exe().ok()?)
}

fn portable_download_dir_for(exe: &Path) -> Option<PathBuf> {
    portable_root(exe).map(|dir| dir.join("Downloads"))
}

pub fn database_path() -> PathBuf {
    data_dir().join("prism.db")
}

pub fn ipc_file() -> PathBuf {
    data_dir().join("ipc.json")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

/// Directory for managed tools (yt-dlp, ffmpeg).
pub fn tools_dir() -> PathBuf {
    data_dir().join("tools")
}

/// Default folder for finished downloads (`~/Downloads/Prism` style is avoided:
/// IDM users expect category sub-folders directly under Downloads).
pub fn default_download_dir() -> PathBuf {
    portable_download_dir()
        .or_else(dirs::download_dir)
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir())?;
    std::fs::create_dir_all(logs_dir())?;
    std::fs::create_dir_all(tools_dir())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_download_default_follows_the_executable() {
        let home = tempfile::tempdir().unwrap();
        let from = home.path().join("before");
        let to = home.path().join("after");
        std::fs::create_dir(&from).unwrap();
        assert_eq!(portable_download_dir_for(&from.join("prism.exe")), None);
        std::fs::write(from.join("portable.txt"), "portable").unwrap();
        assert_eq!(
            portable_download_dir_for(&from.join("prism.exe")),
            Some(from.join("Downloads"))
        );
        std::fs::rename(&from, &to).unwrap();
        assert_eq!(
            portable_download_dir_for(&to.join("prism.exe")),
            Some(to.join("Downloads"))
        );
    }
}
