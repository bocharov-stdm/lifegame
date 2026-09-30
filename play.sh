#!/bin/sh
# Launch lifegame on Linux and macOS: build if needed and open the window.
#   sh play.sh                     # menu
#   sh play.sh --scale 100 --seed 7
set -e
cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Не найден Rust (cargo). Установите его с https://rustup.rs и запустите этот файл снова."
    exit 1
fi

# Cargo never deletes its old builds: every new compiler or dependency version leaves the old ones
# behind for good. Past 8 GB they are dropped and built anew; the sweeps' results in target/sweeps
# stay.
cache_kb=$(du -sk target/debug target/release 2>/dev/null | awk '{ s += $1 } END { print s + 0 }')
if [ "$cache_kb" -ge $((8 * 1024 * 1024)) ]; then
    echo "Кэш сборки вырос до $((cache_kb / 1024 / 1024)) ГБ: убираю старые сборки, эта сборка будет дольше..."
    rm -rf target/debug target/release
fi

echo "Собираю игру. Первый раз это несколько минут, дальше секунды..."
cargo build -p life-app --release
exec ./target/release/life-app "$@"
