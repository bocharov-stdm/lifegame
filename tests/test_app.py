"""Тесты приложения: партия, камера, настройки, отрисовка и экраны.

Запуск:   python -m unittest discover tests

Логика приложения (app/settings, session, history, camera) проверяется без
pygame. Экраны рисуются в память: App(headless=True) и SDL-драйвер dummy,
окно не открывается. Если pygame не установлен, эти тесты пропускаются.
Это единственный файл тестов, которому можно импортировать pygame.

Все прогоны ограничены: фиксированное число кадров и тиков, ни одного while.
"""

import json
import math
import os
import random
import subprocess
import sys
import tempfile
import unittest
from itertools import combinations
from pathlib import Path
from unittest import mock

os.environ.setdefault("SDL_VIDEODRIVER", "dummy")
os.environ.setdefault("PYGAME_HIDE_SUPPORT_PROMPT", "1")

PROJECT_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

from life.config     import *
from life.headless   import simulate
from life.plant      import Plant
from life.predator   import Predator
from life.rules      import DEFAULT_RULES, Rules
from life.vegetarian import Vegetarian
from life.world      import World

from app import settings as settings_io
from app.camera   import Camera
from app.history  import SMOOTH_TICKS, History, Sample, sample_of
from app.session  import Session, pick_creature
from app.settings import FIELDS, RULE_KEYS, Settings

try:
    import pygame
except ImportError:
    pygame = None

if pygame is not None:
    from app import render, theme
    from app.app import App
    from app.scenes.game  import GameScene
    from app.scenes.help  import HelpScene
    from app.scenes.menu  import MenuScene
    from app.scenes.prefs import PrefsScene
    from app.scenes.setup import SetupScene

UNLIMITED = float("inf")        # бюджет кадра без ограничения — число тиков предсказуемо


def run_session(session, frames, dt=1 / 60):
    """Фиксированное число кадров партии; тиков за кадр — ровно session.speed."""
    done = 0
    for _ in range(frames):
        done += session.advance(dt, budget=UNLIMITED)
    return done


# ─────────────────────────────────────────────────────────────────────────────
# Настройки
# ─────────────────────────────────────────────────────────────────────────────

class TestSettings(unittest.TestCase):

    def tmp_path(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        return Path(tmp.name)

    def test_defaults_match_config(self):
        """Настройки по умолчанию — ровно прежний мир: те же правила и числа."""
        s = Settings()
        self.assertEqual(s.rules(), DEFAULT_RULES)
        self.assertEqual(s.rules().upkeep(40, 10, 400), DEFAULT_RULES.upkeep(40, 10, 400))
        self.assertEqual((s.n_vegetarians, s.n_predators, s.predator_speed, s.predator_vision),
                         (VEGETARIANS_AT_START, PREDATORS_AT_START,
                          PREDATOR_BASE_SPEED, PREDATOR_BASE_VISION))

        a, b = s.make_world(seed=11), World(seed=11)
        self.assertEqual([(v.x, v.y) for v in a.vegetarians],
                         [(v.x, v.y) for v in b.vegetarians])
        self.assertEqual([(p.x, p.y) for p in a.predators],
                         [(p.x, p.y) for p in b.predators])

    def test_fields_are_consistent(self):
        """Каждый слайдер — поле настроек, значение по умолчанию — в пределах и на сетке."""
        default = Settings()
        for f in FIELDS:
            with self.subTest(f.key):
                self.assertTrue(hasattr(default, f.key))
                value = getattr(default, f.key)
                self.assertLessEqual(f.lo, value)
                self.assertLessEqual(value, f.hi)
                self.assertEqual(f.snap(value), value,
                                 "значение по умолчанию не на сетке шага — слайдер его собьёт")
                self.assertTrue(f.hint and f.label)
        self.assertTrue(set(RULE_KEYS) <= set(Rules.keys()))

    def test_load_cleans_garbage(self):
        path = self.tmp_path() / "s.json"
        path.write_text(json.dumps({
            "n_vegetarians": 1e9,          # за пределом
            "plant_growth": -5,            # ниже предела
            "mutation_sigma": 0.3333,      # мимо сетки
            "cost_scale": float("nan"),
            "seed": "abc",
            "smooth": 0,                   # не bool
            "fullscreen": True,
            "ui_scale": 1.3,
            "unknown": 5,
            "n_predators": True,           # bool — не число
        }), encoding="utf-8")

        s = settings_io.load(path)
        self.assertEqual(s.n_vegetarians, 200)
        self.assertEqual(s.plant_growth, 0.2)
        self.assertEqual(s.mutation_sigma, 0.35)
        self.assertEqual(s.cost_scale, 1.0)
        self.assertEqual(s.seed, Settings().seed)
        self.assertIs(s.smooth, True)
        self.assertIs(s.fullscreen, True)
        self.assertEqual(s.ui_scale, 1.25)
        self.assertEqual(s.n_predators, PREDATORS_AT_START)
        self.assertFalse(hasattr(s, "unknown"))

    def test_broken_file_gives_defaults(self):
        folder = self.tmp_path()
        for name, content in (("bad.json", "{не json"), ("list.json", "[1, 2]")):
            path = folder / name
            path.write_text(content, encoding="utf-8")
            self.assertEqual(settings_io.load(path), Settings(), name)
        self.assertEqual(settings_io.load(folder / "нет.json"), Settings())

    def test_save_and_load_roundtrip(self):
        folder = self.tmp_path()
        path = folder / "s.json"
        s = Settings(seed=777, random_seed=False, n_vegetarians=55, plant_energy=80,
                     size_power=1.5, ui_scale=1.5, smooth=False)
        self.assertTrue(settings_io.save(s, path))
        self.assertEqual(settings_io.load(path), s)
        self.assertEqual([p.name for p in folder.iterdir()], ["s.json"],
                         "после записи остались временные файлы")

    def test_save_failure_is_not_fatal(self):
        path = self.tmp_path() / "нет такой папки" / "s.json"
        self.assertFalse(settings_io.save(Settings(), path))

    def test_reset_touches_one_tab(self):
        s = Settings(n_vegetarians=99, plant_energy=99, random_seed=False)
        s.reset("lab")
        self.assertEqual(s.plant_energy, Settings().plant_energy)
        self.assertEqual(s.n_vegetarians, 99)
        s.reset("world")
        self.assertEqual(s.n_vegetarians, Settings().n_vegetarians)
        self.assertTrue(s.random_seed)


# ─────────────────────────────────────────────────────────────────────────────
# История для графиков
# ─────────────────────────────────────────────────────────────────────────────

class StubWorld:
    """Мир-заглушка: тик и списки нужной длины, без симуляции."""

    def __init__(self):
        self.tick = 0
        self.plants, self.vegetarians, self.predators = [], [], []

    def stats(self):
        return {"tick": self.tick, "avg_genom": [40.0] * 7 if self.vegetarians else None}


class TestHistory(unittest.TestCase):

    def test_whole_game_is_bounded_and_keeps_ends(self):
        h = History(full=10)
        for i in range(1000):                            # фиксированное число точек
            h.add(Sample(i * 10, i, i, i, None))

        self.assertLessEqual(len(h.full), 10)
        series = h.series(whole=True)
        self.assertEqual(series[0].tick, 0, "первая точка потерялась при прореживании")
        self.assertEqual(series[-1].tick, 9990, "последняя точка не попала в график")
        steps = {b.tick - a.tick for a, b in zip(h.full, h.full[1:])}
        self.assertEqual(steps, {h.stride * 10}, "прореженные точки легли неровно")

    def test_recent_window(self):
        h = History(recent=5)
        for i in range(20):
            h.add(Sample(i, 0, 0, 0, None))
        self.assertEqual([p.tick for p in h.series(whole=False)], [15, 16, 17, 18, 19])

    def test_counts_are_smoothed(self):
        """Пила от деления разом (0, 30, 0, 30…) превращается в ровное среднее."""
        world = StubWorld()
        h = History(every=10)
        for tick in range(1, 3 * SMOOTH_TICKS + 1):     # фиксированное число тиков
            world.tick = tick
            world.vegetarians = [None] * (30 if tick % 2 else 0)
            h.record(world)
        last = h.last
        self.assertEqual(last.tick, 3 * SMOOTH_TICKS)
        self.assertAlmostEqual(last.vegetarians, 15, delta=1)

    def test_origin_survives_the_window(self):
        """Исходный геном хранится отдельно: окно последних точек его уже не содержит."""
        h = History(recent=3, full=4)
        h.add(Sample(0, 0, 0, 0, None))                   # травоядных ещё нет
        h.add(Sample(10, 0, 0, 0, (40.0,) * 7))
        for i in range(2, 50):
            h.add(Sample(i * 10, 0, 0, 0, (80.0,) * 7))
        self.assertEqual(h.origin, (40.0,) * 7)
        self.assertNotIn(h.origin, [p.genom for p in h.series(whole=False)])

    def test_extinct_genome_is_none(self):
        self.assertIsNone(sample_of(StubWorld()).genom)


# ─────────────────────────────────────────────────────────────────────────────
# Камера
# ─────────────────────────────────────────────────────────────────────────────

class Walker:
    """Цель для слежения: бежит по миру быстрее, чем камера успела бы плавно."""

    def __init__(self, x, y):
        self.x, self.y, self.alive = x, y, True


class TestCamera(unittest.TestCase):

    def setUp(self):
        self.cam = Camera(WORLD_WIDTH, WORLD_HEIGHT, (100, 50, 800, 600))

    def test_roundtrip(self):
        self.cam.zoom_at(500, 350, 3.0)
        for wx, wy in ((0, 0), (1234.5, 987.6), (WORLD_WIDTH, WORLD_HEIGHT)):
            sx, sy = self.cam.to_screen(wx, wy)
            x, y = self.cam.to_world(sx, sy)
            self.assertAlmostEqual(x, wx, places=6)
            self.assertAlmostEqual(y, wy, places=6)

    def test_fit_shows_whole_world(self):
        x0, y0, x1, y1 = self.cam.visible_world()
        self.assertTrue(x0 <= 0 and y0 <= 0 and x1 >= WORLD_WIDTH and y1 >= WORLD_HEIGHT)
        self.assertTrue(self.cam.is_fit)

    def test_zoom_keeps_point_under_cursor(self):
        cursor = (550, 350)
        point = self.cam.to_world(*cursor)
        self.cam.zoom_at(*cursor, 2.0)
        sx, sy = self.cam.to_screen(*point)
        self.assertAlmostEqual(sx, cursor[0], places=6)
        self.assertAlmostEqual(sy, cursor[1], places=6)

    def test_zoom_limits(self):
        self.cam.zoom_at(500, 350, 1e6)
        self.assertEqual(self.cam.zoom, self.cam.max_zoom)
        self.cam.zoom_at(500, 350, 1e-6)
        self.assertEqual(self.cam.zoom, self.cam.min_zoom)

    def test_pan_stays_inside_world(self):
        self.cam.zoom_at(500, 350, 4.0)
        for dx, dy in ((10**6, 10**6), (-10**6, -10**6)):
            self.cam.pan(dx, dy)
            x0, y0, x1, y1 = self.cam.visible_world()
            self.assertGreaterEqual(x0, -1e-6)
            self.assertGreaterEqual(y0, -1e-6)
            self.assertLessEqual(x1, WORLD_WIDTH + 1e-6)
            self.assertLessEqual(y1, WORLD_HEIGHT + 1e-6)

    def test_resize_keeps_fit(self):
        self.cam.set_view((0, 0, 1600, 900))
        self.assertTrue(self.cam.is_fit)
        self.cam.zoom_at(800, 450, 2.0)
        zoom = self.cam.zoom
        self.cam.set_view((0, 0, 1500, 900))
        self.assertEqual(self.cam.zoom, zoom, "приближённый вид сбросился при смене размера")

    def test_follow_keeps_fast_target_on_screen(self):
        """На x32 существо уходит за кадр на тысячу единиц — камера не должна отставать."""
        target = Walker(1000, 2000)
        self.cam.zoom_at(500, 350, 5.0)
        self.cam.follow(target)
        self.cam.update(10.0)                             # камера подошла к цели
        for _ in range(4):                                # туда и обратно
            target.x += 1000
            self.cam.update(1 / 60)
            self.assertTrue(self.cam.contains(*self.cam.to_screen(target.x, target.y)),
                            "цель ушла с экрана")
        for _ in range(4):
            target.x -= 1000
            self.cam.update(1 / 60)
            self.assertTrue(self.cam.contains(*self.cam.to_screen(target.x, target.y)))

        target.alive = False
        self.cam.update(1 / 60)
        self.assertIsNone(self.cam.target, "камера следит за мёртвым")


# ─────────────────────────────────────────────────────────────────────────────
# Партия
# ─────────────────────────────────────────────────────────────────────────────

class TestSession(unittest.TestCase):

    def settings(self, **kw):
        base = dict(seed=7, random_seed=False, n_vegetarians=30, n_predators=4,
                    predator_speed=14, plant_energy=60, mutation_sigma=0.25)
        base.update(kw)
        return Settings(**base)

    def test_same_as_headless(self):
        """Партия в окне — тот же мир, что и headless-прогон с теми же настройками.

        Между кадрами генератор дёргают посторонние (как фоновый мир меню),
        партия то стоит на паузе, то меняет скорость — на результат это
        влиять не должно.
        """
        s = self.settings()
        session = Session(s)
        noise = random.Random(0)
        for frame in range(60):                           # фиксированное число кадров
            random.random()                               # чужой вызов между кадрами
            session.set_speed_index(noise.randrange(4))
            if frame % 7 == 3:
                session.toggle_pause()
                session.step_once()
            session.advance(1 / 60, budget=UNLIMITED)

        ticks = session.world.tick
        self.assertGreater(ticks, 100, "партия почти не шла — тест бессмыслен")
        res = simulate(seed=7, ticks=ticks, sample_every=10, seconds=15.0,
                       n_vegetarians=30, n_predators=4, predator_speed=14,
                       rules=s.rules())
        self.assertEqual(res.world.tick, ticks, f"прогон оборвался: {res.stop_reason}")

        w, h = session.world, res.world
        self.assertEqual([(v.x, v.y, v.energy) for v in w.vegetarians],
                         [(v.x, v.y, v.energy) for v in h.vegetarians])
        self.assertEqual([(p.x, p.y) for p in w.predators], [(p.x, p.y) for p in h.predators])

        expected = {st["tick"]: st["avg_genom"] for st in res.history}
        for point in session.history.full:
            if point.tick in expected and expected[point.tick] is not None:
                self.assertEqual(list(point.genom), expected[point.tick],
                                 f"средний геном на тике {point.tick} разошёлся")

    def test_restart_repeats_the_game(self):
        first = Session(self.settings(seed=21))
        run_session(first, 50)
        again = first.restart()
        run_session(again, 50)
        self.assertEqual(again.seed, 21)
        self.assertEqual([(v.x, v.y) for v in first.world.vegetarians],
                         [(v.x, v.y) for v in again.world.vegetarians])

    def test_session_leaves_global_rng_alone(self):
        random.seed(123)
        expected = [random.random() for _ in range(3)]
        random.seed(123)
        session = Session(self.settings())
        run_session(session, 5)
        self.assertEqual([random.random() for _ in range(3)], expected)

    def test_extinction_events_and_end(self):
        session = Session(self.settings())
        session.world.predators = []
        run_session(session, 1)
        self.assertEqual(len(session.events), 1)
        self.assertIn("Хищники вымерли", session.events[0].text)
        self.assertIsNone(session.ended, "травоядные живы — партия не окончена")

        session.world.vegetarians = []
        run_session(session, 1)
        self.assertEqual(session.ended, "extinct")
        tick = session.world.tick
        self.assertEqual(run_session(session, 3), 0, "после конца партия идёт дальше")
        self.assertEqual(session.world.tick, tick)
        self.assertEqual(len(session.events), 2)

    def test_explosion_stops_and_can_continue(self):
        with mock.patch("app.session.EXPLOSION_LIMIT", 10):
            session = Session(self.settings(n_vegetarians=20))
            session.set_speed_index(3)
            self.assertEqual(run_session(session, 1), 1, "взрыв должен остановить сразу")
            self.assertEqual(session.ended, "explosion")
            self.assertEqual(run_session(session, 1), 0)

            session.keep_going()
            self.assertIsNone(session.ended)
            self.assertEqual(run_session(session, 2), 2 * session.speed)

    def test_frame_budget_on_slow_clock(self):
        """Тик дольше бюджета кадра — за кадр ровно один тик, и это видно."""
        session = Session(self.settings())
        session.set_speed_index(5)
        clock = iter(range(1000)).__next__               # каждый вызов — «секунда»
        self.assertEqual(session.advance(1 / 60, budget=0.012, clock=clock), 1)
        self.assertTrue(session.lagging)
        self.assertEqual(session.advance(1 / 60, budget=UNLIMITED), session.speed)
        self.assertFalse(session.lagging)

    def test_pause_and_step(self):
        session = Session(self.settings())
        self.assertEqual(session.step_once(), 0, "шаг без паузы не нужен")
        session.toggle_pause()
        self.assertEqual(run_session(session, 3), 0)
        self.assertEqual(session.step_once(), 1)
        self.assertEqual(session.world.tick, 1)

    def test_settings_are_copied(self):
        s = self.settings()
        session = Session(s)
        s.n_vegetarians = 1
        self.assertEqual(session.settings.n_vegetarians, 30,
                         "правка настроек в меню изменила идущую партию")


class TestPicking(unittest.TestCase):
    """Выбор существа кликом — чистая функция, pygame ей не нужен."""

    def setUp(self):
        random.seed(0)
        self.world = World(seed=0, n_vegetarians=0, n_predators=0)
        self.near  = Vegetarian(x=1000, y=1000)
        self.close = Vegetarian(x=1030, y=1000)
        self.hunter = Predator(x=3000, y=3000)
        self.big = Vegetarian(x=4000, y=1000, genom=[400, 10, 400, 70, 30, 5, 100])
        self.world.vegetarians = [self.near, self.close, self.big]
        self.world.predators   = [self.hunter]

    def test_picks_nearest(self):
        self.assertIs(pick_creature(self.world, 1005, 1000, 50), self.near)
        self.assertIs(pick_creature(self.world, 1025, 1000, 50), self.close)

    def test_picks_predator(self):
        self.assertIs(pick_creature(self.world, 3000, 3010, 50), self.hunter)

    def test_click_anywhere_on_a_big_body(self):
        self.assertIs(pick_creature(self.world, 4180, 1000, 5), self.big)

    def test_miss_returns_none(self):
        self.assertIsNone(pick_creature(self.world, 2000, 2000, 50))


class TestAppLayering(unittest.TestCase):

    def test_app_logic_does_not_import_pygame(self):
        """Логика приложения проверяется без дисплея — и должна такой остаться."""
        out = subprocess.run(
            [sys.executable, "-c",
             "import app.settings, app.session, app.history, app.camera, sys; "
             "print('pygame' in sys.modules)"],
            cwd=str(PROJECT_ROOT), capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(out.returncode, 0, "импорт упал: " + out.stderr)
        self.assertEqual(out.stdout.strip(), "False",
                         "логика приложения тянет pygame — её не проверить без дисплея")


# ─────────────────────────────────────────────────────────────────────────────
# Отрисовка
# ─────────────────────────────────────────────────────────────────────────────

def color_near(surf, area, color, tolerance=40):
    """Сколько пикселей в area близки к color (сглаженные линии не дают точного цвета)."""
    area = pygame.Rect(area).clip(surf.get_rect())
    count = 0
    for x in range(area.left, area.right, 2):
        for y in range(area.top, area.bottom, 2):
            c = surf.get_at((x, y))
            if sum(abs(a - b) for a, b in zip(c[:3], color)) <= tolerance:
                count += 1
    return count


@unittest.skipUnless(pygame is not None, "pygame не установлен")
class TestDrawing(unittest.TestCase):

    def setUp(self):
        pygame.font.init()
        theme.set_scale(1.0)
        self.surf = pygame.Surface((1200, 800))
        self.cam = Camera(WORLD_WIDTH, WORLD_HEIGHT, (0, 0, 1200, 800))

    def live_session(self, ticks):
        session = Session(Settings(seed=1, random_seed=False))
        run_session(session, ticks)
        return session

    def test_world_is_drawn(self):
        """Растения, травоядные и хищники видны своими цветами — ровно там, где стоят."""
        world = World(seed=1, n_vegetarians=0, n_predators=0)
        plant = Plant()
        plant.x, plant.y = 500, 500
        veg = Vegetarian(x=1000, y=1000)
        veg.energy = veg.max_energy
        hunter = Predator(x=3000, y=3000)
        world.plants, world.vegetarians, world.predators = [plant], [veg], [hunter]

        for smooth in (True, False):
            with self.subTest(smooth=smooth):
                render.draw_world(self.surf, world, self.cam, smooth=smooth)
                at = lambda c: tuple(self.surf.get_at(
                    tuple(int(v) for v in self.cam.to_screen(c.x, c.y))))[:3]
                self.assertEqual(at(plant), render._PLANT_DIM)
                self.assertEqual(at(veg), render._VEG_SHADES[-1])
                self.assertEqual(at(hunter), theme.PREDATOR_COLOR)

    def test_selection_for_both_kinds(self):
        world = self.live_session(50).world
        self.assertTrue(world.vegetarians and world.predators, "выбирать некого")
        self.cam.zoom_at(600, 400, 4.0)
        for creature in (world.vegetarians[0], world.predators[0]):
            with self.subTest(kind=type(creature).__name__):
                self.cam.follow(creature)
                self.cam.update(10.0)                     # подвели камеру к цели
                render.draw_world(self.surf, world, self.cam, selected=creature)
                sx, sy = self.cam.to_screen(creature.x, creature.y)
                ring = pygame.Rect(0, 0, 200, 200)
                ring.center = (int(sx), int(sy))
                self.assertGreater(color_near(self.surf, ring, theme.ACCENT, 10), 0,
                                   "кольца выделения нет")

    def test_card_for_both_kinds(self):
        session = self.live_session(50)
        avg = session.history.last.genom
        for creature in (session.world.vegetarians[0], session.world.predators[0]):
            for compact in (False, True):
                with self.subTest(kind=type(creature).__name__, compact=compact):
                    h = render.card_height(creature, avg, compact)
                    render.draw_creature_card(self.surf, (0, 0, 300, h), creature, avg, compact)
        veg = session.world.vegetarians[0]
        self.assertLess(render.card_height(veg, avg, True), render.card_height(veg, avg))
        render.draw_creature_card(self.surf, (0, 0, 300, 300), veg, None)   # генома нет

    def test_charts_edge_cases(self):
        """Графики не падают, когда рисовать почти нечего.

        Пусто — первый кадр; одна точка — первые тики; все нули и геном None —
        всё вымерло (без защиты было бы деление на ноль); тесный прямоугольник —
        низкое окно.
        """
        genom = (40.0,) * 7
        cases = {
            "пусто":         [],
            "одна точка":    [Sample(0, 5, 3, 1, genom)],
            "все нули":      [Sample(i, 0, 0, 0, None) for i in range(10)],
            "хищники — 0":   [Sample(0, 100, 50, 0, genom), Sample(10, 120, 40, 0, genom)],
            "геном пропал":  [Sample(0, 1, 1, 1, genom), Sample(10, 1, 0, 1, None),
                              Sample(20, 1, 1, 1, (41.0,) * 7)],
        }
        for rect in ((0, 0, 300, 400), (0, 0, 300, 40)):
            for name, points in cases.items():
                for hover in (None, 150):
                    with self.subTest(name, rect=rect, hover=hover):
                        render.draw_population_chart(self.surf, rect, points, None, hover)
                        render.draw_genome_chart(self.surf, rect, points, 300, hover)

    def test_big_creature_at_the_edge_is_drawn(self):
        """Крупное существо видно, даже когда его центр за краем кадра.

        Баг: отсечение шло по центру с запасом 200, и тело диаметром 800,
        на 150 px заходящее в кадр, пропадало целиком. При пологой цене
        размера такие тела вырастают сами.
        """
        self.cam.zoom, self.cam.cx, self.cam.cy = 1.0, 3000, 2000   # видно x 2400..3600
        world = World(seed=1, n_vegetarians=0, n_predators=0)
        big = Vegetarian(x=3850, y=2000, genom=[800, 10, 400, 70, 30, 5, 100])
        big.energy = big.max_energy
        hunter = Predator(x=2400 - 15, y=2000)            # центр за левым краем
        world.plants, world.vegetarians, world.predators = [], [big], [hunter]

        render.draw_world(self.surf, world, self.cam)
        self.assertEqual(tuple(self.surf.get_at((1150, 400)))[:3], render._VEG_SHADES[-1],
                         "тело крупного существа у края не нарисовано")
        self.assertEqual(tuple(self.surf.get_at((2, 400)))[:3], theme.PREDATOR_COLOR,
                         "хищник у левого края не нарисован")

    def test_genome_change_is_from_the_start_of_the_game(self):
        """«Изменение от начала» одно и то же в окне «3000» и на всей партии.

        Баг: базой была первая точка показанного участка, и рост размера
        с 40 до 80 в окне последних тиков выглядел как 0%.
        """
        h = History(recent=3)
        h.add(Sample(0, 0, 0, 0, (40.0,) * 7))
        for i in range(1, 10):
            h.add(Sample(i * 10, 0, 0, 0, (80.0,) * 7))
        for whole in (False, True):
            with self.subTest(whole=whole),                     mock.patch.object(render, "blit_text", wraps=render.blit_text) as blit:
                render.draw_genome_chart(self.surf, pygame.Rect(0, 0, 300, 300),
                                         h.series(whole), origin=h.origin)
                changes = {c.args[2] for c in blit.call_args_list if c.args[1] == "tiny"}
                self.assertEqual(changes, {"+100%"})

    def test_population_chart_from_live_session(self):
        """Настоящая история партии: линии всех трёх видов на месте."""
        session = self.live_session(600)
        self.assertIsNone(session.ended, "партия оборвалась")
        rect = pygame.Rect(0, 0, 400, 300)
        points = session.history.series(whole=False)
        index = render.draw_population_chart(self.surf, rect, points, 300, hover_x=200)
        self.assertIsNotNone(index, "подсказка под курсором не построилась")
        self.surf.fill(theme.PANEL)
        render.draw_population_chart(self.surf, rect, points, 300)
        for (name, _), (_, color) in zip(theme.SPECIES, render.POP_SERIES):
            self.assertGreater(color_near(self.surf, rect, color), 3, f"линии «{name}» нет")

    def test_cached_text_follows_the_text(self):
        """Кэш надписей отдаёт надпись для своего текста, а не прошлую.

        Ошибка в ключе кэша заморозила бы счётчики на первом значении — и это
        не упало бы, а просто перестало обновляться.
        """
        shots = []
        for s in ("111", "WWW", "111"):
            self.surf.fill(theme.BG)
            render.blit_text(self.surf, "body", s, theme.TEXT, "topleft", topleft=(10, 10))
            shots.append(pygame.image.tobytes(self.surf, "RGB"))
        self.assertNotEqual(shots[0], shots[1], "разный текст нарисован одинаково")
        self.assertEqual(shots[0], shots[2], "один и тот же текст нарисован по-разному")

    def test_every_icon_draws(self):
        for name in ("play", "pause", "step", "minus", "plus", "fit", "menu",
                     "dice", "close", "follow", "back"):
            with self.subTest(name):
                self.surf.fill((0, 0, 0))
                render.draw_icon(self.surf, name, pygame.Rect(0, 0, 40, 40), theme.TEXT)
                self.assertGreater(color_near(self.surf, (0, 0, 40, 40), theme.TEXT, 60), 0)


# ─────────────────────────────────────────────────────────────────────────────
# Экраны целиком: App по кадрам с синтетическими событиями
# ─────────────────────────────────────────────────────────────────────────────

@unittest.skipUnless(pygame is not None, "pygame не установлен")
class AppCase(unittest.TestCase):

    def make_app(self, size=(1280, 800), path=None):
        if path is None:
            tmp = tempfile.TemporaryDirectory()
            self.addCleanup(tmp.cleanup)
            path = Path(tmp.name) / "settings.json"
        self.settings_path = path
        return App(size=size, settings_path=path, headless=True)

    @staticmethod
    def frame(app, *events, frames=1):
        for _ in range(frames):
            app.frame(list(events), 1 / 60)
            events = ()

    def move(self, app, pos):
        self.frame(app, pygame.event.Event(pygame.MOUSEMOTION, pos=pos, rel=(0, 0),
                                           buttons=(0, 0, 0)))

    def click(self, app, target, button=1):
        if isinstance(target, pygame.Rect):
            pos = target.center
        elif hasattr(target, "rect"):
            pos = target.rect.center
        else:
            pos = tuple(map(int, target))
        self.frame(app,
                   pygame.event.Event(pygame.MOUSEMOTION, pos=pos, rel=(0, 0), buttons=(0, 0, 0)),
                   pygame.event.Event(pygame.MOUSEBUTTONDOWN, pos=pos, button=button),
                   pygame.event.Event(pygame.MOUSEBUTTONUP, pos=pos, button=button))

    def drag(self, app, start, end, button=1):
        self.frame(app,
                   pygame.event.Event(pygame.MOUSEBUTTONDOWN, pos=start, button=button),
                   pygame.event.Event(pygame.MOUSEMOTION, pos=end, rel=(0, 0), buttons=(1, 0, 0)),
                   pygame.event.Event(pygame.MOUSEBUTTONUP, pos=end, button=button))

    def key(self, app, key, unicode=""):
        self.frame(app, pygame.event.Event(pygame.KEYDOWN, key=key, mod=0,
                                           unicode=unicode, scancode=0),
                   pygame.event.Event(pygame.KEYUP, key=key, mod=0, unicode=unicode,
                                      scancode=0))

    @staticmethod
    def button(owner, label):
        found = [b for b in owner.buttons if b.label == label]
        assert found, f"нет кнопки «{label}»"
        return found[0]


class TestAppFlow(AppCase):

    def test_menu_setup_game_and_back(self):
        app = self.make_app()
        self.frame(app, frames=3)
        menu = app.scene
        self.assertIsInstance(menu, MenuScene)
        self.assertFalse(menu.continue_btn.visible, "«Продолжить» без партии")

        self.click(app, menu.new_btn)
        setup = app.scene
        self.assertIsInstance(setup, SetupScene)

        by_key = {s.field.key: s for s in setup.sliders}
        veg, pred = by_key["n_vegetarians"], by_key["n_predators"]
        self.drag(app, veg.rect.center, (veg.track.right + 50, veg.rect.centery))
        self.drag(app, pred.rect.center, (pred.track.left - 50, pred.rect.centery))
        self.assertEqual((app.settings.n_vegetarians, app.settings.n_predators), (200, 0))

        self.click(app, setup.random_seed)
        self.assertFalse(app.settings.random_seed)
        self.click(app, setup.seed_field)
        for digit in "42":
            self.key(app, getattr(pygame, f"K_{digit}"), digit)
        self.key(app, pygame.K_RETURN)
        self.assertEqual(app.settings.seed, 42)
        self.assertIsInstance(app.scene, SetupScene, "Enter в поле сида начал игру")

        self.click(app, setup.start_btn)
        game = app.scene
        self.assertIsInstance(game, GameScene)
        world = game.session.world
        start = game.session.history.full[0]              # снимок до первого тика
        self.assertEqual((game.session.seed, start.vegetarians, start.predators),
                         (42, 200, 0))
        saved = json.loads(self.settings_path.read_text(encoding="utf-8"))
        self.assertEqual((saved["seed"], saved["n_vegetarians"]), (42, 200))

        # темп: x1 — ровно тик за кадр
        tick = world.tick
        self.frame(app, frames=5)
        self.assertEqual(world.tick, tick + 5)
        self.key(app, pygame.K_SPACE)
        self.frame(app, frames=3)
        self.assertEqual(world.tick, tick + 5, "пауза не остановила партию")
        self.key(app, pygame.K_RIGHT)
        self.assertEqual(world.tick, tick + 6, "шаг на паузе не сработал")
        self.key(app, pygame.K_EQUALS, "=")
        self.assertEqual(game.session.speed, 2)
        self.click(app, game.play_btn)
        self.assertFalse(game.session.paused)

        # камера и выбор
        view = pygame.Rect(game.cam.view)
        self.move(app, view.center)
        self.frame(app, pygame.event.Event(pygame.MOUSEWHEEL, x=0, y=3, flipped=False))
        self.assertFalse(game.cam.is_fit, "колесо не приблизило")

        self.key(app, pygame.K_SPACE)                     # на паузе существо не убежит
        target = min(world.vegetarians,
                     key=lambda v: math.dist(game.cam.to_screen(v.x, v.y), view.center))
        self.click(app, game.cam.to_screen(target.x, target.y))
        self.assertIs(game.selected, target, "клик не выбрал существо")
        self.assertTrue(game.follow_btn.visible and game.close_btn.visible)
        self.click(app, game.follow_btn)
        self.assertIs(game.cam.target, target)
        self.click(app, game.close_btn)
        self.assertIsNone(game.selected)

        center = game.cam.cx, game.cam.cy
        self.drag(app, view.center, (view.centerx + 120, view.centery + 60))
        self.assertNotEqual((game.cam.cx, game.cam.cy), center, "перетаскивание не сдвинуло вид")
        self.assertIsNone(game.selected, "перетаскивание выбрало существо")
        self.key(app, pygame.K_HOME)
        self.assertTrue(game.cam.is_fit)

        # графики и панель
        self.key(app, pygame.K_g)
        self.assertEqual(game.chart, "genome")
        self.click(app, game.range_tabs.rect.move(game.range_tabs.rect.w // 4, 0))
        self.assertTrue(game.whole)
        self.key(app, pygame.K_TAB)
        self.assertEqual(game.cam.view[2], app.screen.get_width(), "панель не скрылась")
        self.key(app, pygame.K_TAB)

        # меню паузы -> главное меню -> «Продолжить»
        self.key(app, pygame.K_SPACE)                     # снова играем
        self.assertFalse(game.session.paused)
        self.key(app, pygame.K_ESCAPE)
        self.assertEqual(game.overlay.kind, "pause")
        tick = world.tick
        self.key(app, pygame.K_SPACE)                     # под модальным окном — не играем
        self.frame(app, frames=3)
        self.assertEqual(world.tick, tick)
        self.click(app, self.button(game.overlay, "Главное меню"))
        menu = app.scene
        self.assertIsInstance(menu, MenuScene)
        self.assertTrue(menu.continue_btn.visible)
        self.click(app, menu.continue_btn)
        self.assertIs(app.scene, game)
        self.assertIsNone(game.overlay, "«Продолжить» оставило меню паузы")

    def test_typed_seed_survives_tab_switch(self):
        """Введённый сид не теряется, если сразу переключить вкладку.

        Баг: вкладки забирали клик раньше поля и прятали его, ввод
        оставался незавершённым, и игра начиналась со старым сидом.
        """
        app = self.make_app()
        app.open_setup()
        setup = app.scene
        app.settings.random_seed = False
        self.click(app, setup.seed_field)
        for digit in "42":
            self.key(app, getattr(pygame, f"K_{digit}"), digit)
        self.click(app, setup.tabs._cells()[1])           # «Лаборатория»
        self.assertEqual(setup.tab, "lab")
        self.click(app, setup.start_btn)
        self.assertEqual(app.session.seed, 42)

    def test_end_card_and_restart(self):
        app = self.make_app()
        app.settings.random_seed = False
        app.settings.seed = 3
        app.start_game()
        game = app.scene
        self.frame(app, frames=10)

        world = game.session.world
        for c in world.vegetarians + world.predators:
            c.alive = False
        world.vegetarians, world.predators = [], []
        self.frame(app)
        self.assertEqual(game.overlay.kind, "extinct")
        self.assertTrue(game.toasts, "заметок о вымирании нет")

        self.click(app, self.button(game.overlay, "Посмотреть графики"))
        self.assertIsNone(game.overlay)
        self.frame(app, frames=3)
        self.assertIsNone(game.overlay, "карточка конца вернулась сама")
        self.click(app, game.play_btn)
        self.assertEqual(game.overlay.kind, "extinct", "«играть» после конца не показало итог")

        self.click(app, self.button(game.overlay, "Заново — тот же мир"))
        again = app.scene
        self.assertIsNot(again, game)
        self.assertEqual((again.session.seed, again.session.history.full[0].tick), (3, 0))
        fresh = Session(app.settings, seed=3)
        run_session(fresh, again.session.world.tick)      # столько же тиков, сколько прошло
        self.assertEqual([(v.x, v.y) for v in again.session.world.vegetarians],
                         [(v.x, v.y) for v in fresh.world.vegetarians])

    def test_explosion_card_keep_going(self):
        app = self.make_app()
        with mock.patch("app.session.EXPLOSION_LIMIT", 5):
            app.start_game()
            game = app.scene
            self.frame(app)
            self.assertEqual(game.overlay.kind, "explosion")
            self.click(app, self.button(game.overlay, "Продолжить всё равно"))
            self.assertIsNone(game.overlay)
            tick = game.session.world.tick
            self.frame(app, frames=3)
            self.assertEqual(game.session.world.tick, tick + 3)
            self.assertIsNone(game.overlay)

    def test_selected_creature_death_is_reported(self):
        app = self.make_app()
        app.start_game()
        game = app.scene
        victim = game.session.world.vegetarians[0]
        game.select(victim)
        victim.alive, victim.energy = False, 0
        self.frame(app)
        self.assertIsNone(game.selected)
        self.assertIn("съели", game.toasts[-1][0])

    def test_settings_survive_restart_of_the_app(self):
        first = self.make_app()
        first.settings.plant_energy = 80
        first.settings.random_seed = False
        first.start_game()
        second = self.make_app(path=self.settings_path)
        self.assertEqual(second.settings.plant_energy, 80)
        self.assertFalse(second.settings.random_seed)

    def test_prefs(self):
        app = self.make_app(size=(1280, 800))
        app.open_prefs()
        prefs = app.scene
        self.assertIsInstance(prefs, PrefsScene)
        cells = prefs.scale._cells()
        self.click(app, cells[3])                          # 150%
        self.assertEqual(app.settings.ui_scale, 1.5)
        self.assertAlmostEqual(app.effective_scale, 800 / 600, msg="масштаб не ужался под окно")

        self.click(app, prefs.smooth)
        self.assertFalse(app.settings.smooth)
        self.key(app, pygame.K_F11)
        self.assertTrue(app.settings.fullscreen)
        self.key(app, pygame.K_ESCAPE)
        self.assertIsInstance(app.scene, MenuScene)

    def test_quit(self):
        app = self.make_app()
        self.frame(app, pygame.event.Event(pygame.QUIT))
        self.assertFalse(app.running)


class TestLayout(AppCase):
    """Страж «дизайн не развалился»: на любом размере окна и масштабе всё на месте.

    Виджеты внутри окна, не налезают друг на друга, текст влезает в кнопки и
    слайдеры, график не схлопнулся.
    """

    SIZES  = ((960, 600), (1280, 800), (1440, 900), (2560, 1440), (800, 500))
    SCALES = (1.0, 1.5, 2.0)

    def screens(self, app):
        """Все экраны по очереди: (название, сцена, модальное окно или None)."""
        yield "меню", MenuScene(app), None
        setup = SetupScene(app)
        yield "новый мир", setup, None
        yield "лаборатория", setup, "lab"
        yield "настройки", PrefsScene(app), None
        yield "справка", HelpScene(app), None

        app.settings.random_seed = False
        app.start_game()
        game = app.game
        game.session.advance(1 / 60, budget=UNLIMITED)
        yield "игра", game, None
        for creature in (game.session.world.vegetarians[0], game.session.world.predators[0]):
            game.select(creature)
            yield f"игра: {type(creature).__name__}", game, None
        game.open_pause()
        yield "пауза", game, None
        game.close_overlay()
        game.session.ended = "extinct"
        game.show_end()
        yield "конец", game, None

    def check(self, app, name, scene):
        screen = app.screen.get_rect()
        widgets = [w for w in scene.active_widgets() if w.interactive]
        overlay = getattr(scene, "overlay", None)
        if overlay is not None:
            widgets = list(overlay.buttons)
            self.assertTrue(screen.contains(overlay.card), f"{name}: модальное окно за краем")
        for w in widgets:
            self.assertTrue(screen.contains(w.rect), f"{name}: {type(w).__name__} за краем окна")
            self.assertFalse(w.problems(), f"{name}: {w.problems()}")
        for a, b in combinations(widgets, 2):
            self.assertFalse(a.rect.colliderect(b.rect),
                             f"{name}: {type(a).__name__} налезает на {type(b).__name__}")
        panel = getattr(scene, "panel", None)
        if panel is not None:
            self.assertTrue(screen.contains(panel), f"{name}: карточка экрана за краем окна")
        if isinstance(scene, GameScene) and overlay is None:
            self.assertGreaterEqual(scene.chart_rect.h, theme.S(120), f"{name}: график схлопнулся")
            for w in widgets:
                self.assertFalse(w.rect.colliderect(scene.chart_rect),
                                 f"{name}: {type(w).__name__} на графике")

    def test_all_screens_fit(self):
        for size in self.SIZES:
            for scale in self.SCALES:
                app = self.make_app(size=size)
                app.system_scale = scale
                app.layout()
                for name, scene, tab in self.screens(app):
                    with self.subTest(size=size, scale=scale, screen=name):
                        if tab is not None:
                            scene._set_tab(tab)
                        if app.scene is not scene:
                            app.go(scene, remember=False)
                        scene.layout(app.screen.get_size())
                        self.frame(app)
                        self.check(app, name, scene)


if __name__ == "__main__":
    unittest.main()
