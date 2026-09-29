@echo off
chcp 65001 >nul
rem Launch lifegame on Windows: build if needed and open the window.
rem A double click opens the menu. From a console you can pass flags:
rem   play.bat --scale 100 --seed 7
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo Не найден Rust ^(cargo^). Установите его с https://rustup.rs и запустите этот файл снова.
    pause
    exit /b 1
)

echo Собираю игру. Первый раз это несколько минут, дальше секунды...
cargo build -p life-app --release
if errorlevel 1 (
    echo.
    echo Сборка не удалась, подробности выше.
    pause
    exit /b 1
)

rem Without flags the window starts apart and the console closes.
rem With flags the game runs in this console, so that errors in them are visible.
if "%~1"=="" (
    start "" "target\release\life-app.exe"
) else (
    "target\release\life-app.exe" %*
)
