@echo off
setlocal enabledelayedexpansion

REM ------------------------------------------------------------
REM build_embed_release.bat
REM Purpose:
REM   1) Build frontend dist assets (Vite)
REM   2) Build Rust release executable with embedded frontend
REM Output:
REM   rust\target\release\photo_analyzer.exe
REM ------------------------------------------------------------

REM NOTE:
REM Keep BAT comments ASCII-only for cmd.exe compatibility.
REM Some code pages may mis-parse non-ASCII REM lines and execute garbled fragments.

REM Resolve project root based on script location.
REM %~dp0 points to Scripts\ folder.
REM ROOT_DIR becomes parent folder of Scripts (PhotoAnalyzer\).
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..") do set "ROOT_DIR=%%~fI"

REM Key paths used by this script.
set "FRONTEND_DIR=%ROOT_DIR%\web\frontend"
set "RUST_DIR=%ROOT_DIR%\rust"
set "EXE_PATH=%RUST_DIR%\target\release\photo_analyzer.exe"

REM ==========================
REM Step 1: Build frontend
REM ==========================
echo [1/2] Building frontend dist...

REM pushd changes current directory and stores previous dir on stack.
REM Fail early if frontend directory is missing.
pushd "%FRONTEND_DIR%" || (
  echo ERROR: frontend directory not found: %FRONTEND_DIR%
  exit /b 1
)

REM Use call so control always returns to this script.
call npm run build

REM Non-zero exit code means build failed.
if errorlevel 1 (
  echo ERROR: frontend build failed.
  popd
  exit /b 1
)

REM Restore previous directory after step 1.
popd

REM ==========================
REM Step 2: Build Rust release
REM ==========================
echo [2/2] Building Rust release with embedded frontend...
pushd "%RUST_DIR%" || (
  echo ERROR: rust directory not found: %RUST_DIR%
  exit /b 1
)

REM embed-frontend feature embeds frontend assets into the binary.
call cargo build --release --features embed-frontend
if errorlevel 1 (
  echo ERROR: rust release build failed.
  popd
  exit /b 1
)
popd

REM Final sanity check: verify exe exists and print size.
if exist "%EXE_PATH%" (
  echo.
  echo Build complete.
  echo EXE: %EXE_PATH%
  for %%F in ("%EXE_PATH%") do echo Size: %%~zF bytes
) else (
  echo ERROR: build finished but exe not found: %EXE_PATH%
  exit /b 1
)

REM endlocal restores environment variables changed in this script.
endlocal
