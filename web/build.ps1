# Builds the website into web\dist: the Rust engine compiled to WebAssembly plus the static page.
# Requirements: Rust (rustup), then once:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.100 --locked
# Preview locally:  python -m http.server -d web\dist 8000
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
$root = Split-Path $PSScriptRoot -Parent

Write-Host '==> Compiling the engine to WebAssembly' -ForegroundColor Cyan
cargo build --manifest-path "$root\desktop\Cargo.toml" -p anytomd-wasm --target wasm32-unknown-unknown --profile wasm
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$dist = Join-Path $PSScriptRoot 'dist'
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force "$dist\pkg", "$dist\fonts" | Out-Null
wasm-bindgen "$root\desktop\target\wasm32-unknown-unknown\wasm\anytomd_wasm.wasm" --target web --no-typescript --out-dir "$dist\pkg"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Copy-Item index.html, app.js, worker.js, favicon.svg $dist
Copy-Item "$root\desktop\app\ui\common.js", "$root\desktop\app\ui\common.css" $dist
Copy-Item "$root\desktop\app\ui\fonts\*.woff2" "$dist\fonts"
Write-Host "Done. Site is in $dist" -ForegroundColor Green
