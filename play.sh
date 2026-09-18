#!/bin/sh
# Запуск игры Tiny Life на Linux и macOS: собрать, если нужно, и открыть окно.
#   sh play.sh                     # меню
#   sh play.sh --scale 100 --seed 7
set -e
cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Не найден Rust (cargo). Установите его с https://rustup.rs и запустите этот файл снова."
    exit 1
fi

echo "Собираю игру. Первый раз это несколько минут, дальше секунды..."
cargo build -p life-app --release
exec ./target/release/life-app "$@"
