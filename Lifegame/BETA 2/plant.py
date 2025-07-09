# plant.py

import pygame, random, math
from config import *


class Plant:
    def __init__(self):
        while True:
            x = random.uniform(PLANT_RADIUS , WORLD_WIDTH  - PLANT_RADIUS)
            y = random.uniform(PLANT_RADIUS + 200, WORLD_HEIGHT - PLANT_RADIUS)

            decay = 7.0
            spawn_chance = math.exp(-decay * (y / WORLD_HEIGHT))

            if random.random() < spawn_chance:
                self.x = x
                self.y = y
                break

    def draw(self, surf, scale_x, scale_y):
        pygame.draw.circle(
            surf, (0, 255, 0),
            (int(self.x * scale_x), int(self.y * scale_y)),
            max(1, int(PLANT_RADIUS * scale_x))
        )