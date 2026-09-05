# world.py — состояние симуляции и один логический тик.
#
# Здесь нет ни pygame-окна, ни отрисовки: World можно гонять headless
# (тесты, подбор баланса на 100k тиков). Рисует и обрабатывает события main.py.

import random

from config     import *
from grid       import Grid
from plant      import Plant
from vegetarian import Vegetarian
from predator   import Predator


class World:
    """Популяции и правила одного тика."""

    def __init__(self, seed=None):
        if seed is not None:
            random.seed(seed)

        self.plants      = [Plant()      for _ in range(PLANTS_AT_START)]
        self.vegetarians = [Vegetarian() for _ in range(VEGETARIANS_AT_START)]
        self.predators   = [Predator()   for _ in range(PREDATORS_AT_START)]

        self.tick = 0

    # ── один логический тик ─────────────────────────────────────────────────
    def step(self):
        self._spawn_plants()
        self._update_predators()
        self._update_vegetarians()
        self.tick += 1

    def _spawn_plants(self):
        # PLANT_SPAWN_CHANCE — это ожидаемое число растений за тик, а не вероятность:
        # целую часть спауним всегда, дробную — с соответствующим шансом
        for _ in range(TICKS_PER_FRAME):
            count = int(PLANT_SPAWN_CHANCE)
            if random.random() < PLANT_SPAWN_CHANCE - count:
                count += 1
            self.plants.extend(Plant() for _ in range(count))

    # ── поиск соседей ───────────────────────────────────────────────────────
    # Существа по-прежнему просто перебирают всех, кого им дали, — но дают им
    # теперь не весь мир, а соседей по сетке. Индексацией занимается мир,
    # сущности остаются наивными.
    #
    # Размер клетки считается по САМОМУ БОЛЬШОМУ радиусу запроса: блок 3x3
    # накрывает всё, что ближе одной клетки, и при слишком мелкой клетке
    # существа начали бы не замечать соседей у себя под носом. Зрение — ген,
    # оно эволюционирует, поэтому размер пересчитывается каждый тик.

    def _update_predators(self):
        cell = max(GRID_MIN_CELL, PREDATOR_DIAM,
                   max((pr.vision for pr in self.predators), default=0.0))
        prey = Grid(cell, self.vegetarians)

        offspring     = []     # дети текущего тика
        new_predators = []

        for pr in self.predators:
            pr.move(prey.near(pr.x, pr.y))
            pr.try_eat(prey.near(pr.x, pr.y))   # уже с новой позиции

            if self.tick % DIVIDE_PERIOD == 0:
                pr.maybe_divide(offspring)     # ← в буфер, а не в список итерации

            if pr.alive:
                new_predators.append(pr)

        self.predators = new_predators + offspring

    def _update_vegetarians(self):
        cell = max(GRID_MIN_CELL,
                   max((max(v.vision, v.size) for v in self.vegetarians), default=0.0))
        food    = Grid(cell, self.plants)
        hunters = Grid(cell, self.predators)

        offspring       = []   # дети текущего тика
        new_vegetarians = []

        for v in self.vegetarians:
            if not v.alive:                    # съеден хищником в этом же тике
                continue

            v.move(food.near(v.x, v.y), hunters.near(v.x, v.y))
            v.try_eat(food.near(v.x, v.y))     # уже с новой позиции

            if self.tick % DIVIDE_PERIOD == 0:
                v.maybe_divide(offspring)

            if v.alive:
                new_vegetarians.append(v)

        self.vegetarians = new_vegetarians + offspring
        self.plants      = [p for p in self.plants if p.alive]   # выметаем съеденное

    # ── статистика ──────────────────────────────────────────────────────────
    def stats(self):
        """Сводка по популяции; avg_genom/avg_energy — None, если все вымерли."""
        vegetarians = self.vegetarians
        avg_genom = avg_energy = None

        if vegetarians:
            n = len(vegetarians)
            avg_genom  = [sum(v.genom[i] for v in vegetarians) / n
                          for i in range(len(vegetarians[0].genom))]
            avg_energy = sum(v.energy for v in vegetarians) / n

        return {
            "tick":       self.tick,
            "plants":     len(self.plants),
            "vegetarians": len(vegetarians),
            "predators":  len(self.predators),
            "avg_genom":  avg_genom,
            "avg_energy": avg_energy,
        }
