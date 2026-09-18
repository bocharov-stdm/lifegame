# world.py — состояние симуляции и один логический тик.
#
# Здесь нет ни pygame-окна, ни отрисовки: World можно гонять headless
# (тесты, подбор баланса на 100k тиков). Рисует и ловит события пакет app/.

import random

from .config     import *
from .grid       import Grid
from .plant      import Plant
from .vegetarian import Vegetarian
from .predator   import Predator
from .rules      import DEFAULT_RULES


class World:
    """Популяции и правила одного тика."""

    def __init__(self, seed=None, rules=None,
                 n_vegetarians=VEGETARIANS_AT_START,
                 n_predators=PREDATORS_AT_START,
                 predator_speed=PREDATOR_BASE_SPEED,
                 predator_vision=PREDATOR_BASE_VISION):
        """Мир со стартовыми популяциями.

        rules — правила мира (life/rules.py); без них — значения из config.py.
        При аргументах по умолчанию случайные числа тянутся в прежнем порядке,
        поэтому прогоны с тем же сидом не сдвигаются.
        """
        if seed is not None:
            random.seed(seed)
        self.rules = rules = rules or DEFAULT_RULES

        self.plants      = [Plant() for _ in range(PLANTS_AT_START)]
        self.vegetarians = [Vegetarian(rules=rules) for _ in range(n_vegetarians)]
        self.predators   = [Predator(speed=predator_speed, vision=predator_vision,
                                     rules=rules)
                            for _ in range(n_predators)]

        self.predator_speed  = predator_speed      # такими приходят мигранты
        self.predator_vision = predator_vision
        self.hunting         = n_predators > 0     # мир без охоты мигрантов не ждёт
        self.migrants        = 0

        self.tick = 0

    # ── один логический тик ─────────────────────────────────────────────────
    def step(self):
        self._spawn_plants()
        self._update_predators()
        self._update_vegetarians()
        self.tick += 1
        self._migrate_predators()           # после счёта: на тике 0 мигрантов нет

    def _spawn_plants(self):
        # plant_rate (по умолчанию PLANT_SPAWN_CHANCE) — это ожидаемое число
        # растений за тик, а не вероятность: целую часть спауним всегда,
        # дробную — с соответствующим шансом.
        #
        # Выше PLANT_MAX не растём: без травоядных растения копились бы бесконечно.
        # Жребий тянется в любом случае — так поток случайных чисел не зависит от
        # того, упёрлись ли мы в потолок. Съеденное выметено в конце прошлого тика,
        # поэтому len(self.plants) здесь — ровно живые растения.
        rate = self.rules.plant_rate
        for _ in range(TICKS_PER_FRAME):
            count = int(rate)
            if random.random() < rate - count:
                count += 1
            count = min(count, max(0, PLANT_MAX - len(self.plants)))
            self.plants.extend(Plant() for _ in range(count))

    def _migrate_predators(self):
        # Случайные числа тянутся только при срабатывании, см. config.py.
        period = int(self.rules.predator_migration)
        if (period <= 0 or not self.hunting or self.tick % period
                or len(self.predators) >= PREDATOR_MIGRATION_MIN
                or len(self.vegetarians) < PREDATOR_MIGRATION_PREY):
            return
        d = PREDATOR_DIAM
        if random.random() < 0.5:           # левый/правый край
            x = random.choice((d, WORLD_WIDTH - d))
            y = random.uniform(d, WORLD_HEIGHT - d)
        else:                               # верхний/нижний
            x = random.uniform(d, WORLD_WIDTH - d)
            y = random.choice((d, WORLD_HEIGHT - d))
        self.predators.append(Predator(x, y, speed=self.predator_speed,
                                       vision=self.predator_vision, rules=self.rules))
        self.migrants += 1

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
        # Хищник видит и ловит по краю тела добычи, поэтому к радиусу запроса
        # прибавляется половина самого крупного травоядного.
        half = max((v.half for v in self.vegetarians), default=0.0)
        cell = max(GRID_MIN_CELL, PREDATOR_DIAM / 2 + half,
                   max((pr.vision for pr in self.predators), default=0.0) + half)
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
