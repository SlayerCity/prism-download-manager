//! Read only bounded, local WAV files for per-event sounds, without expanding WebView file access.
use std::io::Read;
use std::path::Path;

const MAX_BYTES: u64 = 5 * 1024 * 1024;

pub fn read_wav(path: &Path) -> Result<Vec<u8>, String> {
    let invalid = || "Choose a local WAV file no larger than 5 MiB".to_string();
    if !path.is_absolute()
        || path.to_string_lossy().starts_with("\\\\")
        || !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        return Err(invalid());
    }
    let info = std::fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !info.is_file() || info.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| invalid())?;
    if bytes.len() as u64 > MAX_BYTES
        || !bytes.starts_with(b"RIFF")
        || bytes.get(8..12) != Some(b"WAVE")
    {
        return Err(invalid());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_shipped_wav_and_rejects_non_wav_or_oversize() {
        let dir = tempfile::tempdir().unwrap();
        let sound = dir.path().join("sample.wav");
        let actual = include_bytes!("../../public/sounds/complete.wav");
        std::fs::write(&sound, actual).unwrap();
        assert_eq!(read_wav(&sound).unwrap(), actual);
        std::fs::write(&sound, b"not audio data").unwrap();
        assert!(read_wav(&sound).is_err());
        std::fs::File::create(&sound)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(read_wav(&sound).is_err());
        assert!(read_wav(Path::new("sample.wav")).is_err());
    }
}
