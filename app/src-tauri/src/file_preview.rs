//! Build bounded thumbnails from completed local downloads; never send a local path to the webview.
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageFormat, ImageReader, Limits};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read};
use std::path::Path;

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
