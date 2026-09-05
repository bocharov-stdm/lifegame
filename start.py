"""Единая точка входа: запуск игры, тестов или калибратора.

Запуск без аргументов открывает игру. Для остального — подкоманда:

    python start.py            # игра (то же самое, что python -m lifegame)
    python start.py test       # python -m pytest -q
    python start.py calibrate  # python tools/calibrate.py (остальные аргументы прокидываются как есть)
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def run_game():
    return subprocess.call([sys.executable, "-m", "lifegame"], cwd=ROOT)


def run_tests(extra_args):
    return subprocess.call([sys.executable, "-m", "pytest", "-q", *extra_args], cwd=ROOT)


def run_calibrate(extra_args):
    return subprocess.call([sys.executable, "tools/calibrate.py", *extra_args], cwd=ROOT)


def main():
    args = sys.argv[1:]
    if not args:
        return run_game()

    command, rest = args[0], args[1:]
    if command in ("game", "run"):
        return run_game()
    if command in ("test", "tests"):
        return run_tests(rest)
    if command in ("calibrate", "calibrator"):
        return run_calibrate(rest)

    print(f"Неизвестная команда: {command!r}. Доступно: (пусто)/game, test, calibrate.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
