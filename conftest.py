# conftest.py — тесты не открывают окно и находят пакет из корня репозитория.
import os
import sys

os.environ.setdefault("SDL_VIDEODRIVER", "dummy")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
