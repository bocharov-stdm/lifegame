"""Запуск игры: python start.py"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent

if __name__ == "__main__":
    sys.exit(subprocess.call([sys.executable, "main.py"], cwd=ROOT))
