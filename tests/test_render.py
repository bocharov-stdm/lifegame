"""Тесты окна: отрисовка и выбор существа.

Запуск:   python -m unittest discover tests

Окно не открывается: SDL работает с драйвером dummy, а всё рисуется на
обычную pygame.Surface. Проверяется не красота, а то, что отрисовка не
падает — в том числе на краевых случаях, которые в живом окне ловятся
только случайно. Если pygame не установлен, тесты пропускаются: движку
(life/) pygame не нужен, и его тесты от этого файла не зависят.
"""

import os
import random
from collections import deque
import sys
import unittest
from pathlib import Path

os.environ.setdefault("SDL_VIDEODRIVER", "dummy")
os.environ.setdefault("PYGAME_HIDE_SUPPORT_PROMPT", "1")

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

try:
    import pygame
except ImportError:
    pygame = None

from life.config     import *
from life.headless   import simulate
from life.plant      import Plant
from life.predator   import Predator
from life.vegetarian import Vegetarian
from life.world      import World

if pygame is not None:
    import main
    import render


SCALE_X = WIDTH  / WORLD_WIDTH
SCALE_Y = HEIGHT / WORLD_HEIGHT


def live_run(ticks, on_tick=None):
    """Живой мир через simulate(): те же лимиты, что у всех прогонов в тестах.

    Голый цикл по world.step() ограничен только числом тиков, а стоимость тика
    растёт с популяцией — сломанный баланс превратил бы его в долгий прогон.
    """
    return simulate(seed=1, ticks=ticks, sample_every=ticks, seconds=10.0,
                    on_tick=on_tick)


@unittest.skipUnless(pygame is not None, "pygame не установлен")
class TestDrawing(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        pygame.font.init()
        cls.font = pygame.font.SysFont(None, 24)

    def setUp(self):
        self.surf = pygame.Surface((WIDTH, HEIGHT))
        self.surf.fill((30, 30, 30))

    def colors(self, area=None):
        """Цвета на экране или в прямоугольнике area (каждый второй пиксель — хватает)."""
        area = area or pygame.Rect(0, 0, WIDTH, HEIGHT)
        get = self.surf.get_at
        return {tuple(get((x, y)))[:3]
                for x in range(area.left, area.right, 2)
                for y in range(area.top, area.bottom, 2)}

    def test_world_is_drawn(self):
        """Растения, травоядные и хищники видны своими цветами."""
        world = World(seed=1)
        world.plants      = [Plant()]
        world.vegetarians = [Vegetarian(x=1000, y=1000)]
        world.predators   = [Predator(x=3000, y=3000)]

        render.draw_world(self.surf, world, SCALE_X, SCALE_Y)

        colors = self.colors()
        self.assertIn(render.PLANT_COLOR,      colors)
        self.assertIn(render.VEGETARIAN_COLOR, colors)
        self.assertIn(render.PREDATOR_COLOR,   colors)

    def test_selection_and_panel_for_both_kinds(self):
        """Кольцо и панель рисуются и для травоядного, и для хищника (у него нет генома)."""
        world = live_run(50).world
        self.assertTrue(world.vegetarians, "травоядные вымерли — выбирать некого")
        self.assertTrue(world.predators,   "хищники вымерли — выбирать некого")
        for creature in (world.vegetarians[0], world.predators[0]):
            with self.subTest(kind=type(creature).__name__):
                self.setUp()
                render.draw_selection(self.surf, creature, SCALE_X, SCALE_Y)
                render.draw_panel(self.surf, self.font, creature)
                self.assertIn(render.ACCENT_COLOR, self.colors())

    def test_graph_edge_cases(self):
        """График не падает, когда рисовать почти нечего.

        Пустая история — первый кадр; одна точка — первые 10 тиков; все нули —
        всё вымерло. В последнем случае максимум ряда равен нулю, и без защиты
        было бы деление на ноль.
        """
        cases = {
            "пусто":       [],
            "одна точка":  [(5, 3, 1)],
            "все нули":    [(0, 0, 0)] * 10,
            "хищники — 0": [(100, 50, 0), (120, 40, 0)],
        }
        for name, history in cases.items():
            with self.subTest(name):
                self.setUp()
                render.draw_graph(self.surf, self.font, history)

    def test_graph_from_live_world(self):
        """Настоящая история, как её копит main.py: линии всех трёх цветов на месте."""
        history = deque(maxlen=main.GRAPH_POINTS)

        def sample(world):
            if world.tick % main.GRAPH_EVERY == 0:
                history.append((len(world.plants),
                                len(world.vegetarians),
                                len(world.predators)))

        res = live_run(600, on_tick=sample)
        self.assertEqual(res.stop_reason, "готово", f"прогон оборвался: {res.stop_reason}")

        render.draw_graph(self.surf, self.font, history)

        # смотрим только под легендой: там те же цвета, и без линий тест бы не заметил
        width, height = render.GRAPH_SIZE
        legend_bottom = render.PAD + self.font.get_linesize()
        plot = pygame.Rect(WIDTH - render.MARGIN - width,
                           HEIGHT - render.MARGIN - height + legend_bottom,
                           width, height - legend_bottom)
        colors = self.colors(plot)
        for name, color in render.GRAPH_SERIES:
            self.assertIn(color, colors, f"линии «{name}» на графике нет")

    def test_hud_in_both_states(self):
        for paused in (False, True):
            with self.subTest(paused=paused):
                render.draw_hud(self.surf, self.font, paused, 16, 12345)

    def test_cached_text_follows_the_text(self):
        """Кэш надписей отдаёт надпись для своего текста, а не прошлую.

        Надписи кэшируются (render._text); ошибка в ключе кэша заморозила бы
        строку статистики на первом значении — и это не упало бы, а просто
        перестало обновляться.
        """
        shots = []
        for text in ("111", "WWW", "111"):
            self.setUp()
            render.draw_stats(self.surf, self.font, text)
            self.assertIn(render.TEXT_COLOR, self.colors(pygame.Rect(0, 0, 200, 40)))
            shots.append(pygame.image.tobytes(self.surf, "RGB"))

        self.assertNotEqual(shots[0], shots[1], "разный текст нарисован одинаково")
        self.assertEqual(shots[0], shots[2], "один и тот же текст нарисован по-разному")


class TestPicking(unittest.TestCase):
    """Выбор существа кликом — чистая функция, pygame ей не нужен."""

    def setUp(self):
        if pygame is None:
            self.skipTest("pygame не установлен: main.py без него не импортируется")
        random.seed(0)
        self.world = World(seed=0)
        self.near  = Vegetarian(x=1000, y=1000)
        self.close = Vegetarian(x=1030, y=1000)
        self.hunter = Predator(x=3000, y=3000)
        self.world.vegetarians = [self.near, self.close]
        self.world.predators   = [self.hunter]

    def test_picks_nearest(self):
        self.assertIs(main.pick_creature(self.world, 1005, 1000, 50), self.near)
        self.assertIs(main.pick_creature(self.world, 1025, 1000, 50), self.close)

    def test_picks_predator(self):
        self.assertIs(main.pick_creature(self.world, 3000, 3010, 50), self.hunter)

    def test_miss_returns_none(self):
        self.assertIsNone(main.pick_creature(self.world, 2000, 2000, 50))


if __name__ == "__main__":
    unittest.main()
