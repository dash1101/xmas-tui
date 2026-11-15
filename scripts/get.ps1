# Install (or update) the latest xmas-tui release on Windows: per-user, no
# admin rights needed. From PowerShell:
#
#   irm https://raw.githubusercontent.com/dash1101/xmas-tui/main/scripts/get.ps1 | iex
#
# Downloads the release's setup program, checks it against the release's
# SHA256SUMS, installs it quietly to %LOCALAPPDATA%\Programs\xmas-tui (with
# "run at startup" on), and starts the wallpaper.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$repo = 'dash1101/xmas-tui'
$asset = 'xmas-tui-setup-x86_64.exe'
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers @{ 'User-Agent' = 'xmas-tui-installer' }
$tag = $release.tag_name
if (-not $tag) { throw "couldn't find the latest release" }
Write-Host "installing xmas-tui $tag..."

$base = "https://github.com/$repo/releases/download/$tag"
$dir = Join-Path $env:TEMP "xmas-tui-$tag"
New-Item -ItemType Directory -Force $dir | Out-Null
$setup = Join-Path $dir $asset
Invoke-WebRequest "$base/$asset" -OutFile $setup -UseBasicParsing
$sums = Invoke-WebRequest "$base/SHA256SUMS" -UseBasicParsing
$text = if ($sums.Content -is [byte[]]) { [Text.Encoding]::UTF8.GetString($sums.Content) } else { [string]$sums.Content }
$line = $text -split "`n" | Where-Object { $_ -match [regex]::Escape($asset) } | Select-Object -First 1
if (-not $line) { throw "no checksum for $asset in SHA256SUMS - not installing" }
$want = ($line.Trim() -split '\s+')[0].ToLower()
$got = (Get-FileHash $setup -Algorithm SHA256).Hash.ToLower()
if ($want -ne $got) { throw "checksum mismatch for $asset - not installing" }

# a running copy is replaced, then started again
Get-Process xmas-tui -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500
$p = Start-Process $setup -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', '/MERGETASKS="startup"' -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "the installer exited with code $($p.ExitCode)" }
Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue

$exe = Join-Path $env:LOCALAPPDATA 'Programs\xmas-tui\xmas-tui.exe'
if (Test-Path $exe) {
    Start-Process $exe
    Write-Host "done: xmas-tui $tag is installed and running behind the desktop icons."
    Write-Host "settings: the gear at the top right, the tray icon, or 'xmas-tui settings' in the Start menu."
} else {
    Write-Host "installed xmas-tui $tag (start it from the Start menu)."
}
