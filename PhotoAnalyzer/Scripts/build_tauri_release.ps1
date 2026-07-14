$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$rootDir = (Resolve-Path (Join-Path $scriptDir "..")).Path
$desktopDir = Join-Path $rootDir "desktop"
Write-Host "[1/1] Build Tauri desktop app with embedded backend..."
Push-Location $desktopDir
try {
    npm run tauri:build
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed." }
}
finally {
    Pop-Location
}

Write-Host ""
Write-Host "Tauri build complete."
Write-Host "Artifacts: $desktopDir\target\release\bundle"
