"""Раскладка пакета.

Эти проверки ловят поломки, которые иначе всплывают не у всех: код, работающий
при запуске из папки пакета, но падающий из корня, и «тихое» отключение подмены
параметров в калибраторе.
"""

import ast
import importlib
import pathlib
import subprocess
import sys

PACKAGE = pathlib.Path(__file__).resolve().parent.parent / "lifegame"
ROOT = PACKAGE.parent


def _sources():
    return sorted(PACKAGE.glob("*.py"))


def test_entry_point_is_importable():
    """python -m lifegame должен находить точку входа."""
    module = importlib.import_module("lifegame.__main__")
    assert callable(module.main)


def test_package_has_no_absolute_intra_package_imports():
    """`from config import ...` работает из папки пакета и падает из корня."""
    internal = {p.stem for p in _sources()} | {"tests", "tools"}
    offenders = []
    for path in _sources():
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom) and node.level == 0:
                if node.module and node.module.split(".")[0] in internal:
                    offenders.append(f"{path.name}: from {node.module} import ...")
            elif isinstance(node, ast.Import):
                for alias in node.names:
                    if alias.name.split(".")[0] in internal:
                        offenders.append(f"{path.name}: import {alias.name}")
    assert not offenders, "нужны относительные импорты: " + "; ".join(offenders)


def test_package_init_does_not_import_simulation():
    """От этого зависит подмена параметров в tools/calibrate.py."""
    source = (PACKAGE / "__init__.py").read_text(encoding="utf-8")
    tree = ast.parse(source)
    imports = [n for n in ast.walk(tree) if isinstance(n, (ast.Import, ast.ImportFrom))]
    assert not imports, "__init__.py должен оставаться без импортов"


def test_config_override_survives_before_world_import():
    """Сквозная проверка механизма калибратора в отдельном процессе."""
    code = (
        "import lifegame.config as config\n"
        "config.PLANTS_AT_START = 4321\n"
        "from lifegame.world import World\n"
        "w = World(seed=1)\n"
        "print(len(w.plants))\n"
    )
    result = subprocess.run(
        [sys.executable, "-c", code], cwd=ROOT, capture_output=True, text=True,
        env={"SDL_VIDEODRIVER": "dummy", "PATH": "/usr/bin:/bin"},
    )
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip().splitlines()[-1] == "4321"
