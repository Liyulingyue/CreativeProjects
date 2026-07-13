$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$rootDir = (Resolve-Path (Join-Path $scriptDir "..")).Path
$frontendDir = Join-Path $rootDir "web\frontend"
$rustDir = Join-Path $rootDir "rust"
$tauriBinDir = Join-Path $frontendDir "src-tauri\bin"
$backendExe = Join-Path $rustDir "target\release\photo_analyzer.exe"
$backendDst = Join-Path $tauriBinDir "photo_analyzer_backend.exe"

Write-Host "[1/3] Build backend exe (embedded frontend API server)..."
Push-Location $rustDir
try {
    cargo build --release --features embed-frontend
    if ($LASTEXITCODE -ne 0) { throw "Backend build failed." }
}
finally {
    Pop-Location
}

Write-Host "[2/3] Copy backend exe into Tauri resources..."
New-Item -ItemType Directory -Force -Path $tauriBinDir | Out-Null
Copy-Item -Force $backendExe $backendDst

Write-Host "[3/3] Build Tauri desktop app..."
Push-Location $frontendDir
try {
    npm run tauri:build
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed." }
}
finally {
    Pop-Location
}

Write-Host ""
Write-Host "Tauri build complete."
Write-Host "Backend resource: $backendDst"
Write-Host "Artifacts: $frontendDir\src-tauri\target\release\bundle"
