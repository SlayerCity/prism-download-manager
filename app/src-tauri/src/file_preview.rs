//! Build bounded thumbnails from completed local downloads; never send a local path to the webview.
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, ImageReader, Limits};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Semaphore;

const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_EDGE: u32 = 4096;
const THUMB_EDGE: u32 = 960;

fn format_for_path(path: &Path) -> Option<ImageFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

// Force passive containers, not playlists or network-capable demuxers.
fn video_demuxer(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "mp4" | "m4v" | "mov" => Some("mov"),
        "mkv" | "webm" => Some("matroska"),
        _ => None,
    }
}

pub fn is_video(path: &Path) -> bool {
    video_demuxer(path).is_some()
}

pub async fn video_thumbnail(path: &Path, ffmpeg: &Path) -> Result<String, String> {
    static VIDEO_PREVIEW: Semaphore = Semaphore::const_new(1);
    const MAX_FRAME_BYTES: u64 = 4 * 1024 * 1024;

    let demuxer = video_demuxer(path).ok_or("This file type has no preview")?;
    if !path.is_absolute()
        || !ffmpeg.is_absolute()
        || !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
    {
        return Err("The downloaded file or FFmpeg is unavailable".into());
    }
    let _permit = VIDEO_PREVIEW
        .try_acquire()
        .map_err(|_| "Another video preview is in progress")?;
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-max_alloc",
            "67108864",
            "-protocol_whitelist",
            "file,pipe",
            "-f",
            demuxer,
            "-threads",
            "1",
            "-max_pixels",
            "36000000",
            "-i",
        ])
        .arg(path)
        .args([
            "-map",
            "0:V:0",
            "-frames:v",
            "1",
            "-filter_threads",
            "1",
            "-vf",
            "scale='min(960,iw)':'min(960,ih)':force_original_aspect_ratio=decrease",
            "-threads",
            "1",
            "-c:v",
            "png",
            "-f",
            "image2pipe",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = command.spawn().map_err(|_| "Could not start FFmpeg")?;
    let mut output = child
        .stdout
        .take()
        .ok_or("Could not read the video preview")?
        .take(MAX_FRAME_BYTES + 1);
    let result = tokio::time::timeout(Duration::from_secs(8), async {
        let mut bytes = Vec::new();
        output
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Could not read the video preview")?;
        if bytes.len() as u64 > MAX_FRAME_BYTES {
            return Err("The video preview exceeds the output limit");
        }
        let status = child
            .wait()
            .await
            .map_err(|_| "FFmpeg could not produce a preview")?;
        if !status.success() || image::guess_format(&bytes).ok() != Some(ImageFormat::Png) {
            return Err("FFmpeg could not find a preview frame");
        }
        Ok(bytes)
    })
    .await;
    match result {
        Ok(Ok(bytes)) => Ok(format!("data:image/png;base64,{}", STANDARD.encode(bytes))),
        Ok(Err(message)) => {
            let _ = child.kill().await;
            Err(message.into())
        }
        Err(_) => {
            let _ = child.kill().await;
            Err("The video preview timed out".into())
        }
    }
}

pub fn thumbnail(path: &Path) -> Result<String, String> {
    let format = format_for_path(path).ok_or("This file type has no preview")?;
    let metadata = fs::symlink_metadata(path).map_err(|_| "The downloaded file is unavailable")?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err("The file is not a regular image or exceeds the 16 MiB preview limit".into());
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| "The downloaded file is unavailable")?;
    if !file
        .metadata()
        .map_err(|_| "The downloaded file is unavailable")?
        .is_file()
    {
        return Err("The downloaded file is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the downloaded file")?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("The file exceeds the 16 MiB preview limit".into());
    }
    if image::guess_format(&bytes).ok() != Some(format) {
        return Err("The file contents do not match its image extension".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_EDGE);
    limits.max_image_height = Some(MAX_EDGE);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Could not decode the image within the preview limits")?;
    let image = if image.width() > THUMB_EDGE || image.height() > THUMB_EDGE {
        image.thumbnail(THUMB_EDGE, THUMB_EDGE)
    } else {
        image
    };
    let mut buffer = Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|_| "Could not create the image preview")?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(buffer.into_inner())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_scaled_and_encoded_as_png() {
        let dir = tempfile::tempdir().unwrap();
        for (name, format) in [
            ("screen.PNG", ImageFormat::Png),
            ("photo.jpg", ImageFormat::Jpeg),
            ("photo.webp", ImageFormat::WebP),
        ] {
            let path = dir.path().join(name);
            image::RgbImage::new(1440, 720)
                .save_with_format(&path, format)
                .unwrap();
            let result = thumbnail(&path).unwrap();
            let data = STANDARD
                .decode(result.strip_prefix("data:image/png;base64,").unwrap())
                .unwrap();
            let decoded = image::load_from_memory_with_format(&data, ImageFormat::Png).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (960, 480));
        }
    }

    #[test]
    fn rejects_other_contents_and_oversized_files_or_images() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake.png");
        fs::write(&fake, "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        assert!(thumbnail(&fake).is_err());
        assert!(thumbnail(&dir.path().join("file.svg")).is_err());
        fs::File::create(&fake)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert!(thumbnail(&fake).is_err());
        image::RgbImage::new(MAX_EDGE + 1, 8).save(&fake).unwrap();
        assert!(thumbnail(&fake).is_err());
    }

    #[tokio::test]
    async fn video_previews_use_one_local_frame() {
        use std::process::Command;
        let exe = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        let ffmpeg = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(exe))
            .find(|path| path.is_absolute() && path.is_file());
        let Some(ffmpeg) = ffmpeg else {
            eprintln!("FFmpeg not installed; skipping video fixture");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        for (name, codec, size) in [
            ("video #01.mp4", "mpeg4", "1280x720"),
            ("video.mkv", "ffv1", "320x180"),
        ] {
            let path = dir.path().join(name);
            let fixture = Command::new(&ffmpeg)
                .args([
                    "-v",
                    "error",
                    "-nostdin",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("color=c=blue:s={size}:d=0.1"),
                    "-frames:v",
                    "1",
                    "-threads",
                    "1",
                    "-c:v",
                    codec,
                ])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                fixture.status.success(),
                "{}",
                String::from_utf8_lossy(&fixture.stderr)
            );
            let result = video_thumbnail(&path, &ffmpeg).await.unwrap();
            let data = STANDARD
                .decode(result.strip_prefix("data:image/png;base64,").unwrap())
                .unwrap();
            let png = image::load_from_memory_with_format(&data, ImageFormat::Png).unwrap();
            let expected = if name.ends_with(".mp4") {
                (960, 540)
            } else {
                (320, 180)
            };
            assert_eq!((png.width(), png.height()), expected);
        }
        let invalid = dir.path().join("invalid.mp4");
        fs::write(&invalid, "Not a video").unwrap();
        assert!(video_thumbnail(&invalid, &ffmpeg).await.is_err());
        assert!(!is_video(&dir.path().join("playlist.m3u8")));
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_a_replacement_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let actual = dir.path().join("actual.png");
        image::RgbImage::new(20, 20).save(&actual).unwrap();
        let link = dir.path().join("link.png");
        std::os::unix::fs::symlink(actual, &link).unwrap();
        assert!(thumbnail(&link).is_err());
    }
}
