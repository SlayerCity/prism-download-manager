#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::Command;

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(
        hwnd: *mut core::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        kind: u32,
    ) -> i32;
}

fn show_error(message: &str) {
    #[cfg(windows)]
    {
        let text = message.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let title = "Prism Portable"
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // SAFETY: both strings are nul-terminated and remain alive for the call.
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
        }
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}

fn app_path(root: PathBuf) -> PathBuf {
    root.join("App").join("Prism").join("prism.exe")
}

fn run() -> Result<(), String> {
    let root = std::env::current_exe()
        .map_err(|error| format!("Could not locate the portable folder: {error}"))?
        .parent()
        .ok_or_else(|| "The portable launcher has no parent folder.".to_owned())?
        .to_path_buf();
    let app = app_path(root.clone());
    if !app.is_file() {
        return Err(format!("Prism is missing from {}.", app.display()));
    }
    let status = Command::new(&app)
        .current_dir(&root)
        .args(std::env::args_os().skip(1))
        .status()
        .map_err(|error| format!("Could not start Prism: {error}"))?;
    std::process::exit(status.code().unwrap_or(1));
}

fn main() {
    if let Err(error) = run() {
        show_error(&error);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_resolves_nested_app_binary() {
        let root = PathBuf::from(r"E:\Portable\PrismPortable");
        assert_eq!(
            app_path(root.join(".")),
            root.join("App").join("Prism").join("prism.exe")
        );
    }
}
