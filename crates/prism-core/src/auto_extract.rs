//! Safe, opt-in extraction of completed ZIP downloads (#56).
//!
//! Extraction is deliberately ZIP-only for now. Entries are preflighted before any output is
//! created, constrained by entry/expanded-size limits, and written into a fresh sibling folder
//! with create_new so existing files can never be overwritten. Archive permissions are ignored.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const MAX_ENTRIES: usize = 10_000;
const MAX_FILE_SIZE: u64 = 1 << 30;
const MAX_TOTAL_SIZE: u64 = 2 << 30;
const COPY_CHUNK: usize = 64 * 1024;

#[derive(Debug)]
struct PlannedEntry {
    path: PathBuf,
    is_dir: bool,
    size: u64,
    index: usize,
}

/// Extract a ZIP into a newly-created folder beside the archive. Returns the folder path.
pub fn extract_zip_archive(archive_path: &Path) -> Result<PathBuf, String> {
    let file = File::open(archive_path).map_err(|e| format!("Could not open ZIP: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Invalid ZIP archive: {e}"))?;
    if archive.len() == 0 || archive.len() > MAX_ENTRIES {
        return Err(format!(
            "ZIP must contain between 1 and {MAX_ENTRIES} entries"
        ));
    }

    let mut plans = Vec::with_capacity(archive.len());
    let mut seen = HashSet::new();
    let mut files = HashSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Could not read ZIP directory: {e}"))?;
        let raw_name = entry
            .name()
            .map_err(|e| format!("Invalid ZIP filename: {e}"))?
            .into_owned();
        if raw_name
            .chars()
            .any(|ch| ch == '\\' || ch == ':' || ch == '\0')
        {
            return Err("ZIP contains a platform-ambiguous path".into());
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| "ZIP contains an unsafe path".to_string())?
            .to_path_buf();
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("ZIP contains an unsafe path".into());
        }
        let is_dir = entry.is_dir();
        if let Some(mode) = entry.unix_mode() {
            let kind = mode & 0o170000;
            if kind == 0o120000 || (kind != 0 && kind != 0o100000 && kind != 0o040000) {
                return Err("ZIP contains a link or special file".into());
            }
        }
        if !seen.insert(relative.clone()) {
            // Duplicate directory records are harmless, but duplicate files are ambiguous.
            if !is_dir {
                return Err("ZIP contains duplicate file paths".into());
            }
            continue;
        }
        let size = entry.size();
        if !is_dir {
            if size > MAX_FILE_SIZE {
                return Err(format!(
                    "ZIP entry exceeds the {MAX_FILE_SIZE}-byte per-file limit"
                ));
            }
            total = total
                .checked_add(size)
                .ok_or_else(|| "ZIP expanded size overflow".to_string())?;
            if total > MAX_TOTAL_SIZE {
                return Err(format!(
                    "ZIP exceeds the {MAX_TOTAL_SIZE}-byte expanded-size limit"
                ));
            }
            files.insert(relative.clone());
        }
        plans.push(PlannedEntry {
            path: relative,
            is_dir,
            size,
            index,
        });
    }
    // A regular file cannot also be an ancestor of another entry.
    for plan in &plans {
        let mut parent = plan.path.parent();
        while let Some(path) = parent {
            if files.contains(path) {
                return Err("ZIP file path conflicts with a directory path".into());
            }
            parent = path.parent();
        }
    }

    let parent = archive_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = archive_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("extracted");
    let destination = (1..=10_000)
        .find_map(|n| {
            let name = if n == 1 {
                stem.to_string()
            } else {
                format!("{stem} ({n})")
            };
            let candidate = parent.join(name);
            match fs::create_dir(&candidate) {
                Ok(()) => Some(Ok(candidate)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(e) => Some(Err(format!("Could not create extraction folder: {e}"))),
            }
        })
        .ok_or_else(|| "Could not choose a new extraction folder".to_string())??;

    let result = (|| {
        for plan in &plans {
            let output = destination.join(&plan.path);
            if plan.is_dir {
                fs::create_dir_all(&output)
                    .map_err(|e| format!("Could not create ZIP folder: {e}"))?;
                continue;
            }
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Could not create ZIP folder: {e}"))?;
            }
            let mut input = archive
                .by_index(plan.index)
                .map_err(|e| format!("Could not read ZIP entry: {e}"))?;
            let mut output_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)
                .map_err(|e| format!("Could not create extracted file: {e}"))?;
            let mut copied = 0_u64;
            let mut buffer = vec![0; COPY_CHUNK];
            loop {
                let read = input
                    .read(&mut buffer)
                    .map_err(|e| format!("ZIP entry failed integrity checking: {e}"))?;
                if read == 0 {
                    break;
                }
                copied = copied
                    .checked_add(read as u64)
                    .ok_or_else(|| "ZIP entry size overflow".to_string())?;
                if copied > plan.size || copied > MAX_FILE_SIZE {
                    return Err("ZIP entry expanded beyond its declared size".into());
                }
                output_file
                    .write_all(&buffer[..read])
                    .map_err(|e| format!("Could not write extracted file: {e}"))?;
            }
            if copied != plan.size {
                return Err("ZIP entry size did not match its directory record".into());
            }
        }
        Ok(destination.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&destination);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, contents) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn extracts_into_new_folder_without_overwriting() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("bundle.zip");
        make_zip(&archive, &[("nested/file.txt", b"hello")]);
        fs::create_dir(temp.path().join("bundle")).unwrap();
        let output = extract_zip_archive(&archive).unwrap();
        assert_eq!(output.file_name().unwrap(), "bundle (2)");
        assert_eq!(fs::read(output.join("nested/file.txt")).unwrap(), b"hello");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_links() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("link.zip");
        let file = File::create(&archive).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        // unix_permissions intentionally masks off file-type bits; use the ZIP writer's
        // symlink helper so the test creates a real symlink entry in external attributes.
        writer
            .add_symlink("link", "../../outside", SimpleFileOptions::default())
            .unwrap();
        writer.finish().unwrap();
        assert!(extract_zip_archive(&archive).is_err());
        assert!(!temp.path().join("link").exists());
    }

    #[test]
    fn rejects_traversal_and_writes_nothing_outside() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("bad.zip");
        make_zip(&archive, &[("../escape.txt", b"no")]);
        assert!(extract_zip_archive(&archive).is_err());
        assert!(!temp.path().parent().unwrap().join("escape.txt").exists());
        assert!(!temp.path().join("bad").exists());
    }
}
