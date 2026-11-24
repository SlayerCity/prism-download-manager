# Builds a PortableApps.com Format directory/zip from finished Windows release artifacts.
# Usage: ./scripts/make-portable-paf.ps1 [-Target x86_64-pc-windows-msvc]
param([string]$Target = "x86_64-pc-windows-msvc")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$release = Join-Path $root "target\$Target\release"
if (-not (Test-Path (Join-Path $release "prism.exe"))) { $release = Join-Path $root "target\release" }
$nativeHostPath = Join-Path $root "app\src-tauri\binaries\prism-native-host-$Target.exe"
if (-not (Test-Path $nativeHostPath)) { throw "Missing Windows native host: $nativeHostPath" }
$version = (Get-Content (Join-Path $root "app\src-tauri\tauri.conf.json") | ConvertFrom-Json).version
$out = Join-Path $root "packaging\out"
$stage = Join-Path $out "PrismPortable"
$archive = Join-Path $out "Prism-$version-windows-x64-portableapps.zip"
$appDir = Join-Path $stage "App\Prism"
$packageVersion = "{0}.0" -f $version
$appInfo = Join-Path $stage "App\AppInfo"

Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $appDir, $appInfo, (Join-Path $stage "Data\Downloads"), (Join-Path $stage "Other\Source") | Out-Null
"Prism portable data folder" | Set-Content -Path (Join-Path $stage "Data\README.txt") -Encoding utf8
Copy-Item (Join-Path $release "prism.exe") $appDir
Copy-Item $nativeHostPath (Join-Path $appDir "prism-native-host.exe")
Copy-Item (Join-Path $root "app\src-tauri\icons\icon.ico") (Join-Path $appInfo "appicon.ico")

# The same marker is read by Prism and the browser-launched native host. Relative paths
# keep both user data and downloads beside App while the complete PAF folder is movable.
@'
DATA_DIR=../../Data
DOWNLOADS_DIR=../../Data/Downloads
'@ | Set-Content -Path (Join-Path $appDir "portable.txt") -Encoding utf8

$ext = Join-Path $root "extension\build"
$extensionDir = Join-Path $appDir "extension"
New-Item -ItemType Directory -Force $extensionDir | Out-Null
foreach ($browser in @("chromium", "firefox")) {
  $source = Join-Path $ext $browser
  if (-not (Test-Path $source)) { throw "Missing built extension directory: $source" }
  Copy-Item -Recurse $source (Join-Path $extensionDir $browser)
}
@'
Browser extension setup

Chromium-based browsers: install Prism Download Manager from the Chrome Web Store.
Firefox: open about:debugging, choose This Firefox, then Load Temporary Add-on and
select extension\firefox\manifest.json. Temporary Firefox add-ons must be loaded
again after Firefox restarts.
'@ | Set-Content -Path (Join-Path $extensionDir "README.txt") -Encoding utf8

$launcher = Join-Path $stage "PrismPortable.exe"
& rustc --edition=2024 --target $Target -C opt-level=3 -C strip=symbols (Join-Path $PSScriptRoot "portable-launcher.rs") -o $launcher
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $launcher)) { throw "Failed to build PrismPortable.exe" }

@"
[Format]
Type=PortableApps.comFormat
Version=3.9

[Details]
Name=Prism Portable
AppID=PrismPortable
Publisher=Prism contributors
Homepage=https://github.com/SlayerCity/prism-download-manager
Category=Utilities
Description=Fast multi-connection download manager
Language=Multilingual

[License]
Shareable=true
OpenSource=true
Freeware=true
CommercialUse=true

[Version]
PackageVersion=$packageVersion
DisplayVersion=$version

[Dependencies]
Requires64bitOS=yes
RequiresAdmin=no

[Control]
Icons=1
Start=PrismPortable.exe
ExtractIcon=App\Prism\prism.exe
"@ | Set-Content -Path (Join-Path $appInfo "appinfo.ini") -Encoding utf8

@'
<!doctype html>
<html lang="en"><meta charset="utf-8"><title>Prism Portable</title>
<body><h1>Prism Portable</h1>
<p>Run PrismPortable.exe from this folder. Settings and history are stored in Data;
new portable downloads go to Data\Downloads. Moving the complete PrismPortable folder
keeps both locations together. Prism itself is in App\Prism, and the launcher forwards
command-line arguments to it.</p>
<p>Browser integration registers the bundled native host for the current Windows user.
If you move the package after registering it, run PrismPortable.exe once from its new
location to refresh the browser registration. Browser setup instructions are in
App\Prism\extension\README.txt.</p>
<p>This is a PortableApps.com Format directory/zip, not an official PortableApps.com
release or installer. Microsoft WebView2 is required.</p></body></html>
'@ | Set-Content -Path (Join-Path $stage "help.html") -Encoding utf8
Copy-Item (Join-Path $PSScriptRoot "portable-launcher.rs") (Join-Path $stage "Other\Source\portable-launcher.rs")
Copy-Item (Join-Path $root "LICENSE") (Join-Path $stage "Other\Source\LICENSE")

Remove-Item -Force $archive -ErrorAction SilentlyContinue
$bsdtar = Join-Path $env:SystemRoot "System32\tar.exe"
& $bsdtar -a -c -f $archive -C $out "PrismPortable"
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $archive)) { throw "tar failed to create $archive" }
Write-Host "PortableApps-format package: $archive"
