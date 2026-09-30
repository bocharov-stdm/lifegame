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

rem Cargo never deletes its old builds: every new compiler or dependency version leaves the old
rem ones behind for good. Past CACHE_LIMIT GB they are dropped and built anew; the sweeps' results
rem in target\sweeps stay. rmdir, not cargo clean: it goes on past the files an editor holds open.
set CACHE_LIMIT=8
set CACHE_GB=0
for /f %%s in ('powershell -NoProfile -Command "[int][math]::Floor(((Get-ChildItem -LiteralPath target\debug,target\release -Recurse -File -Force -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum) / 1GB)"') do set CACHE_GB=%%s
if %CACHE_GB% GEQ %CACHE_LIMIT% (
    echo Кэш сборки вырос до %CACHE_GB% ГБ: убираю старые сборки, эта сборка будет дольше...
    rmdir /s /q target\debug 2>nul
    rmdir /s /q target\release 2>nul
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
