# plant.py

import math
import random

import pygame

from config import (WORLD_WIDTH, WORLD_HEIGHT, PLANT_RADIUS, PLANT_DEPTH_DECAY)


class Plant:
    """Растение. Плотность падает с глубиной — верх мира кормит лучше."""

    __slots__ = ("x", "y", "alive")

    def __init__(self, x=None, y=None):
        if x is None or y is None:
            x, y = self._spawn_position()
        self.x = x
        self.y = y
        self.alive = True

    @staticmethod
    def _spawn_position():
        while True:
            x = random.uniform(PLANT_RADIUS, WORLD_WIDTH - PLANT_RADIUS)
            y = random.uniform(PLANT_RADIUS, WORLD_HEIGHT - PLANT_RADIUS)
            if random.random() < math.exp(-PLANT_DEPTH_DECAY * (y / WORLD_HEIGHT)):
                return x, y

    def draw(self, surf, scale_x, scale_y):
        pygame.draw.circle(
            surf, (0, 255, 0),
            (int(self.x * scale_x), int(self.y * scale_y)),
            max(1, int(PLANT_RADIUS * scale_x)),
        )
