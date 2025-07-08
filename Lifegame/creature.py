# creature.py

import pygame
import random
import math
from config import *

class Creature:
    DIAM = 20

    def __init__(self, x=None, y=None, energy=MAX_ENERGY * 0.5,
                 speed=BASE_SPEED, smell_radius=BASE_SMELL):
        self.x = x if x is not None else random.randint(self.DIAM, WORLD_WIDTH - self.DIAM)
        self.y = y if y is not None else random.randint(self.DIAM, WORLD_HEIGHT - self.DIAM)
        self.energy = energy
        self.speed = speed
        self.smell_radius = smell_radius
        self._choose_new_target()

    def _choose_new_target(self):
        self.tx = random.randint(self.DIAM, WORLD_WIDTH - self.DIAM)
        self.ty = random.randint(self.DIAM, WORLD_HEIGHT - self.DIAM)

    def _vector_towards(self, tx, ty):
        dx, dy = tx - self.x, ty - self.y
        dist = math.hypot(dx, dy)
        return (0, 0) if dist == 0 else (dx * self.speed / dist, dy * self.speed / dist)

    def _nearest_plant_in_smell(self, plants):
        nearest, best_d = None, self.smell_radius
        for p in plants:
            d = math.hypot(self.x - p.x, self.y - p.y)
            if d < best_d:
                nearest, best_d = p, d
        return nearest

    def move(self, plants):
        # Пытаемся "унюхать" ближайшее растение
        nearest_plant = self._nearest_plant_in_smell(plants)

        if nearest_plant:
            # Если учуяли растение — идём к нему
            dx, dy = self._vector_towards(nearest_plant.x, nearest_plant.y)
        else:
            # Иначе идём к случайной цели
            dx, dy = self._vector_towards(self.tx, self.ty)

        # Двигаемся, но не выходим за границы мира
        self.x = min(max(self.x + dx, self.DIAM), WORLD_WIDTH - self.DIAM)
        self.y = min(max(self.y + dy, self.DIAM), WORLD_HEIGHT - self.DIAM)

        # Тратим энергию за движение и за использование обоняния
        move_cost = K_MOVE * self.speed ** 2
        smell_cost = K_SMELL * self.smell_radius
        self.energy -= move_cost + smell_cost + BASAL_METABOLISM

        # Если мы не нашли еду и почти пришли к случайной цели — выбираем новую
        if not nearest_plant:
            distance_to_target = math.hypot(self.x - self.tx, self.y - self.ty)
            if distance_to_target < self.speed:
                self._choose_new_target()

    def try_eat(self, plants):
        for p in plants:
            if math.hypot(self.x - p.x, self.y - p.y) < self.DIAM:
                plants.remove(p)
                self._choose_new_target()  # <--- вот это добавь
                self.energy = min(MAX_ENERGY, self.energy + ENERGY_FROM_PLANT)
                return True
        return False

    def _mutate(self, value, sigma=0.1):
        factor = 1 + random.gauss(0, sigma)
        return max(0.01, value * factor)

    def maybe_divide(self, creatures):
        if self.energy >= MAX_ENERGY * 0.8:
            self.energy -= MAX_ENERGY * 0.5
            cx = min(max(self.x + random.randint(-100, 100), self.DIAM), WORLD_WIDTH - self.DIAM)
            cy = min(max(self.y + random.randint(-100, 100), self.DIAM), WORLD_HEIGHT - self.DIAM)
            child_speed = self.speed if random.random() >= 0.5 else self._mutate(self.speed)
            child_smell = self.smell_radius if random.random() >= 0.5 else self._mutate(self.smell_radius)
            creatures.append(Creature(cx, cy, MAX_ENERGY * 0.25, child_speed, child_smell))

    def draw(self, surf, scale_x, scale_y):
        sx, sy = int(self.x * scale_x), int(self.y * scale_y)
        pygame.draw.circle(surf, (120, 120, 255), (sx, sy), max(1, int(self.smell_radius * scale_x)), 1)
        pygame.draw.circle(surf, (200, 0, 0), (sx, sy), max(1, int((self.DIAM // 2) * scale_x)))
