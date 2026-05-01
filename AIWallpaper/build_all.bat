@echo off
setlocal enabledelayedexpansion

echo [1/3] Building React frontend (ui-pro)...
cd ui-pro
call npm install
call npm run build
if %errorlevel% neq 0 (
    echo [ERROR] Frontend build failed. Please check Node.js environment and dependencies.
    pause
    exit /b %errorlevel%
)
cd ..

echo.
echo [2/3] Compiling Rust backend (Release mode)...
cargo build --release
if %errorlevel% neq 0 (
    echo [ERROR] Backend compilation failed. Please check Rust environment.
    pause
    exit /b %errorlevel%
)

echo.
echo [3/3] Organizing distribution files...
set DIST_DIR=dist_package
if exist %DIST_DIR% rd /s /q %DIST_DIR%
mkdir %DIST_DIR%

copy target\release\ai-wallpaper.exe %DIST_DIR%\
copy .env.example %DIST_DIR%\.env
mkdir %DIST_DIR%\assets
xcopy /e /i /y assets %DIST_DIR%\assets

echo.
echo ==================================================
echo Build completed!
echo Distribution files are in: %DIST_DIR%
echo Run %DIST_DIR%\ai-wallpaper.exe to start the app.
echo ==================================================
pause
