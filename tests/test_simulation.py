"""Безэкранные тесты симуляции.

Запуск:   python -m unittest discover tests

ГАРАНТИИ ЗАВЕРШАЕМОСТИ
----------------------
Ни один тест не может зациклиться или уехать по времени:

  * нет ни одного `while` — все циклы ограничены фиксированным числом тиков;
  * каждый прогон дополнительно ограничен дедлайном по часам и потолком
    популяции (см. run_ticks). Стоимость тика растёт вместе с популяцией,
    поэтому одного лимита тиков НЕДОСТАТОЧНО: популяция растёт — тик дорожает.
    Сетка соседей (life/grid.py) сделала этот рост почти линейным вместо
    квадратичного, но не отменила его — лимиты нужны по-прежнему;
  * если лимит достигнут, прогон останавливается, а проверки выполняются на
    том состоянии, до которого дошли. Тест не падает от медленной машины,
    но и не превращается в пустышку — инварианты всё равно проверены.

Весь набор укладывается в несколько секунд.
pygame-окно не требуется: движок живёт в пакете life/ и не трогает дисплей.
"""

import contextlib
import copy
import io
import math
import random
import subprocess
import sys
import time
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import sim_report
from life.config     import *
from life.genome     import Genom, PERCENT_GENES
from life.grid       import Grid
from life.headless   import simulate
from life.plant      import Plant
from life.vegetarian import Vegetarian
from life.predator   import Predator
from life.world      import World

PROJECT_ROOT = Path(__file__).resolve().parent.parent


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
# тестам, а 400 тиков с эволюцией — самая дорогая часть всего набора.
BASELINE_TICKS   = 400            # ~3.6 млн работы, ~0.3 с
BASELINE_CEILING = 3000           # ~6x от наблюдаемого пика популяции (~480)
BASELINE_WORK    = 25_000_000     # ~7x от расхода здорового прогона (3.6 млн)
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

        # 2000 тиков по 2.5 растения — это выше PLANT_MAX. Тест про скорость
        # спауна, а не про потолок, поэтому потолок здесь снят (у потолка свой тест).
        with mock.patch("life.world.PLANT_MAX", 10**9):
            before = len(world.plants)
            ticks  = self.bounded(world, 2000, seconds=10.0)
            rate   = (len(world.plants) - before) / ticks

        # разброс среднего на 2000 тиках ~0.007, допуск 0.1 — мигать не может
        self.assertAlmostEqual(
            rate, PLANT_SPAWN_CHANCE, delta=0.1,
            msg=f"прирост {rate:.3f} растений/тик вместо {PLANT_SPAWN_CHANCE}",
        )

    def test_plant_count_is_capped(self):
        """Без травоядных растения упираются в PLANT_MAX, а не растут вечно.

        Баг: растение исчезает только съеденным, и в мире без едоков их
        становилось на 2.5 больше каждый тик — 15 тысяч за 6000 тиков.
        К 600-му тику потолок достигнут; ещё 400 тиков проверяют, что он держится.
        """
        world = World(seed=7)
        world.vegetarians = []
        world.predators   = []

        self.bounded(world, 1000, seconds=10.0)

        self.assertEqual(len(world.plants), PLANT_MAX,
                         f"растений {len(world.plants)} при потолке {PLANT_MAX}")

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
        parent.energy = parent.max_energy        # сытый — готов делиться
        world.tick = 0                           # tick % 30 == 0 → ветка размножения

        # Деление хищника случайно (PREDATOR_DIVIDE_CHANCE), и ждать удачного
        # броска значит зависеть от сида. Здесь шанс равен единице: хищник
        # делится ровно на этом тике.
        with mock.patch("life.predator.PREDATOR_DIVIDE_CHANCE", 1.0):
            world.step()

        newborns = [p for p in world.predators if p is not parent]
        self.assertEqual(len(newborns), 1, "хищник не поделился — тест бессмыслен")
        child = newborns[0]
        self.assertEqual(
            child.energy, child.max_energy * PREDATOR_CHILD_ENERGY,
            "ребёнок потратил энергию, значит его обработали в тике рождения",
        )

    def test_starved_vegetarian_does_not_act(self):
        """Травоядное, умершее от голода на своём ходу, не ест и не размножается.

        Баг: move() ставил alive=False, но try_eat и maybe_divide вызывались
        следом без проверки. Труп съедал растения (160 раз за 5 прогонов по
        3000 тиков) и даже оставлял потомство.
        """
        random.seed(3)
        world = World(seed=3)
        world.predators = []

        veg = Vegetarian(x=1000, y=1000)
        veg.energy = veg.upkeep / 2              # этот ход — последний
        plants = []
        for dx in (0, 5):                        # двух растений хватило бы на деление
            p = Plant()
            p.x, p.y = veg.x + dx, veg.y
            plants.append(p)
        world.vegetarians = [veg]
        world.plants      = list(plants)

        world.tick = 0                           # tick % 30 == 0 → ветка размножения
        world.step()

        self.assertFalse(veg.alive, "не умер — тест бессмыслен")
        self.assertEqual(world.vegetarians, [], "умершее от голода травоядное оставило потомство")
        for p in plants:
            self.assertIn(p, world.plants, "умершее от голода травоядное съело растение")

    def test_starved_predator_does_not_hunt(self):
        """Хищник, умерший от голода на своём ходу, никого не съедает.

        Тот же баг у хищников: мёртвый хищник убивал добычу (3 раза за 5 прогонов).
        """
        random.seed(3)
        world = World(seed=3)
        world.plants = []

        prey   = Vegetarian(x=1000, y=1000)
        hunter = Predator(x=1010, y=1000)        # добыча в пасти
        hunter.energy = hunter.upkeep / 2        # этот ход — последний
        world.vegetarians = [prey]
        world.predators   = [hunter]

        world.step()

        self.assertFalse(hunter.alive, "не умер — тест бессмыслен")
        self.assertTrue(prey.alive, "умерший от голода хищник съел добычу")
        self.assertEqual(world.predators, [], "умерший хищник остался в мире")

    def test_predator_target_is_reachable(self):
        """Цель блуждания хищника лежит там, куда он может дойти.

        Баг: цель бралась до самой стены, а сам хищник держится в DIAM от края.
        Цель у стены была недостижима: хищник упирался в стену и стоял, пока мимо
        не пройдёт добыча. На сиде 1 один так простоял 426 тиков и умер от голода.
        """
        random.seed(0)
        d = Predator.DIAM
        spots = [(d, d), (WORLD_WIDTH - d, d), (d, WORLD_HEIGHT - d),
                 (WORLD_WIDTH - d, WORLD_HEIGHT - d), (WORLD_WIDTH / 2, WORLD_HEIGHT / 2)]
        for x, y in spots:
            pr = Predator(x=x, y=y)
            for _ in range(200):                     # фиксированное число целей
                pr._choose_new_target()
                self.assertTrue(d <= pr.tx <= WORLD_WIDTH - d,
                                f"цель недостижима: x={pr.tx:.1f}, хищник у ({x}, {y})")
                self.assertTrue(d <= pr.ty <= WORLD_HEIGHT - d,
                                f"цель недостижима: y={pr.ty:.1f}, хищник у ({x}, {y})")

        # и в движении: хищник из угла без добычи не застревает ни на тик
        pr = Predator(x=d, y=d)
        pr.energy = pr.upkeep * 10_000               # голод здесь ни при чём
        stood = 0
        for _ in range(2000):                        # фиксированное число ходов
            before = (pr.x, pr.y)
            pr.move([])
            stood += (pr.x, pr.y) == before
        self.assertEqual(stood, 0, f"хищник без добычи стоял на месте {stood} тиков")

    def test_predator_ignores_eaten_prey(self):
        """Хищник не гонится за травоядным, которого в этом тике уже съели.

        Баг: поиск добычи не смотрел на alive, и хищник шёл к трупу, пока тот
        не выметут в конце тика (1863 хода за 5 прогонов).
        """
        random.seed(0)
        hunter = Predator(x=1000, y=1000)
        corpse = Vegetarian(x=1100, y=1000)          # ближе, но уже съеден
        corpse.alive = False
        living = Vegetarian(x=1000, y=1300)

        hunter.move([corpse, living])

        self.assertEqual(hunter.x, 1000, "хищник свернул к трупу")
        self.assertGreater(hunter.y, 1000, "хищник не пошёл к живой добыче")

    def test_parent_keeps_reserve_after_division(self):
        """После деления у родителя остаётся не меньше VEGETARIAN_REPRO_RESERVE.

        Баг: резерв проверялся до вычета доли ребёнка. Родитель с большой долей
        отдавал всё до нуля и ниже (201 раз за 5 прогонов) и умирал на следующем
        ходу, а хищник, съевший такого, терял энергию.
        """
        random.seed(0)
        energy = 60                    # порог 30% от бака 100 + резерв 20 = 50 — делиться можно
        divided = blocked = 0
        for share in (10, 30, 50, 70, 90):
            with self.subTest(share=share):
                parent = Vegetarian(x=1000, y=1000, energy=energy,
                                    genom=[40, 10, 400, 30, share, 5, 100])
                kids = []
                parent.maybe_divide(kids)
                if kids:
                    divided += 1
                    self.assertGreaterEqual(
                        parent.energy, VEGETARIAN_REPRO_RESERVE,
                        f"доля {share}%: после деления у родителя {parent.energy:.1f}",
                    )
                else:
                    blocked += 1
                    self.assertEqual(parent.energy, energy, "деления не было, а энергия ушла")

        self.assertTrue(divided and blocked, "доли подобраны так, что одна из веток не проверена")

    def test_child_is_born_inside_its_layer(self):
        """Ребёнок рождается внутри своего слоя и не на диагонали от родителя.

        Баги: y ребёнка зажималась только в мир, а слой у ребёнка уже свой,
        мутировавший, — первый ход телепортировал его в слой (бывало на 2324 px).
        А смещение по x и по y было одним и тем же числом, поэтому все дети
        ложились на диагональ от родителя.
        """
        random.seed(0)
        # родитель у нижнего края слоя: у детей эта граница мутирует и вверх, и вниз
        parent = Vegetarian(x=3000, y=3900, genom=[40, 10, 400, 30, 30, 5, 100])
        diagonal = 0
        for _ in range(300):                         # фиксированное число делений
            parent.energy = parent.max_energy
            kids = []
            parent.maybe_divide(kids)
            self.assertEqual(len(kids), 1, "сытый родитель не поделился")
            child = kids[0]

            self.assertTrue(child.body_lo <= child.y <= child.body_hi,
                            f"ребёнок вне своего слоя: y={child.y:.1f}, "
                            f"слой [{child.body_lo:.1f}, {child.body_hi:.1f}]")
            self.assertTrue(child.x_lo <= child.x <= child.x_hi, f"ребёнок вне мира: x={child.x}")
            diagonal += (child.x - parent.x) == (child.y - parent.y)

            x, y = child.x, child.y
            child.move([], [])
            jump = math.hypot(child.x - x, child.y - y)
            self.assertLessEqual(jump, child.speed + 1e-9,
                                 f"первый ход ребёнка — прыжок на {jump:.0f} px")

        self.assertEqual(diagonal, 0, f"{diagonal} детей из 300 легли на диагональ от родителя")

    def test_fractional_size_without_coordinates(self):
        """Существо с дробным размером рождается без заданных координат.

        Баг: x выбиралась через randint(self.size, ...), а размер после мутации
        дробный. На Python 3.12+ randint дробных не берёт и падает с TypeError.
        """
        random.seed(0)
        veg = Vegetarian(genom=[40.5, 10, 400, 70, 30, 5, 100])
        self.assertTrue(veg.x_lo <= veg.x <= veg.x_hi, f"родился вне мира: x={veg.x}")
        self.assertTrue(veg.body_lo <= veg.y <= veg.body_hi, f"родился вне слоя: y={veg.y}")


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

    def test_creatures_get_every_neighbour_in_live_world(self):
        """Каждое существо получает от мира всех соседей в радиусе своего запроса.

        Проверяется связка целиком: размер клетки, который подбирает World,
        точка запроса и сама сетка. Вызовы сущностей перехватываются, и то, что
        им дали, сверяется с перебором в лоб. Прежняя версия теста считала
        формулу клетки сама и не замечала, если World считал клетку неверно:
        подмена клетки на 100 px при зрении 400 проходила все тесты.
        """
        world = copy.deepcopy(baseline().world)      # общий прогон не трогаем
        self.assertGreater(len(world.vegetarians), 0, "популяция вымерла — проверять нечего")
        self.assertGreater(len(world.predators),   0, "хищники вымерли — проверять нечего")
        missed = []

        def check(who, radius, given, pool):
            given = {id(o) for o in given}
            r2 = radius * radius
            for o in pool:
                if id(o) not in given and (o.x - who.x) ** 2 + (o.y - who.y) ** 2 <= r2:
                    missed.append(f"{type(who).__name__} (радиус {radius:.0f}) "
                                  f"не получил {type(o).__name__}")

        veg_move, veg_eat = Vegetarian.move, Vegetarian.try_eat
        pr_move,  pr_eat  = Predator.move,   Predator.try_eat

        def v_move(v, plants, predators):
            check(v, v.vision, plants,    world.plants)
            check(v, v.vision, predators, world.predators)
            return veg_move(v, plants, predators)

        def v_eat(v, plants):                        # позиция уже новая, после шага
            check(v, v.size, plants, world.plants)
            return veg_eat(v, plants)

        def p_move(pr, vegetarians):
            check(pr, pr.vision, vegetarians, world.vegetarians)
            return pr_move(pr, vegetarians)

        def p_eat(pr, vegetarians):
            check(pr, pr.DIAM, vegetarians, world.vegetarians)
            return pr_eat(pr, vegetarians)

        with mock.patch.object(Vegetarian, "move", v_move), \
             mock.patch.object(Vegetarian, "try_eat", v_eat), \
             mock.patch.object(Predator, "move", p_move), \
             mock.patch.object(Predator, "try_eat", p_eat):
            run_ticks(world, 3)

        self.assertEqual(missed[:3], [], f"соседи потеряны {len(missed)} раз")


# ─────────────────────────────────────────────────────────────────────────────
# Границы слоёв проекта
# ─────────────────────────────────────────────────────────────────────────────

class TestLayering(unittest.TestCase):
    """Движок не знает про экран — и это проверяется, а не только обещается."""

    def test_engine_does_not_import_pygame(self):
        """Импорт life/ не должен тянуть за собой pygame.

        Раньше у каждой сущности был свой draw(), поэтому headless-прогон
        импортировал pygame, хотя дисплея не касался. Отрисовка живёт в
        render.py, и эта граница должна оставаться на месте.

        Проверять приходится в подпроцессе: в самом наборе тестов pygame может
        уже оказаться в sys.modules, и проверка стала бы пустышкой.
        """
        out = subprocess.run(
            [sys.executable, "-c",
             "import life.headless, life.world, sys; print('pygame' in sys.modules)"],
            cwd=str(PROJECT_ROOT), capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(out.returncode, 0, "импорт life/ упал: " + out.stderr)
        self.assertEqual(
            out.stdout.strip(), "False",
            "life/ снова тянет pygame — отрисовка просочилась в движок",
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

            # вертикальный слой (гены min_y/max_y): тело внутри своей полосы,
            # а схлопнутая полоса — хотя бы целиком внутри мира
            lo = (v.min_y / 100) * WORLD_HEIGHT + v.size
            hi = (v.max_y / 100) * WORLD_HEIGHT - v.size
            if lo > hi:
                lo, hi = v.size, WORLD_HEIGHT - v.size
            self.assertTrue(
                lo <= v.y <= hi,
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
        Регрессии: ValueError: empty range in randrange(6040, 5961) при
        рождении; тело, торчащее за край мира у слоя на границе; и существо,
        которое со схлопнутым слоем стояло столбом — случайная цель никогда
        не попадала в перевёрнутую полосу.
        """
        random.seed(0)
        genom = [40, 10, 400, 70, 30, 50.0, 50.0]      # min_y == max_y

        veg = Vegetarian(genom=genom)                   # без x/y — раньше падало
        self.assertTrue(0 <= veg.y <= WORLD_HEIGHT, f"родился вне мира: y={veg.y}")

        for pct in (0.0, 100.0):                        # слой на самой границе мира
            edge = Vegetarian(genom=[40, 10, 400, 70, 30, pct, pct])
            self.assertTrue(edge.size <= edge.y <= WORLD_HEIGHT - edge.size,
                            f"тело торчит за край мира: y={edge.y}")

        veg = Vegetarian(x=1000, y=6000, genom=genom)
        line = veg.body_lo
        self.assertEqual(veg.y, line, "заданная y не прижата к схлопнутому слою")
        for _ in range(50):                             # фиксированные 50 тиков
            veg.move([], [])

        self.assertTrue(math.isfinite(veg.x) and math.isfinite(veg.y),
                        "координаты стали NaN при схлопнутом слое")
        self.assertTrue(0 <= veg.x <= WORLD_WIDTH,  f"вылетел за мир: x={veg.x}")
        self.assertEqual(veg.y, line, "сошёл со схлопнутого слоя")
        self.assertGreater(abs(veg.x - 1000), veg.speed,
                           "со схлопнутым слоем существо стоит на месте")

    def test_mutation_keeps_percent_genes_in_range(self):
        """Гены-проценты (порог, доля потомку, слой) при мутации остаются в 0‒100.

        Сторожит место, где раньше стояло `if i == 5 or i == 6`: гены
        выбираются по имени, и промах мимо них выпустил бы слой за пределы
        мира. Родитель стоит у самых краёв, чтобы мутации туда и тянули.
        """
        random.seed(0)
        parent = Vegetarian(genom=[40, 10, 400, 99.5, 99.5, 0.5, 99.5])

        for _ in range(500):                             # фиксированное число мутаций
            child = parent.mutate()
            self.assertIsInstance(child, Genom)
            for name in PERCENT_GENES:
                value = getattr(child, name)
                self.assertTrue(0 <= value <= 100, f"{name}={value}")


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
        становится неподъёмным. Теперь растения ограничены PLANT_MAX, но бюджет
        остаётся страховкой на случай, если потолок поднимут или сломают. Такой прогон однажды уже уткнулся в дедлайн
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


class TestReport(unittest.TestCase):
    """Отчёт sim_report.py — инструмент проверки баланса, и врать он не должен."""

    def test_summary_reports_cut_runs(self):
        """Сводка не пишет «всё в порядке», если прогон оборван раньше срока.

        Баг: на 3000 тиках seed 4 обрывался перегрузкой на 2604-м тике, а
        сводка всё равно рапортовала, что прогоны в разумном коридоре.
        """
        res = simulate(seed=1, ticks=BASELINE_TICKS, seconds=15.0,
                       max_total_work=1_000_000)       # заведомо ниже здорового расхода
        self.assertTrue(res.overloaded, "прогон не оборвался — тест бессмыслен")

        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            sim_report.print_summary([(1, res)])

        self.assertIn("ОБОРВАНЫ", out.getvalue())
        self.assertNotIn("в разумном коридоре", out.getvalue())


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
