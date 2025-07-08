# predator.py
import pygame, random, math
from config import *

class Predator:
    DIAM = 24

    def __init__(self, x=None, y=None,
                 energy=MAX_ENERGY * 0.5,
                 speed=BASE_SPEED * 1.25,
                 smell_radius=BASE_SMELL * 1.3):
        self.x = x if x is not None else random.randint(self.DIAM, WORLD_WIDTH  - self.DIAM)
        self.y = y if y is not None else random.randint(self.DIAM, WORLD_HEIGHT - self.DIAM)
        self.energy       = energy
        self.speed        = speed
        self.smell_radius = smell_radius
        self._choose_new_target()

    # ---------- утилиты ----------
    def _choose_new_target(self):
        self.tx = random.randint(self.DIAM, WORLD_WIDTH  - self.DIAM)
        self.ty = random.randint(self.DIAM, WORLD_HEIGHT - self.DIAM)

    def _vector_towards(self, tx, ty):
        dx, dy = tx - self.x, ty - self.y
        dist   = math.hypot(dx, dy)
        return (0, 0) if dist == 0 else (dx * self.speed / dist,
                                         dy * self.speed / dist)

    def _nearest_prey(self, creatures):
        nearest, best_d = None, self.smell_radius
        for c in creatures:
            d = math.hypot(self.x - c.x, self.y - c.y)
            if d < best_d:
                nearest, best_d = c, d
        return nearest
    # ----------------------------------

    def move(self, creatures):
        prey = self._nearest_prey(creatures)
        dx, dy = (self._vector_towards(prey.x, prey.y) if prey
                  else self._vector_towards(self.tx, self.ty))

        self.x = min(max(self.x + dx, self.DIAM), WORLD_WIDTH  - self.DIAM)
        self.y = min(max(self.y + dy, self.DIAM), WORLD_HEIGHT - self.DIAM)

        self.energy -= (K_MOVE / 1.5 * self.speed**2) + (K_SMELL / 1.25 * self.smell_radius) + BASAL_METABOLISM

        if (not prey) and math.hypot(self.x - self.tx, self.y - self.ty) < self.speed:
            self._choose_new_target()

    def try_eat(self, creatures):
        for c in creatures:
            if math.hypot(self.x - c.x, self.y - c.y) < self.DIAM:
                creatures.remove(c)
                self.energy = min(MAX_ENERGY * 2, self.energy + c.energy)
                self._choose_new_target()
                return True
        return False

    # ---------- размножение ----------
    def _mutate(self, value, sigma=0.5):
        return max(0.01, value * (1 + random.gauss(0, sigma)))

    def maybe_divide(self, predators):
        if self.energy >= MAX_ENERGY * 0.8 and random.random() > 0.5:
            self.energy -= MAX_ENERGY * 0.30
            cx = min(max(self.x + random.randint(-300, 300), self.DIAM), WORLD_WIDTH  - self.DIAM)
            cy = min(max(self.y + random.randint(-300, 300), self.DIAM), WORLD_HEIGHT - self.DIAM)
            child_speed = self.speed        if random.random() > 0.6 else self._mutate(self.speed)
            child_smell = self.smell_radius if random.random() > 0.6 else self._mutate(self.smell_radius)
            predators.append(Predator(cx, cy, MAX_ENERGY * 0.25, child_speed, child_smell))
            self._choose_new_target()
    # ----------------------------------

    def draw(self, surf, scale_x, scale_y):
        sx, sy = int(self.x * scale_x), int(self.y * scale_y)
        pygame.draw.circle(surf, (255, 255, 255), (sx, sy),
                           max(1, int((self.DIAM // 2) * scale_x)))
        pygame.draw.circle(surf, (0, 120, 255), (sx, sy),
                           max(1, int(self.smell_radius * scale_x)), 1)
