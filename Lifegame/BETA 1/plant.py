# plant.py
import pygame, random
from config import WORLD_WIDTH, WORLD_HEIGHT, PLANT_RADIUS

class Plant:
    def __init__(self):
        self.x = random.randint(PLANT_RADIUS, WORLD_WIDTH  - PLANT_RADIUS)
        self.y = random.randint(PLANT_RADIUS, WORLD_HEIGHT - PLANT_RADIUS)

    def draw(self, surf, scale_x, scale_y):
        pygame.draw.circle(
            surf, (0, 200, 0),
            (int(self.x * scale_x), int(self.y * scale_y)),
            max(1, int(PLANT_RADIUS * scale_x))
        )
