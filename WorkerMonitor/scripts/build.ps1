# Auto-download model if not exists
function Ensure-Model {
    $modelPath = "$PSScriptRoot\..\backend\resource\end2end.onnx"
    if (Test-Path $modelPath) {
        Write-Host "[OK] Model exists: $modelPath"
        return
    }

    $zipPath = "$PSScriptRoot\..\rtmpose.zip"
    $extractBase = "$PSScriptRoot\..\rtmpose-extract"
    $MODEL_URL = "https://download.openmmlab.com/mmpose/v1/projects/rtmposev1/onnx_sdk/rtmpose-t_simcc-body7_pt-body7_420e-256x192-026a1439_20230504.zip"

    Write-Host "[INFO] Model not found, downloading..."
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocol]::Tls12
    Invoke-WebRequest -Uri $MODEL_URL -OutFile $zipPath -UseBasicParsing

    if (Test-Path $extractBase) {
        Remove-Item $extractBase -Recurse -Force
    }
    Expand-Archive -Path $zipPath -DestinationPath $extractBase -Force

    $onnxFiles = Get-ChildItem -Path $extractBase -Filter "*.onnx" -Recurse -File
    if ($onnxFiles.Count -eq 0) {
        Write-Error "No .onnx file found in archive"
        exit 1
    }

    $onnxFile = $onnxFiles[0]
    Write-Host "[INFO] Found: $($onnxFile.FullName)"
    Copy-Item $onnxFile.FullName -Destination $modelPath -Force
    Write-Host "[OK] Copied to: $modelPath"

    Remove-Item $zipPath -Force
    Remove-Item $extractBase -Recurse -Force
}

# Build backend
function Build-Backend {
    Write-Host ""
    Write-Host "[BUILD] Backend (Rust)..."
    $env:RTMPOSE_MODEL = "$PSScriptRoot\..\backend\resource\end2end.onnx"
    cargo build --release --manifest-path "$PSScriptRoot\..\backend\Cargo.toml"
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Backend build failed"
        exit 1
    }
    Write-Host "[OK] Backend built"
}

# Build frontend
function Build-Frontend {
    Write-Host ""
    Write-Host "[BUILD] Frontend (React)..."
    Push-Location "$PSScriptRoot\..\frontend"
    npm install
    npm run build
    Pop-Location
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Frontend build failed"
        exit 1
    }
    Write-Host "[OK] Frontend built"
}

# Build Tauri app
function Build-Tauri {
    Write-Host ""
    Write-Host "[BUILD] Tauri app..."
    Push-Location "$PSScriptRoot\..\src-tauri"
    cargo tauri build
    Pop-Location
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Tauri build failed"
        exit 1
    }
    Write-Host "[OK] Tauri built"
}

$Action = $args[0]
if ($Action -eq "model") {
    Ensure-Model
} elseif ($Action -eq "backend") {
    Ensure-Model
    Build-Backend
} elseif ($Action -eq "frontend") {
    Build-Frontend
} elseif ($Action -eq "tauri") {
    Build-Tauri
} elseif ($Action -eq "all") {
    Ensure-Model
    Build-Backend
    Build-Frontend
    Build-Tauri
} else {
    Write-Host "Usage: .\build.ps1 <model|backend|frontend|tauri|all>"
    Write-Host "  model    - Download ONNX model if missing"
    Write-Host "  backend  - Build backend (Rust)"
    Write-Host "  frontend - Build frontend (React)"
    Write-Host "  tauri   - Build Tauri desktop app"
    Write-Host "  all     - Build everything"
}
