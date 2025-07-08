# creature.py
import pygame, random, math
from config import *

class Creature:
    DIAM = 20

    def __init__(self, x=None, y=None,
                 energy=MAX_ENERGY * 0.5,
                 speed=BASE_SPEED,
                 smell_radius=BASE_SMELL):
        self.x = x if x is not None else random.randint(self.DIAM, WORLD_WIDTH  - self.DIAM)
        self.y = y if y is not None else random.randint(self.DIAM, WORLD_HEIGHT - self.DIAM)
        self.energy       = energy
        self.speed        = speed
        self.smell_radius = smell_radius
        self.flee_ticks   = 0
        self._choose_new_target()

    # ---------- утилиты ----------
    def _choose_new_target(self):
        self.tx = random.randint(
            max(int(self.x) - WANDER_RADIUS, 0),
            min(int(self.x) + WANDER_RADIUS, WORLD_WIDTH - 1)
        )
        self.ty = random.randint(
            max(int(self.y) - WANDER_RADIUS, 0),
            min(int(self.y) + WANDER_RADIUS, WORLD_HEIGHT - 1)
        )
    def _vector_towards(self, tx, ty):
        dx, dy = tx - self.x, ty - self.y
        dist   = math.hypot(dx, dy)
        return (0, 0) if dist == 0 else (dx * self.speed / dist,
                                         dy * self.speed / dist)

    def _nearest_plant_in_smell(self, plants):
        nearest, best_d = None, self.smell_radius
        for p in plants:
            d = math.hypot(self.x - p.x, self.y - p.y)
            if d < best_d:
                nearest, best_d = p, d
        return nearest
    # --------------------------------

    def move(self, plants, predators):
        nearest_plant = None
        nearest_pred, best_pred_d = None, self.smell_radius
        for pr in predators:
            d = math.hypot(self.x - pr.x, self.y - pr.y)
            if d < best_pred_d:
                nearest_pred, best_pred_d = pr, d

        fleeing = False
        if self.flee_ticks > 0:
            fleeing = True
            self.flee_ticks -= 1
        if nearest_pred and best_pred_d < self.smell_radius / 2:
            self.flee_ticks = FLEE_TICKS
            fleeing = True

        if fleeing:
            if nearest_pred:
                dx, dy = self._vector_towards(nearest_pred.x, nearest_pred.y)
                dx, dy = -dx, -dy
            else:
                self._choose_new_target()
                dx, dy = self._vector_towards(self.tx, self.ty)
        else:
            nearest_plant = self._nearest_plant_in_smell(plants)
            if nearest_plant:
                dx, dy = self._vector_towards(nearest_plant.x, nearest_plant.y)
            else:
                dx, dy = self._vector_towards(self.tx, self.ty)

        self.x = min(max(self.x + dx, self.DIAM), WORLD_WIDTH  - self.DIAM)
        self.y = min(max(self.y + dy, self.DIAM), WORLD_HEIGHT - self.DIAM)

        move_cost  = K_MOVE  * self.speed**2
        smell_cost = K_SMELL * self.smell_radius
        self.energy -= move_cost + smell_cost + BASAL_METABOLISM

        if (not nearest_plant) and (not fleeing):
            if math.hypot(self.x - self.tx, self.y - self.ty) < self.speed:
                self._choose_new_target()

    def try_eat(self, plants):
        for p in plants:
            if math.hypot(self.x - p.x, self.y - p.y) < self.DIAM:
                plants.remove(p)
                self._choose_new_target()
                self.energy = min(MAX_ENERGY * 3, self.energy + ENERGY_FROM_PLANT)
                return True
        return False

    # ---------- размножение ----------
    def _mutate(self, value, sigma=0.5):
        return max(0.01, value * (1 + random.gauss(0, sigma)))

    def maybe_divide(self, creatures):
        if self.energy >= MAX_ENERGY * 0.8 and random.random() > 0.5:
            self.energy -= MAX_ENERGY * 0.3
            cx = min(max(self.x + random.randint(-300, 300), self.DIAM), WORLD_WIDTH  - self.DIAM)
            cy = min(max(self.y + random.randint(-300, 300), self.DIAM), WORLD_HEIGHT - self.DIAM)
            child_speed = self.speed        if random.random() > 0.6 else self._mutate(self.speed)
            child_smell = self.smell_radius if random.random() > 0.6 else self._mutate(self.smell_radius)
            creatures.append(Creature(cx, cy, MAX_ENERGY * 0.25, child_speed, child_smell))
            self._choose_new_target()
    # ----------------------------------

    def draw(self, surf, scale_x, scale_y):
        sx, sy = int(self.x * scale_x), int(self.y * scale_y)
        pygame.draw.circle(surf, (120, 120, 255), (sx, sy),
                           max(1, int(self.smell_radius * scale_x)), 1)
        pygame.draw.circle(surf, (200, 100, 0), (sx, sy),
                           max(1, int((self.DIAM // 2) * scale_x)))
