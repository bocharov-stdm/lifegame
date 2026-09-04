# world.py — логика симуляции без единой строчки про окно.
#
# Тем же кодом пользуются игра (main.py), калибровка (tools/calibrate.py) и
# тесты, поэтому баланс проверяется ровно на том, что потом крутится на экране.

import random

from .config import (PLANTS_AT_START, PLANT_SPAWN_RATE, REPRO_EVERY,
                    SPATIAL_CELL_SIZE, VEGETARIANS_AT_START, PREDATORS_AT_START)
from .plant import Plant
from .predator import Predator
from .spatial import SpatialHash
from .vegetarian import Vegetarian


class World:
    def __init__(self, seed=None):
        self.seed = seed
        if seed is not None:
            random.seed(seed)

        self.tick = 0
        self.plants = [Plant() for _ in range(PLANTS_AT_START)]
        self.vegetarians = [Vegetarian() for _ in range(VEGETARIANS_AT_START)]
        self.predators = [Predator() for _ in range(PREDATORS_AT_START)]

        self.plant_grid = SpatialHash(SPATIAL_CELL_SIZE)
        self.veg_grid = SpatialHash(SPATIAL_CELL_SIZE)
        self.pred_grid = SpatialHash(SPATIAL_CELL_SIZE)

    # ── один тик ─────────────────────────────────────────────────────────────
    def step(self):
        self._spawn_plants()

        self.plant_grid.rebuild(self.plants)
        self.veg_grid.rebuild(self.vegetarians)
        self.pred_grid.rebuild(self.predators)

        breeding = self.tick % REPRO_EVERY == 0
        # Дети копятся в буфере и вступают в игру со следующего тика — иначе
        # они успевали походить и поесть в тик собственного рождения.
        veg_offspring = []
        pred_offspring = []

        for pr in self.predators:
            pr.update(self.veg_grid)
            if breeding and pr.alive:
                pr.maybe_divide(pred_offspring)

        for v in self.vegetarians:
            if not v.alive:              # съеден хищником в этом же тике
                continue
            v.update(self.plant_grid, self.pred_grid)
            if breeding and v.alive:
                v.maybe_divide(veg_offspring)

        # единственное место, где кто-либо физически удаляется из списков
        self.plants = [p for p in self.plants if p.alive]
        self.vegetarians = [v for v in self.vegetarians if v.alive] + veg_offspring
        self.predators = [p for p in self.predators if p.alive] + pred_offspring

        self.tick += 1

    def _spawn_plants(self):
        """PLANT_SPAWN_RATE — ожидаемое число растений за тик; дробная часть
        разыгрывается честно (раньше «вероятность» была больше 1 и давала
        ровно одно растение детерминированно)."""
        count = int(PLANT_SPAWN_RATE)
        if random.random() < PLANT_SPAWN_RATE - count:
            count += 1
        for _ in range(count):
            self.plants.append(Plant())

    # ── сводка ───────────────────────────────────────────────────────────────
    def snapshot(self):
        return {
            "tick": self.tick,
            "plants": len(self.plants),
            "vegetarians": len(self.vegetarians),
            "predators": len(self.predators),
        }

    @property
    def extinct(self):
        return not self.vegetarians and not self.predators
