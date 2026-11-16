# Builds the Windows installer (.exe, NSIS) and the portable app.
# Requirements: Rust (rustup) + Visual Studio C++ Build Tools. First run takes a few minutes.
#   powershell -ExecutionPolicy Bypass -File desktop\build.ps1
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

Write-Host '==> Testing the engine' -ForegroundColor Cyan
cargo test -p anytomd-engine --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host '==> Building the anytomd command-line tool' -ForegroundColor Cyan
cargo build --release -p anytomd-engine --bin anytomd
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$triple = (rustc -vV | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
New-Item -ItemType Directory -Force app\binaries | Out-Null
Copy-Item target\release\anytomd.exe "app\binaries\anytomd-$triple.exe" -Force

if (-not (Get-Command cargo-tauri -ErrorAction SilentlyContinue)) {
    Write-Host '==> Installing the Tauri CLI (one time)' -ForegroundColor Cyan
    cargo install tauri-cli --version '^2' --locked
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Push-Location app
try {
    if (-not (Test-Path icons\icon.ico)) {
        Write-Host '==> Generating app icons' -ForegroundColor Cyan
        cargo tauri icon icons\app-icon.svg
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    Write-Host '==> Building the desktop app and installer' -ForegroundColor Cyan
    cargo tauri build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally { Pop-Location }

New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\bundle\nsis\*.exe dist\ -Force
Copy-Item target\release\any-file-to-md.exe 'dist\Any File To MD (portable).exe' -Force
Copy-Item target\release\anytomd.exe dist\anytomd.exe -Force
Write-Host ''
Write-Host "Done. Installer and portable app are in $PSScriptRoot\dist" -ForegroundColor Green
Get-ChildItem dist | Format-Table Name, @{n='MB';e={[math]::Round($_.Length/1MB,1)}}
