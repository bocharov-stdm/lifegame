@echo off
chcp 65001 >nul
rem Запуск игры Tiny Life на Windows: собрать, если нужно, и открыть окно.
rem Двойной щелчок открывает меню. Из консоли можно передать флаги:
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

rem Без флагов окно запускается отдельно, и консоль закрывается.
rem С флагами игра идёт в этой консоли, чтобы были видны ошибки в них.
if "%~1"=="" (
    start "" "target\release\life-app.exe"
) else (
    "target\release\life-app.exe" %*
)
