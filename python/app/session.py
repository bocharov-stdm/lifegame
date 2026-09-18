# session.py — идущая партия: мир, темп, история, события и конец игры.
#
# Всё, что про игру, но не про экран: окно только спрашивает у сессии, что
# рисовать, и передаёт ей нажатия. pygame здесь не нужен — модуль проверяют
# тесты без дисплея.
#
# СВОЙ ГЕНЕРАТОР СЛУЧАЙНЫХ ЧИСЕЛ
# Движок пользуется общим модулем random. Между кадрами им пользуются и
# другие — фоновый мир в меню, кубик сида. Поэтому сессия хранит своё
# состояние генератора и подставляет его на время своих тиков: партия с тем
# же сидом и настройками идёт одинаково, сколько бы её ни ставили на паузу и
# что бы ни происходило между кадрами. На этом держится «Заново».

import random
import time
from typing import NamedTuple

from .history import History, sample_of

SPEEDS          = (1, 2, 4, 8, 16, 32)   # тиков за кадр
FRAME_BUDGET    = 0.012                  # секунд на тики из 16.7 мс кадра
EXPLOSION_LIMIT = 3000                   # существ; дальше окно начинает тормозить


def spaced(n):
    """12345 -> «12 345»: крупные числа читаются легче."""
    return f"{n:,}".replace(",", " ")


class Event(NamedTuple):
    tick: int
    text: str


class Session:
    def __init__(self, settings, seed=None):
        self.settings = settings.copy()          # игра не меняется от правок в меню
        self.seed     = settings.seed if seed is None else seed
        saved = random.getstate()                # World(seed) пересеивает общий генератор
        self.world    = self.settings.make_world(self.seed)
        self._rng     = random.getstate()        # сразу после World(seed)
        random.setstate(saved)

        self.history = History()
        self.history.add(sample_of(self.world))

        self.paused      = False
        self.speed_index = 0
        self.lagging     = False                 # темп не укладывается в кадр
        self.tps         = 0.0                   # фактических тиков в секунду
        self._tps_time   = 0.0
        self._tps_ticks  = 0

        self.events          = []
        self.ended           = None              # None | "extinct" | "explosion"
        self.watch_explosion = True

        w = self.world
        self.peaks = [len(w.plants), len(w.vegetarians), len(w.predators)]
        self._had_vegetarians = bool(w.vegetarians)
        self._had_predators   = bool(w.predators)
        self._migrants        = w.migrants

    # ── темп ────────────────────────────────────────────────────────────────
    @property
    def speed(self):
        return SPEEDS[self.speed_index]

    def set_speed_index(self, index):
        self.speed_index = min(max(index, 0), len(SPEEDS) - 1)

    def faster(self):
        self.set_speed_index(self.speed_index + 1)

    def slower(self):
        self.set_speed_index(self.speed_index - 1)

    def toggle_pause(self):
        self.paused = not self.paused

    @property
    def running(self):
        return not self.paused and self.ended is None

    # ── тики ────────────────────────────────────────────────────────────────
    def advance(self, dt, budget=FRAME_BUDGET, clock=time.perf_counter):
        """Тики одного кадра: не больше speed и не дольше budget секунд.

        Хотя бы один тик за кадр делается всегда, так что партия идёт даже
        при огромной численности — просто медленнее, а окно не зависает.
        """
        done = self._run(self.speed, budget, clock) if self.running else 0
        self._measure(dt, done)
        return done

    def step_once(self):
        """Один тик по кнопке «шаг» — только на паузе."""
        if not self.paused or self.ended is not None:
            return 0
        return self._run(1, None, time.perf_counter)

    def _run(self, ticks, budget, clock):
        saved = random.getstate()
        random.setstate(self._rng)
        done = 0
        try:
            start = clock()
            for _ in range(ticks):
                self.world.step()
                done += 1
                self._after_tick()
                if self.ended is not None:
                    break
                if budget is not None and clock() - start > budget:
                    break
        finally:
            self._rng = random.getstate()
            random.setstate(saved)
        self.lagging = budget is not None and done < ticks and self.ended is None
        return done

    def _after_tick(self):
        w = self.world
        self.history.record(w)

        counts = (len(w.plants), len(w.vegetarians), len(w.predators))
        self.peaks = [max(p, c) for p, c in zip(self.peaks, counts)]
        _, n_veg, n_pred = counts

        if self._had_vegetarians and not n_veg:
            self._had_vegetarians = False
            self.events.append(Event(w.tick, f"Травоядные вымерли на тике {spaced(w.tick)}"))
        if self._had_predators and not n_pred:
            self._had_predators = False
            self.events.append(Event(w.tick, f"Хищники вымерли на тике {spaced(w.tick)}"))

        if w.migrants != self._migrants:
            self._migrants = w.migrants
            self._had_predators = True
            self.events.append(Event(w.tick, f"Пришёл хищник-мигрант на тике {spaced(w.tick)}"))

        if not n_veg and not n_pred:
            self.ended = "extinct"
        elif self.watch_explosion and n_veg + n_pred > EXPLOSION_LIMIT:
            self.ended = "explosion"

    def keep_going(self):
        """«Продолжить всё равно» после взрыва: больше не останавливаемся."""
        if self.ended == "explosion":
            self.ended = None
            self.watch_explosion = False

    def _measure(self, dt, done):
        self._tps_time  += dt
        self._tps_ticks += done
        if self._tps_time >= 0.5:
            self.tps = self._tps_ticks / self._tps_time
            self._tps_time, self._tps_ticks = 0.0, 0

    # ── прочее ──────────────────────────────────────────────────────────────
    def restart(self):
        """Та же партия с начала: тот же сид и те же настройки."""
        return Session(self.settings, seed=self.seed)

    def pick(self, wx, wy, radius):
        return pick_creature(self.world, wx, wy, radius)


def pick_creature(world, wx, wy, radius):
    """Ближайшее к точке (wx, wy) травоядное или хищник не дальше radius, иначе None.

    Координаты мировые. Мелкое существо под курсором находится, даже если
    промахнуться на radius; крупное — если кликнуть в любое место его тела.
    """
    best, best_d = None, None
    for creatures in (world.vegetarians, world.predators):
        for c in creatures:
            body = (c.size if hasattr(c, "size") else c.DIAM) / 2
            d = ((c.x - wx) ** 2 + (c.y - wy) ** 2) ** 0.5 - body
            if d <= radius and (best_d is None or d < best_d):
                best, best_d = c, d
    return best
