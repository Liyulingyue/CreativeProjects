@echo off
setlocal enabledelayedexpansion

echo [1/3] Building frontend...
cd /d "%~dp0"
call npm run build
if %errorlevel% neq 0 (
    echo [ERROR] Frontend build failed.
    pause
    exit /b %errorlevel
)

echo.
echo [2/3] Compiling Rust (Release)...
cd src-tauri
call cargo build --release
if %errorlevel% neq 0 (
    echo [ERROR] Rust build failed.
    pause
    exit /b %errorlevel
)
cd ..

echo.
echo [3/3] Packaging...
set DIST_DIR=dist_package
if exist %DIST_DIR% rd /s /q %DIST_DIR%
mkdir %DIST_DIR%
copy src-tauri\target\release\worker-monitor.exe %DIST_DIR%\

echo.
echo ==================================================
echo  Done!  dist_package\worker-monitor.exe
echo ==================================================
pause
