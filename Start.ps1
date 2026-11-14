$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot

# Find uv (a tiny tool that brings its own Python). If missing, download it into
# .tools\ inside this folder so nothing is installed system-wide.
$uv = Join-Path $PSScriptRoot '.tools\uv.exe'
if (-not (Test-Path -LiteralPath $uv)) {
    $found = Get-Command uv -ErrorAction SilentlyContinue
    if ($found) {
        $uv = $found.Source
    } else {
        Write-Host ''
        Write-Host '  First-time setup: downloading a small helper (uv)...' -ForegroundColor Cyan
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $env:UV_INSTALL_DIR = Join-Path $PSScriptRoot '.tools'
        $env:UV_NO_MODIFY_PATH = '1'
        Invoke-RestMethod https://astral.sh/uv/install.ps1 | Invoke-Expression
        if (-not (Test-Path -LiteralPath $uv)) { throw 'Could not download uv. Check your internet connection and try again.' }
    }
}

if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot '.venv'))) {
    Write-Host '  Installing Python and converters (one time, about a minute)...' -ForegroundColor Cyan
}
& $uv run --quiet --project $PSScriptRoot anytomd @args
exit $LASTEXITCODE
