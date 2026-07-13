$ErrorActionPreference = "Stop"

# ------------------------------------------------------------
# build_embed_release.ps1
# 作用：
#   1) 构建前端 dist 资源
#   2) 构建内嵌前端的 Rust Release 可执行文件
# 输出：
#   rust\target\release\photo_analyzer.exe
# ------------------------------------------------------------

# 根据脚本路径推导项目根目录。
# $MyInvocation.MyCommand.Path 指向当前 ps1 脚本文件。
# rootDir 是 Scripts 的上一级目录，即 PhotoAnalyzer\。
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$rootDir = (Resolve-Path (Join-Path $scriptDir "..")).Path

# 打包流程需要使用的关键路径。
$frontendDir = Join-Path $rootDir "web\frontend"
$rustDir = Join-Path $rootDir "rust"
$exePath = Join-Path $rustDir "target\release\photo_analyzer.exe"

# ==========================
# 第 1 步：构建前端
# ==========================
Write-Host "[1/2] Building frontend dist..."

# Push-Location/Pop-Location 保证结束后回到原目录。
# try/finally 保证即使失败也会执行 Pop-Location。
Push-Location $frontendDir
try {
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "Frontend build failed." }
}
finally {
    Pop-Location
}

# ==========================
# 第 2 步：构建 Rust Release
# ==========================
Write-Host "[2/2] Building Rust release with embedded frontend..."
Push-Location $rustDir
try {
    # embed-frontend 特性会把前端产物嵌入二进制。
    cargo build --release --features embed-frontend
    if ($LASTEXITCODE -ne 0) { throw "Rust release build failed." }
}
finally {
    Pop-Location
}

# 最后校验：目标 exe 必须存在。
if (-not (Test-Path $exePath)) {
    throw "Build completed but exe not found: $exePath"
}

# 打印构建结果摘要，便于复制和确认。
$exe = Get-Item $exePath
Write-Host ""
Write-Host "Build complete."
Write-Host "EXE: $($exe.FullName)"
Write-Host "Size: $($exe.Length) bytes"
