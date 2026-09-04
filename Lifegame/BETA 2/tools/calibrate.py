#!/usr/bin/env python3
"""Headless-прогон для калибровки баланса: python tools/calibrate.py [тиков] [сиды...]"""

import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
os.environ.setdefault("SDL_VIDEODRIVER", "dummy")

from world import World  # noqa: E402


def run(seed, ticks, report_every=2000):
    world = World(seed=seed)
    started = time.time()
    for _ in range(ticks):
        world.step()
        if world.tick % report_every == 0:
            s = world.snapshot()
            print(f"  seed {seed} тик {s['tick']:>6}  растений {s['plants']:>5}  "
                  f"трав. {s['vegetarians']:>5}  хищн. {s['predators']:>4}")
        if world.extinct:
            print(f"  seed {seed}: вымирание на тике {world.tick}")
            break
    print(f"  seed {seed}: {world.tick} тиков за {time.time() - started:.1f} c")
    return world


if __name__ == "__main__":
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10000
    seeds = [int(a) for a in sys.argv[2:]] or [12345]
    for s in seeds:
        run(s, n)
