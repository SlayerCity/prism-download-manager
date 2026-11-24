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
/// A `portable.txt` file next to the executable switches to `<exe dir>/data`; optional
/// `DATA_DIR` and `DOWNLOADS_DIR` entries can route those locations elsewhere.
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
    portable_data_dir_for(&std::env::current_exe().ok()?)
}

/// A marker can optionally redirect portable data to a sibling directory, which lets
/// nested package layouts keep user data outside the application binaries.
fn marker_target(exe: &Path, key: &str, fallback: &str) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let marker = dir.join("portable.txt");
    if !marker.exists() {
        return None;
    }
    let text = std::fs::read_to_string(&marker).unwrap_or_default();
    let target = text
        .lines()
        .find_map(|line| {
            line.trim()
                .trim_start_matches('\u{feff}')
                .strip_prefix(&format!("{key}="))
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback));
    Some(if target.is_absolute() {
        target
    } else {
        dir.join(target)
    })
}

fn portable_data_dir_for(exe: &Path) -> Option<PathBuf> {
    marker_target(exe, "DATA_DIR", "data")
}

/// The built-in portable destination follows the portable package when it moves.
pub(crate) fn portable_download_dir() -> Option<PathBuf> {
    portable_download_dir_for(&std::env::current_exe().ok()?)
}

fn portable_download_dir_for(exe: &Path) -> Option<PathBuf> {
    marker_target(exe, "DOWNLOADS_DIR", "Downloads")
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

    #[test]
    fn portable_paf_paths_keep_data_and_downloads_in_movable_package() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("App").join("Prism");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(
            app.join("portable.txt"),
            "DATA_DIR=../../Data\nDOWNLOADS_DIR=../../Data/Downloads\n",
        )
        .unwrap();
        let exe = app.join("prism.exe");
        assert_eq!(portable_data_dir_for(&exe), Some(app.join("../../Data")));
        assert_eq!(
            portable_download_dir_for(&exe),
            Some(app.join("../../Data/Downloads"))
        );
    }
}
