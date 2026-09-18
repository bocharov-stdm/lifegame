# history.py — история партии для графиков.
#
# Точка снимается раз в GRAPH_EVERY тиков: численности и средний геном.
# Рядов два:
#   recent — последние RECENT_POINTS точек (окно в 3000 тиков): видно
#            колебания «хищник — жертва» крупно;
#   full   — вся партия. Когда точек становится больше FULL_POINTS, ряд
#            прореживается вдвое (каждая вторая точка), а шаг записи
#            удваивается. Память ограничена при любой длине партии.
#
# Численности в точке — средние за последние SMOOTH_TICKS тиков. Существа
# делятся разом раз в DIVIDE_PERIOD тиков, и мгновенные числа рисуют пилу,
# за которой не видно самих колебаний; среднее по тому же периоду её снимает.
#
# pygame здесь не нужен: модуль проверяют тесты без дисплея.

from collections import deque
from typing import NamedTuple, Optional, Tuple

from life.config import DIVIDE_PERIOD

GRAPH_EVERY   = 10
RECENT_POINTS = 300
FULL_POINTS   = 600
SMOOTH_TICKS  = DIVIDE_PERIOD


class Sample(NamedTuple):
    tick:        int
    plants:      float        # численности — средние за SMOOTH_TICKS тиков
    vegetarians: float
    predators:   float
    genom:       Optional[Tuple[float, ...]]    # средний геном; None — травоядных нет


def counts_of(world):
    return len(world.plants), len(world.vegetarians), len(world.predators)


def sample_of(world, counts=None):
    """Точка графика; counts — сглаженные численности (по умолчанию текущие)."""
    s = world.stats()
    genom = tuple(s["avg_genom"]) if s["avg_genom"] is not None else None
    plants, vegetarians, predators = counts if counts is not None else counts_of(world)
    return Sample(s["tick"], plants, vegetarians, predators, genom)


class History:
    def __init__(self, every=GRAPH_EVERY, recent=RECENT_POINTS, full=FULL_POINTS):
        self.every    = every
        self.recent   = deque(maxlen=recent)
        self.full     = []
        self.full_cap = full
        self.stride   = 1            # в full лежит каждая stride-я точка
        self.count    = 0            # сколько точек записано всего
        self.version  = 0            # растёт с каждой точкой — ключ для кэша графиков
        self.origin   = None         # первый средний геном партии — база «изменения от начала»
        self._window  = deque(maxlen=SMOOTH_TICKS)   # численности последних тиков

    def record(self, world):
        """Вызывается каждый тик; точку берёт только раз в every тиков."""
        window = self._window
        window.append(counts_of(world))
        if world.tick % self.every == 0:
            n = len(window)
            smooth = tuple(sum(c[k] for c in window) / n for k in range(3))
            self.add(sample_of(world, smooth))

    def add(self, sample):
        if self.origin is None and sample.genom is not None:
            self.origin = sample.genom
        self.recent.append(sample)
        if self.count % self.stride == 0:
            self.full.append(sample)
            if len(self.full) > self.full_cap:
                # full[i] — точка номер i * stride, поэтому full[::2] — ровно
                # точки с номерами, кратными 2 * stride
                self.full = self.full[::2]
                self.stride *= 2
        self.count   += 1
        self.version += 1

    def series(self, whole):
        """Точки для графика: вся партия (whole) или последнее окно.

        Последняя точка есть всегда: при прореживании она могла не попасть в full.
        """
        if not whole:
            return list(self.recent)
        points = list(self.full)
        if self.recent and (not points or points[-1] is not self.recent[-1]):
            points.append(self.recent[-1])
        return points

    @property
    def last(self):
        return self.recent[-1] if self.recent else None
