#!/usr/bin/env python3
"""Headless-прогон для калибровки баланса.

    python tools/calibrate.py [--ticks N] [--seeds 1,2,3] [ПАРАМЕТР=ЗНАЧЕНИЕ ...]

Переопределения применяются к config ДО импорта модулей симуляции, поэтому
подбирать баланс можно, не редактируя файл:

    python tools/calibrate.py --ticks 20000 PLANT_SPAWN_RATE=4 PREDATION_EFFICIENCY=0.5
"""

import argparse
import ast
import os
import statistics
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
os.environ.setdefault("SDL_VIDEODRIVER", "dummy")

# Порядок важен: config правится ДО того, как симуляция его прочитает.
import lifegame.config as config  # noqa: E402


def apply_overrides(pairs):
    for pair in pairs:
        key, _, raw = pair.partition("=")
        if not hasattr(config, key):
            raise SystemExit(f"нет такого параметра в config: {key}")
        setattr(config, key, ast.literal_eval(raw))
    # производные значения
    config.VEGETARIAN_BASE_GENOM = [g[1] for g in config.GENES]


def run(seed, ticks, report_every):
    from lifegame.world import World      # импорт строго после правки config

    world = World(seed=seed)
    series = {"plants": [], "vegetarians": [], "predators": []}
    started = time.time()
    veg_extinct_at = pred_extinct_at = None

    for _ in range(ticks):
        world.step()
        if world.tick % 50 == 0:
            snap = world.snapshot()
            for key in series:
                series[key].append(snap[key])
        if pred_extinct_at is None and not world.predators:
            pred_extinct_at = world.tick
        if veg_extinct_at is None and not world.vegetarians:
            veg_extinct_at = world.tick
            break
        if world.tick % report_every == 0:
            s = world.snapshot()
            print(f"  тик {s['tick']:>6}  растений {s['plants']:>5}  "
                  f"трав. {s['vegetarians']:>5}  хищн. {s['predators']:>4}")

    # второй половине прогона доверяем больше: переходный процесс уже позади
    tail = {k: v[len(v) // 2:] for k, v in series.items()}
    print(f"  seed {seed}: {world.tick} тиков за {time.time() - started:.1f} c")
    for key, values in tail.items():
        if values:
            print(f"    {key:<12} среднее {statistics.mean(values):8.1f}  "
                  f"мин {min(values):6}  макс {max(values):6}")
    if veg_extinct_at:
        print(f"    ВЫМИРАНИЕ травоядных на тике {veg_extinct_at}")
    if pred_extinct_at:
        print(f"    вымирание хищников на тике {pred_extinct_at}")
    return world


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ticks", type=int, default=10000)
    parser.add_argument("--seeds", default="12345")
    parser.add_argument("--report-every", type=int, default=2000)
    parser.add_argument("overrides", nargs="*")
    args = parser.parse_args()

    apply_overrides(args.overrides)
    if args.overrides:
        print("переопределения:", " ".join(args.overrides))
    for seed in (int(s) for s in args.seeds.split(",")):
        run(seed, args.ticks, args.report_every)


if __name__ == "__main__":
    main()
