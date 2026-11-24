# Installing Prism

Download the latest version from the **[Releases page](https://github.com/FortoGraphy/prism-download-manager/releases/latest)**.

| Your system | Download | Notes |
|---|---|---|
| Windows 10 / 11 (64-bit) | `Prism_<version>_x64-setup.exe` | Recommended. Installs for your user account, no admin rights needed. |
| Windows (IT / all users) | `Prism_<version>_x64_en-US.msi` | Installs for all users; needs administrator rights. |
| Windows (no install) | `Prism-<version>-windows-x64-portable.zip` | Unzip anywhere (USB stick works). Settings stay next to `prism.exe`. |
| macOS 11+ — Apple Silicon (M1–M4) | `Prism_<version>_aarch64.dmg` | |
| macOS 11+ — Intel | `Prism_<version>_x64.dmg` | |
| Ubuntu, Debian, Mint, Pop!_OS, elementary | `Prism_<version>_amd64.deb` | |
| Fedora, openSUSE, RHEL | `Prism-<version>-1.x86_64.rpm` | |
| Any other Linux | `Prism_<version>_amd64.AppImage` | |

Every release also contains `SHA256SUMS.txt` so you can verify downloads.

After installing the app, **install the browser extension** — that is what lets Prism catch
downloads and videos from your browser: [browser-extension.md](browser-extension.md).

---

## Security warnings

Download the artifact and `SHA256SUMS.txt` from the same official release. For example, use
`Get-FileHash -Algorithm SHA256 -LiteralPath .\Prism_<version>_x64-setup.exe` in PowerShell
(replace `<version>` with the downloaded filename), or use `shasum -a 256 <file>` on macOS
and `sha256sum <file>` on Linux. Compare the output with the entry for that exact file.
A match checks that the file is the published artifact; it does not show that it is safe.

If the checksum differs, don't open the file. If antivirus blocks or quarantines a file,
don't restore it or add an exclusion. For a report to the maintainer or antivirus vendor,
include the exact detection name, security product/version, Prism version, whether it was the
installer or an installed executable, and the affected file's SHA-256 if already available.
There's no need to retrieve a quarantined file or post one publicly to report the warning.

## Windows

1. Run `Prism_<version>_x64-setup.exe` if your system allows it. If Windows blocks installation,
   follow the [security guidance above](#security-warnings).
2. Prism starts and shows a short welcome tour. It sits in the system tray (bottom-right)
   while it works.

The installer also:
* registers Prism with Chrome, Edge, Brave, Opera, Vivaldi, Chromium and Firefox (so the
  extension can talk to the app),
* opens `magnet:` links and `.torrent` files with Prism,
* adds a Start-menu entry and an uninstaller (Settings → Apps → Prism → Uninstall).

**Portable version:** unzip, run `prism.exe`. Everything (settings, history, tools) stays in
the `data` folder next to it.

## macOS

1. Open the `.dmg` and drag **Prism** into **Applications**.
2. Notarization depends on the release. If macOS blocks the app, leave the protection
   in place and follow the [security guidance above](#security-warnings).
3. Optional but recommended for video downloads: `brew install ffmpeg` (Prism can also fetch a
   verified ffmpeg build by itself — Settings → Video & streams → Check tools).

## Linux

**Debian / Ubuntu family**

```bash
sudo apt install ./Prism_<version>_amd64.deb
```

**Fedora / openSUSE**

```bash
sudo dnf install ./Prism-<version>-1.x86_64.rpm      # openSUSE: sudo zypper install ./Prism-*.rpm
```

The package needs any `ffmpeg` (`/usr/bin/ffmpeg`): Fedora's `ffmpeg-free` or the full
`ffmpeg` from RPM Fusion both work, and Prism uses whichever is installed.

**AppImage (any distribution)**

```bash
chmod +x Prism_<version>_amd64.AppImage
./Prism_<version>_amd64.AppImage
```

The deb/rpm packages register Prism with Chrome, Chromium, Edge and Firefox for all users;
the app itself also registers Brave, Vivaldi and Opera for your user on every start.
`ffmpeg` is installed as a dependency of the deb/rpm packages (AppImage users: install
`ffmpeg` from your distribution for video features).

The tray icon needs an AppIndicator-capable desktop (KDE, Cinnamon, XFCE, MATE work out of the
box; on GNOME install the *AppIndicator and KStatusNotifierItem Support* extension).

## Updating

Install the newer version over the old one — your downloads, queues and settings are kept.
yt-dlp (used for video sites) updates itself automatically in the background.

## Uninstalling

* **Windows:** Settings → Apps → Prism → Uninstall (removes the browser registration too).
* **macOS:** drag Prism from Applications to the Trash. Settings live in
  `~/Library/Application Support/Prism` if you want to remove them as well.
* **Linux:** `sudo apt remove prism` / `sudo dnf remove prism`; settings live in
  `~/.local/share/prism`.
