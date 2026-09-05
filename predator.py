# predator py

import pygame, random, math
from config import *


class Predator:
    DIAM = PREDATOR_DIAM

    def __init__(self, x=None, y=None,
                 energy=None,
                 speed=PREDATOR_BASE_SPEED,
                 vision=PREDATOR_BASE_VISION):
        self.x = x if x is not None else random.randint(self.DIAM, WORLD_WIDTH  - self.DIAM)
        self.y = y if y is not None else random.randint(self.DIAM, WORLD_HEIGHT - self.DIAM)
        self.max_energy = PREDATOR_MAX_ENERGY
        self.energy = energy if energy is not None else self.max_energy * 0.5
        self.speed  = speed
        self.vision = vision
        self.alive  = True
        self._choose_new_target()

    # ---------- утилиты ----------
    def _choose_new_target(self):
        half = WANDER_RADIUS  # половина стороны квадрата

        # --- горизонтальная полоса ---
        xmin = self.x - half
        xmax = self.x + half
        if xmin < 0:  # залезли за левый край
            xmax += -xmin  # двигаем полосу вправо
            xmin = 0
        if xmax > WORLD_WIDTH - 1:  # залезли за правый край
            xmin -= xmax - (WORLD_WIDTH - 1)  # двигаем влево
            xmax = WORLD_WIDTH - 1
            xmin = max(xmin, 0)  # вдруг ушли левее нуля

        # --- вертикальная полоса ---
        ymin = self.y - half
        ymax = self.y + half
        if ymin < 0:
            ymax += -ymin
            ymin = 0
        if ymax > WORLD_HEIGHT - 1:
            ymin -= ymax - (WORLD_HEIGHT - 1)
            ymax = WORLD_HEIGHT - 1
            ymin = max(ymin, 0)

        # используем float-цели — так плавнее
        self.tx = random.uniform(xmin, xmax)
        self.ty = random.uniform(ymin, ymax)

    def _vector_towards(self, tx, ty):
        dx, dy = tx - self.x, ty - self.y
        dist   = math.hypot(dx, dy)
        return (0, 0) if dist == 0 else (dx * self.speed / dist,
                                         dy * self.speed / dist)

    def _nearest_prey(self, vegetarians):
        # сравниваем квадраты расстояний — то же самое, но без вызова hypot
        x, y = self.x, self.y
        nearest, best_d2 = None, self.vision * self.vision
        for v in vegetarians:
            dx = x - v.x
            dy = y - v.y
            d2 = dx * dx + dy * dy
            if d2 < best_d2:
                nearest, best_d2 = v, d2
        return nearest
    # ----------------------------------

    def move(self, vegetarians):
        prey = self._nearest_prey(vegetarians)
        dx, dy = (self._vector_towards(prey.x, prey.y) if prey
                  else self._vector_towards(self.tx, self.ty))

        self.x = min(max(self.x + dx, self.DIAM), WORLD_WIDTH  - self.DIAM)
        self.y = min(max(self.y + dy, self.DIAM), WORLD_HEIGHT - self.DIAM)

        self.energy -= (
                SIZE_ENERGY_COEF  * self.DIAM   ** SIZE_ENERGY_POWER +
                SPEED_ENERGY_COEF * self.speed  ** SPEED_ENERGY_POWER +
                SIGHT_ENERGY_COEF * self.vision ** SIGHT_ENERGY_POWER
        )
        if self.energy <= 0:
            self.alive = False

        if (not prey) and math.hypot(self.x - self.tx, self.y - self.ty) < self.speed:
            self._choose_new_target()

    def try_eat(self, vegetarians):
        x, y = self.x, self.y
        r2 = self.DIAM * self.DIAM
        for v in vegetarians:
            if not v.alive:
                continue
            dx = x - v.x
            dy = y - v.y
            if dx * dx + dy * dy < r2:
                self.energy = min(self.max_energy, self.energy + v.energy)
                v.alive = False
                v.energy = 0
                self._choose_new_target()
                return True
        return False

    # ---------- размножение ----------
    def _mutate(self, value, sigma=PREDATOR_SIGMA):
        return max(0.01, value * (1 + random.gauss(0, sigma)))

    def maybe_divide(self, predators):
        if (self.energy >= self.max_energy * PREDATOR_REPRO_THRESHOLD
                and random.random() > 1 - PREDATOR_DIVIDE_CHANCE):
            self.energy -= self.max_energy * PREDATOR_REPRO_COST
            cx = min(max(self.x + random.randint(-300, 300), self.DIAM), WORLD_WIDTH  - self.DIAM)
            cy = min(max(self.y + random.randint(-300, 300), self.DIAM), WORLD_HEIGHT - self.DIAM)
            child_speed  = self.speed  if random.random() > 0.6 else self._mutate(self.speed)
            child_vision = self.vision if random.random() > 0.6 else self._mutate(self.vision)
            predators.append(Predator(cx, cy, self.max_energy * PREDATOR_CHILD_ENERGY,
                                      child_speed, child_vision))
            self._choose_new_target()
    # ----------------------------------

    def draw(self, surf, scale_x, scale_y):
        sx, sy = int(self.x * scale_x), int(self.y * scale_y)
        pygame.draw.circle(surf, (255, 255, 255), (sx, sy),
                           max(1, int((self.DIAM // 2) * scale_x)))
