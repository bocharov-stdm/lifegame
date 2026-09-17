# world.py — состояние симуляции и один логический тик.
#
# Здесь нет ни pygame-окна, ни отрисовки: World можно гонять headless
# (тесты, подбор баланса на 100k тиков). Рисует render.py, события ловит main.py.

import random

from .config     import *
from .grid       import Grid
from .plant      import Plant
from .vegetarian import Vegetarian
from .predator   import Predator


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
        # целую часть спауним всегда, дробную — с соответствующим шансом.
        #
        # Выше PLANT_MAX не растём: без травоядных растения копились бы бесконечно.
        # Жребий тянется в любом случае — так поток случайных чисел не зависит от
        # того, упёрлись ли мы в потолок. Съеденное выметено в конце прошлого тика,
        # поэтому len(self.plants) здесь — ровно живые растения.
        for _ in range(TICKS_PER_FRAME):
            count = int(PLANT_SPAWN_CHANCE)
            if random.random() < PLANT_SPAWN_CHANCE - count:
                count += 1
            count = min(count, max(0, PLANT_MAX - len(self.plants)))
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
        prey_near = Grid(cell, self.vegetarians).near
        divide    = self.tick % DIVIDE_PERIOD == 0

        offspring     = []     # дети текущего тика
        new_predators = []

        for pr in self.predators:
            pr.move(prey_near(pr.x, pr.y))
            if not pr.alive:                   # умер от голода на этом ходу:
                continue                       # мёртвый не охотится и не делится
            pr.try_eat(prey_near(pr.x, pr.y))   # уже с новой позиции

            if divide:
                pr.maybe_divide(offspring)     # ← в буфер, а не в список итерации

            new_predators.append(pr)

        self.predators = new_predators + offspring

    def _update_vegetarians(self):
        # Радиусы запросов травоядного: зрение (move) и размер (try_eat). Но
        # try_eat спрашивает не свою клетку, а ту же окрестность, что и move, —
        # вокруг позиции ДО шага. Шаг не длиннее speed, поэтому клетка берётся
        # не меньше size + speed: тогда блок 3x3 вокруг старой позиции накрывает
        # всё, до чего можно дотянуться с новой. Результат тот же — try_eat ест
        # всё в радиусе, порядок кандидатов ему не важен, — а запрос к сетке на
        # каждое травоядное на один меньше.
        cell = max(GRID_MIN_CELL,
                   max((max(v.vision, v.size + v.speed) for v in self.vegetarians),
                       default=0.0))
        # Здесь самый горячий цикл тика, поэтому методы взяты в локальные имена,
        # а без хищников сетку хищников не спрашиваем вовсе.
        food_near    = Grid(cell, self.plants).near
        hunters_near = Grid(cell, self.predators).near if self.predators else None
        no_hunters   = ()
        divide       = self.tick % DIVIDE_PERIOD == 0

        offspring       = []   # дети текущего тика
        new_vegetarians = []

        for v in self.vegetarians:
            if not v.alive:                    # съеден хищником в этом же тике
                continue

            x, y = v.x, v.y
            food = food_near(x, y)
            v.move(food, hunters_near(x, y) if hunters_near else no_hunters)
            if not v.alive:                    # умер от голода на этом ходу:
                continue                       # мёртвый не ест и не делится
            v.try_eat(food)                    # ест уже с новой позиции, см. выше

            if divide:
                v.maybe_divide(offspring)

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
