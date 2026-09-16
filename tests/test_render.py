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
from life.plant      import Plant
from life.predator   import Predator
from life.vegetarian import Vegetarian
from life.world      import World

if pygame is not None:
    import main
    import render


SCALE_X = WIDTH  / WORLD_WIDTH
SCALE_Y = HEIGHT / WORLD_HEIGHT


def live_world(ticks=50):
    world = World(seed=1)
    for _ in range(ticks):                       # фиксированное число тиков
        world.step()
    return world


@unittest.skipUnless(pygame is not None, "pygame не установлен")
class TestDrawing(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        pygame.font.init()
        cls.font = pygame.font.SysFont(None, 24)

    def setUp(self):
        self.surf = pygame.Surface((WIDTH, HEIGHT))
        self.surf.fill((30, 30, 30))

    def colors(self):
        """Все цвета на экране (каждый второй пиксель — этого хватает)."""
        get = self.surf.get_at
        return {tuple(get((x, y)))[:3]
                for x in range(0, WIDTH, 2) for y in range(0, HEIGHT, 2)}

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
        world = live_world()
        for creature in (world.vegetarians[0], world.predators[0]):
            with self.subTest(kind=type(creature).__name__):
                self.setUp()
                render.draw_selection(self.surf, creature, SCALE_X, SCALE_Y)
                render.draw_panel(self.surf, self.font, creature)
                self.assertIn(render.ACCENT_COLOR, self.colors())

    def test_hud_in_both_states(self):
        for paused in (False, True):
            with self.subTest(paused=paused):
                render.draw_hud(self.surf, self.font, paused, 16, 12345)


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
