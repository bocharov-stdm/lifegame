# plant.py

import pygame, random
from config import WORLD_WIDTH, WORLD_HEIGHT, PLANT_RADIUS

class Plant:
    def __init__(self):
        while True:
            x = random.uniform(PLANT_RADIUS, WORLD_WIDTH  - PLANT_RADIUS)
            y = random.uniform(PLANT_RADIUS, WORLD_HEIGHT - PLANT_RADIUS)

            #spawn_chance = x / WORLD_WIDTH

            #if random.random() < spawn_chance:
            self.x = x
            self.y = y
            break

    def draw(self, surf, scale_x, scale_y):
        pygame.draw.circle(
            surf, (0, 255, 0),
            (int(self.x * scale_x), int(self.y * scale_y)),
            max(1, int(PLANT_RADIUS * scale_x))
        )