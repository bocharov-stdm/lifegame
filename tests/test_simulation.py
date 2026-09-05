"""Безэкранные тесты симуляции.

Запуск:   python -m unittest discover tests

ГАРАНТИИ ЗАВЕРШАЕМОСТИ
----------------------
Ни один тест не может зациклиться или уехать по времени:

  * нет ни одного `while` — все циклы ограничены фиксированным числом тиков;
  * каждый прогон дополнительно ограничен дедлайном по часам и потолком
    популяции (см. run_ticks). Стоимость тика растёт вместе с популяцией,
    поэтому одного лимита тиков НЕДОСТАТОЧНО: популяция растёт — тик дорожает.
    Сетка соседей (grid.py) сделала этот рост почти линейным вместо
    квадратичного, но не отменила его — лимиты нужны по-прежнему;
  * если лимит достигнут, прогон останавливается, а проверки выполняются на
    том состоянии, до которого дошли. Тест не падает от медленной машины,
    но и не превращается в пустышку — инварианты всё равно проверены.

Весь набор укладывается примерно в 10 секунд.
pygame-окно не требуется: логика живёт в world.py и не трогает дисплей.
"""

import math
import random
import sys
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from config     import *
from grid       import Grid
from headless   import simulate
from plant      import Plant
from vegetarian import Vegetarian
from predator   import Predator
from world      import World


# ─────────────────────────────────────────────────────────────────────────────
# Ограничитель прогона: тики + часы + потолок популяции
# ─────────────────────────────────────────────────────────────────────────────

def run_ticks(world, ticks, seconds=10.0, max_creatures=3000):
    """Крутит world не дольше ticks, seconds и max_creatures существ.

    Возвращает (сделано_тиков, причина_остановки).
    """
    deadline = time.perf_counter() + seconds

    for done in range(1, ticks + 1):
        world.step()

        if len(world.vegetarians) + len(world.predators) > max_creatures:
            return done, f"популяция > {max_creatures}"
        if time.perf_counter() > deadline:
            return done, f"дедлайн {seconds} с"

    return ticks, "готово"


class BoundedRunMixin:
    def bounded(self, world, ticks, **kw):
        done, why = run_ticks(world, ticks, **kw)
        if done < ticks:
            print(f"\n  [лимит] остановлен на {done}/{ticks} тиков: {why} "
                  f"— проверки идут по достигнутому состоянию")
        return done


# Базовый прогон считается один раз на весь набор: он нужен сразу нескольким
# тестам, а 600 тиков с эволюцией — самая дорогая часть всего набора.
BASELINE_TICKS   = 400            # ~3.8 млн работы, ~0.7 с
BASELINE_CEILING = 3000           # ~6x от наблюдаемого пика популяции (~480)
BASELINE_WORK    = 25_000_000     # ~6x от расхода здорового прогона (3.8 млн)
_baseline_cache  = None


def baseline():
    global _baseline_cache
    if _baseline_cache is None:
        _baseline_cache = simulate(seed=1, ticks=BASELINE_TICKS, sample_every=100,
                                   seconds=15.0, max_creatures=BASELINE_CEILING,
                                   max_total_work=BASELINE_WORK)
    return _baseline_cache


# ─────────────────────────────────────────────────────────────────────────────
# Регрессии на найденные и исправленные баги
# ─────────────────────────────────────────────────────────────────────────────

class TestRegressions(BoundedRunMixin, unittest.TestCase):
    """Каждый тест закрывает конкретный баг, который уже был в коде."""

    def test_plant_spawn_rate(self):
        """PLANT_SPAWN_CHANCE — ожидаемое число растений за тик, а не вероятность.

        Баг: `if random.random() < 2.88` всегда истинно → ровно 1 растение
        за тик вместо 2.88, то есть втрое меньше еды, чем задумано.
        """
        world = World(seed=7)
        world.vegetarians = []          # некому есть — считаем чистый прирост
        world.predators   = []

        before = len(world.plants)
        ticks  = self.bounded(world, 2000, seconds=10.0)
        rate   = (len(world.plants) - before) / ticks

        # разброс среднего на 2000 тиках ~0.007, допуск 0.1 — мигать не может
        self.assertAlmostEqual(
            rate, PLANT_SPAWN_CHANCE, delta=0.1,
            msg=f"прирост {rate:.3f} растений/тик вместо {PLANT_SPAWN_CHANCE}",
        )

    def test_eaten_vegetarian_does_not_act(self):
        """Съеденный хищником в этом же тике не должен есть и размножаться.

        Баг: хищники обрабатываются первыми и ставят alive=False, но цикл
        травоядных не проверял alive — труп успевал съесть растение и родить.
        """
        world = World(seed=3)
        world.predators = []

        victim = world.vegetarians[0]
        victim.alive  = False                    # «съеден» на этом тике
        victim.energy = victim.max_energy        # энергии хватило бы на деление
        world.vegetarians = [victim]

        plant = Plant()
        plant.x, plant.y = victim.x, victim.y    # прямо под ним — съел бы гарантированно
        world.plants = [plant]

        world.tick = 0                           # tick % 30 == 0 → ветка размножения
        world.step()                             # ровно один тик

        self.assertIn(plant, world.plants, "мёртвое травоядное съело растение")
        self.assertEqual(world.vegetarians, [], "мёртвое травоядное оставило потомство")

    def test_flee_continues_after_predator_lost(self):
        """Бегство продолжается, когда хищник пропал из зоны видимости.

        Баг: при nearest_pred=None вектор бегства обнулялся, цель = текущая
        позиция, и существо стояло столбом все FLEE_TICKS, сжигая энергию.
        """
        random.seed(11)
        veg  = Vegetarian(x=1000, y=1000)
        pred = Predator(x=1000 + veg.vision / 4, y=1000)

        veg.move([], [pred])                     # испугался
        self.assertGreater(veg.flee_ticks, 0, "испуг не сработал — тест бессмыслен")

        pred.x = WORLD_WIDTH - 1                 # хищник исчез из виду
        start_x, start_y = veg.x, veg.y
        for _ in range(10):                      # фиксированные 10 тиков
            veg.move([], [pred])

        travelled = math.hypot(veg.x - start_x, veg.y - start_y)
        self.assertGreater(
            travelled, 9 * veg.speed,
            f"убежал всего на {travelled:.1f} px за 10 тиков вместо ~{10 * veg.speed}",
        )
        self.assertLess(veg.x, start_x, "убегает не в ту сторону от хищника")

    def test_predator_offspring_not_processed_same_tick(self):
        """Новорождённый хищник не должен ходить в тике своего рождения.

        Баг: maybe_divide дописывал ребёнка в тот же список, по которому шёл
        цикл, — ребёнок попадал в текущий тик. Проверяем по энергии: движение
        её тратит, значит нетронутая энергия = ребёнок не обрабатывался.
        """
        world = World(seed=5)
        world.vegetarians = []                   # пустой мир — тик почти бесплатный
        world.plants      = []
        parent = world.predators[0]
        world.predators = [parent]

        child, deadline = None, time.perf_counter() + 10.0
        for _ in range(300):                     # жёсткий потолок по тикам
            parent.energy = parent.max_energy    # держим сытым, чтобы делился
            world.step()
            newborns = [p for p in world.predators if p is not parent]
            if newborns:
                child = newborns[0]
                break
            if time.perf_counter() > deadline:   # и по часам
                break

        self.assertIsNotNone(child, "хищник так и не поделился — тест бессмыслен")
        self.assertEqual(
            child.energy, child.max_energy * 0.25,
            "ребёнок потратил энергию, значит его обработали в тике рождения",
        )


# ─────────────────────────────────────────────────────────────────────────────
# Сетка соседей
# ─────────────────────────────────────────────────────────────────────────────

class TestGrid(unittest.TestCase):
    """Сетка обязана быть НАДмножеством честного перебора.

    Это главный риск оптимизации и единственная её часть, которую не поймает
    ни один тест на численность: пропусти сетка соседа — существо перестанет
    замечать еду под носом, симуляция останется правдоподобной, а поведение
    тихо изменится. Поэтому сравниваем с перебором в лоб напрямую.
    """

    def test_grid_matches_brute_force(self):
        random.seed(0)

        class Point:
            def __init__(self, x, y):
                self.x, self.y = x, y

        for _ in range(50):                       # фиксированное число прогонов
            points = [Point(random.uniform(0, WORLD_WIDTH),
                            random.uniform(0, WORLD_HEIGHT))
                      for _ in range(random.randint(0, 200))]
            cell = random.uniform(60, 900)
            grid = Grid(cell, points)

            for _ in range(10):                   # фиксированное число запросов
                qx = random.uniform(0, WORLD_WIDTH)
                qy = random.uniform(0, WORLD_HEIGHT)

                # cell — максимальный радиус, на котором сетка обязана быть полной
                expected = {id(p) for p in points
                            if math.hypot(p.x - qx, p.y - qy) <= cell}
                got      = {id(p) for p in grid.near(qx, qy)}

                missed = expected - got
                self.assertFalse(
                    missed,
                    f"сетка потеряла {len(missed)} соседей: клетка {cell:.0f}, "
                    f"{len(points)} точек, запрос ({qx:.0f}, {qy:.0f})",
                )

    def test_grid_covers_vision_in_live_world(self):
        """То же самое, но на живом мире: важен реальный размер клетки.

        Сетку строит World, подбирая клетку под самый большой радиус запроса.
        Здесь проверяется именно эта связка — что подобранная клетка накрывает
        зрение каждого травоядного, каким бы оно ни выросло.
        """
        world = baseline().world
        self.assertGreater(len(world.vegetarians), 0, "популяция вымерла — проверять нечего")

        cell = max(GRID_MIN_CELL,
                   max(max(v.vision, v.size) for v in world.vegetarians))
        grid = Grid(cell, world.plants)

        for v in world.vegetarians:
            expected = {id(p) for p in world.plants
                        if math.hypot(p.x - v.x, p.y - v.y) <= v.vision}
            got      = {id(p) for p in grid.near(v.x, v.y)}
            self.assertFalse(
                expected - got,
                f"травоядное не увидело еду в радиусе зрения: зрение {v.vision:.0f}, "
                f"клетка {cell:.0f}",
            )


# ─────────────────────────────────────────────────────────────────────────────
# Инварианты: то, что должно быть верно всегда, при любой эволюции
# ─────────────────────────────────────────────────────────────────────────────

class TestInvariants(BoundedRunMixin, unittest.TestCase):

    def assert_world_sane(self, world):
        for p in world.plants:
            self.assertTrue(math.isfinite(p.x) and math.isfinite(p.y), "NaN у растения")
            self.assertTrue(0 <= p.x <= WORLD_WIDTH,  f"растение вне мира: x={p.x}")
            self.assertTrue(0 <= p.y <= WORLD_HEIGHT, f"растение вне мира: y={p.y}")

        for v in world.vegetarians:
            self.assertTrue(math.isfinite(v.x) and math.isfinite(v.y), "NaN у травоядного")
            self.assertTrue(math.isfinite(v.energy), "NaN в энергии травоядного")
            self.assertGreater(v.energy, 0, "живое травоядное с нулевой энергией")
            self.assertTrue(0 <= v.x <= WORLD_WIDTH,  f"травоядное вне мира: x={v.x}")
            self.assertTrue(0 <= v.y <= WORLD_HEIGHT, f"травоядное вне мира: y={v.y}")

            # вертикальный слой (гены min_y/max_y) — только если полоса не схлопнулась
            lo = (v.min_y / 100) * WORLD_HEIGHT + v.size
            hi = (v.max_y / 100) * WORLD_HEIGHT - v.size
            if lo <= hi:
                self.assertTrue(
                    lo - 1 <= v.y <= hi + 1,
                    f"травоядное вышло из своего слоя: y={v.y:.1f}, слой [{lo:.1f}, {hi:.1f}]",
                )

        for pr in world.predators:
            self.assertTrue(math.isfinite(pr.x) and math.isfinite(pr.y), "NaN у хищника")
            self.assertGreater(pr.energy, 0, "живой хищник с нулевой энергией")
            self.assertTrue(0 <= pr.x <= WORLD_WIDTH,  f"хищник вне мира: x={pr.x}")
            self.assertTrue(0 <= pr.y <= WORLD_HEIGHT, f"хищник вне мира: y={pr.y}")

    def test_invariants_hold_over_time(self):
        """Прогон с эволюцией: никто не вылетает за границы мира и за свой слой."""
        world = baseline().world          # общий прогон, см. baseline()

        self.assertGreater(len(world.vegetarians), 0, "популяция вымерла — проверять нечего")
        self.assert_world_sane(world)

    def test_collapsed_layer_is_survivable(self):
        """Схлопнувшийся слой (min_y == max_y) не должен ломать существо.

        Заменяет собой долгий случайный прогон «а вдруг выпадет»: тот же
        краевой случай задаётся геномом напрямую и проверяется мгновенно.
        Регрессия на ValueError: empty range in randrange(6040, 5961).
        """
        random.seed(0)
        genom = [40, 10, 400, 70, 30, 50.0, 50.0]      # min_y == max_y

        veg = Vegetarian(genom=genom)                   # без x/y — раньше падало
        self.assertTrue(0 <= veg.y <= WORLD_HEIGHT, f"родился вне мира: y={veg.y}")

        for pct in (0.0, 100.0):                        # слой на самой границе мира
            edge = Vegetarian(genom=[40, 10, 400, 70, 30, pct, pct])
            self.assertTrue(0 <= edge.y <= WORLD_HEIGHT, f"родился вне мира: y={edge.y}")

        veg = Vegetarian(x=1000, y=6000, genom=genom)
        for _ in range(50):                             # фиксированные 50 тиков
            veg.move([], [])

        self.assertTrue(math.isfinite(veg.x) and math.isfinite(veg.y),
                        "координаты стали NaN при схлопнутом слое")
        self.assertTrue(0 <= veg.x <= WORLD_WIDTH,  f"вылетел за мир: x={veg.x}")
        self.assertTrue(0 <= veg.y <= WORLD_HEIGHT, f"вылетел за мир: y={veg.y}")


# ─────────────────────────────────────────────────────────────────────────────
# Поведение популяции: симуляция должна жить в разумном коридоре
# ─────────────────────────────────────────────────────────────────────────────

class TestPopulationDynamics(unittest.TestCase):
    """Ловит две противоположные поломки баланса: взрыв и вымирание.

    Пороги нарочно широкие — это сторож против патологии, а не фиксация
    текущих чисел. Подкрутка баланса не должна ронять эти тесты; уронить
    их должен только сломанный баланс.
    """

    def test_population_does_not_explode(self):
        """Взрыв численности фиксируется и НЕ вешает тест.

        Потолок популяции здесь работает и как сторож времени: тик дорожает
        именно от числа существ, поэтому упор в потолок обрывает прогон
        раньше, чем он успевает стать медленным.
        """
        res = baseline()

        trail = " -> ".join(f"t{h['tick']}:{h['vegetarians']}" for h in res.history)
        self.assertFalse(
            res.exploded,
            f"взрыв численности: превышен потолок {BASELINE_CEILING} на тике "
            f"{res.ticks_done}. Траектория: {trail}",
        )
        self.assertEqual(res.stop_reason, "готово",
                         f"прогон оборвался раньше срока: {res.stop_reason}")

    def test_population_does_not_die_out(self):
        """Обратная поломка: экосистема не должна схлопываться в ноль."""
        res = baseline()

        self.assertFalse(res.extinct, f"всё вымерло на тике {res.ticks_done}")
        self.assertGreater(res.final["vegetarians"], 0, "травоядные вымерли")
        self.assertGreater(res.final["predators"],   0, "хищники вымерли")

    def test_explosion_is_actually_detected(self):
        """Сторож взрыва действительно срабатывает, а не просто всегда зелёный.

        Ставим потолок заведомо ниже стартовой популяции — детектор обязан
        сработать немедленно. Без этого предыдущий тест мог бы «проходить»
        просто потому, что взрыв не детектируется вообще.
        """
        res = simulate(seed=1, ticks=BASELINE_TICKS, seconds=15.0, max_creatures=5)

        self.assertTrue(res.exploded, "детектор взрыва не сработал на потолке 5")
        self.assertLess(res.ticks_done, 50, "детектор сработал слишком поздно")
        self.assertLess(res.elapsed, 5.0, "детектор не спас от долгого прогона")

    def test_work_budget_is_actually_enforced(self):
        """Бюджет вычислений обрывает прогон — это и есть гарантия завершения.

        Проверяем именно его, потому что взрыв РАСТЕНИЙ (а не существ)
        счётчиком популяции не ловится: травоядных мало, а тик всё равно
        становится неподъёмным. Такой прогон однажды уже уткнулся в дедлайн
        на 25 секундах вместо того, чтобы оборваться сразу.
        """
        res = simulate(seed=1, ticks=BASELINE_TICKS, seconds=15.0,
                       max_total_work=1_000_000)     # заведомо ниже здорового расхода

        self.assertTrue(res.overloaded, f"бюджет работы не сработал: {res.stop_reason}")
        self.assertLess(res.elapsed, 5.0, "бюджет не спас от долгого прогона")

    def test_healthy_run_fits_in_budgets(self):
        """У здорового прогона должен оставаться запас по всем лимитам.

        Если запас исчез, пороги пора пересматривать — иначе тесты начнут
        мигать на ровном месте.
        """
        res = baseline()

        self.assertEqual(res.stop_reason, "готово")
        print(f"\n  [бюджет] работа {res.total_work:,} из {BASELINE_WORK:,} "
              f"({res.total_work / BASELINE_WORK:.0%}), {res.elapsed:.1f} с")
        self.assertLess(
            res.total_work, BASELINE_WORK * 0.5,
            f"здоровый прогон съел {res.total_work:,} из {BASELINE_WORK:,} — запаса почти нет",
        )


# ─────────────────────────────────────────────────────────────────────────────
# Сторож производительности
# ─────────────────────────────────────────────────────────────────────────────

class TestPerformance(unittest.TestCase):

    # Бюджет кадра при 60 FPS — 16.7 мс, фактически тик стоит около 3 мс.
    # Порог опущен со 150 после перехода на сетку соседей: прежний перестал
    # что-либо сторожить. Запас всё ещё десятикратный — мигать не будет.
    MAX_MS_PER_TICK = 50.0

    def test_tick_budget_at_fixed_load(self):
        """Фиксированная нагрузка: 400 травоядных, 400 растений, 10 хищников.

        Нагрузка задана явно, а не выращена симуляцией, — поэтому время
        прогона предсказуемо и не зависит от того, куда ушла эволюция.
        """
        random.seed(99)
        world = World(seed=99)
        world.plants      = [Plant()      for _ in range(400)]
        world.vegetarians = [Vegetarian() for _ in range(400)]
        world.predators   = [Predator()   for _ in range(10)]

        ticks = 20                                      # фиксированно и заведомо коротко
        started = time.perf_counter()
        for _ in range(ticks):
            world.step()
        ms = (time.perf_counter() - started) / ticks * 1000

        print(f"\n  [perf] {ms:.1f} мс/тик при 400 травоядных "
              f"(бюджет кадра 60 FPS — 16.7 мс, порог теста {self.MAX_MS_PER_TICK})")
        self.assertLess(
            ms, self.MAX_MS_PER_TICK,
            f"{ms:.1f} мс/тик — производительность обвалилась",
        )


if __name__ == "__main__":
    unittest.main()
