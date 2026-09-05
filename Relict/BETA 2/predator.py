# predator py

import pygame, random, math
from config import *

class Predator:

    # ── инициализация ────────────────────────────────────────────────────────
    def __init__(self, x=None, y=None, energy=None, genom=None):
        if genom is None:
            genom = PREDATOR_BASE_GENOM[:]

        self.genom = genom
        self.size  = genom[0]
        self.speed = genom[1]
        self.vision = genom[2]
        self.repro_threshold = genom[3]
        self.repro_share     = genom[4]

        # ── диапазон высот (в процентах) — как и у травоядных ───────────────
        min_pct = max(0, min(100, genom[5]))
        max_pct = max(0, min(100, genom[6]))
        if min_pct > max_pct:
            min_pct, max_pct = max_pct, min_pct

        self.min_y = min_pct
        self.max_y = max_pct
        y_min_abs = int((self.min_y / 100) * WORLD_HEIGHT + self.size)
        y_max_abs = int((self.max_y / 100) * WORLD_HEIGHT - self.size)

        # ─────────────────────────────────────────────────────────────────────
        self.max_energy = self.size * 4  # крупнее травоядного — и запас больше
        self.energy = energy if energy is not None else self.max_energy * 0.5
        self.alive  = True

        self.x = x if x is not None else random.randint(self.size, WORLD_WIDTH - self.size)
        self.y = y if y is not None else random.randint(y_min_abs, y_max_abs)

    # Функция отрисовки существа

    def draw(self, surf, scale_x, scale_y):
        sx = int(self.x * scale_x)
        sy = int(self.y * scale_y)
        radius = max(1, int((self.size / 2) * scale_x))
        pygame.draw.circle(surf, (255, 255, 255), (sx, sy), radius)

    # Функция движения
    def _pick_random_target(self):
        """Случайная точка в пределах vision и своего вертикального слоя."""
        y_min_abs = (self.min_y / 100) * WORLD_HEIGHT + self.size
        y_max_abs = (self.max_y / 100) * WORLD_HEIGHT - self.size

        for _ in range(10):
            angle = random.uniform(0, 2 * math.pi)
            dist  = random.uniform(self.vision * 0.5, self.vision * 2)
            tx = self.x + math.cos(angle) * dist
            ty = self.y + math.sin(angle) * dist
            if (self.size <= tx <= WORLD_WIDTH - self.size and
                y_min_abs   <= ty <= y_max_abs):
                self.tx, self.ty = tx, ty
                return
        self.tx, self.ty = self.x, self.y  # fallback

    def move(self, vegetarians):
        # поиск ближайшей добычи
        nearest, best_dist = None, self.vision
        for v in vegetarians:
            if not v.alive:
                continue
            d = math.hypot(v.x - self.x, v.y - self.y)
            if d < best_dist:
                nearest, best_dist = v, d

        if nearest:
            # преследуем, только если добыча в нашем вертикальном слое
            y_min_abs = (self.min_y / 100) * WORLD_HEIGHT
            y_max_abs = (self.max_y / 100) * WORLD_HEIGHT
            if y_min_abs <= nearest.y <= y_max_abs:
                tx, ty = nearest.x, nearest.y
            else:
                nearest = None  # чужой слой — не преследуем

        if not nearest:
            if (not hasattr(self, "tx") or
                    math.hypot(self.x - self.tx, self.y - self.ty) < self.speed):
                self._pick_random_target()
            tx, ty = self.tx, self.ty

        # вектор и шаг
        dx, dy = tx - self.x, ty - self.y
        d = math.hypot(dx, dy)
        if d != 0:
            dx *= self.speed / d
            dy *= self.speed / d

        # новое положение
        self.x = min(max(self.x + dx, self.size), WORLD_WIDTH - self.size)
        self.y = min(max(self.y + dy, self.size), WORLD_HEIGHT - self.size)

        # ♦ ограничиваем по вертикальному слою
        y_min_abs = (self.min_y / 100) * WORLD_HEIGHT + self.size
        y_max_abs = (self.max_y / 100) * WORLD_HEIGHT - self.size
        self.y = min(max(self.y, y_min_abs), y_max_abs)

        # энергозатраты
        self.energy -= (
                SIZE_ENERGY_COEF * self.size ** 1.5 +
                SPEED_ENERGY_COEF * self.speed ** 2 +
                SIGHT_ENERGY_COEF * self.vision
        )
        if self.energy <= 0:
            self.alive = False

    def try_eat(self, vegetarians) -> bool:
        """
        Съесть первую попавшуюся жертву в радиусе `self.size`.
        Возвращает True, если поели.
        """
        r2 = self.size * self.size
        cx, cy = self.x, self.y

        for v in vegetarians:
            if not v.alive:
                continue
            dx = cx - v.x
            dy = cy - v.y
            if dx * dx + dy * dy <= r2:
                self.energy = min(self.max_energy, self.energy + v.energy)
                v.alive = False
                v.energy = 0
                self._pick_random_target()
                return True

        return False

    def mutate(self, sigma=VEGETARIAN_SIGMA):
        new_genom = []
        for i, value in enumerate(self.genom):
            while True:
                gauss = random.gauss(0, sigma)
                if gauss >= -0.9:
                    break
            mutated = value * (1 + gauss)
            # Ограничиваем диапазон только для min_y и max_y (последние 2 гена)
            if i == 5 or i == 6:
                mutated = min(100, max(0, mutated))
            new_genom.append(max(0.01, mutated))
        return new_genom

    def maybe_divide(self, offspring: list):
        """Размножаемся, если остаётся ≥ 20 энергии; детей кладём в отдельный список."""
        RESERVE     = 20      # минимум, что должно остаться у родителя
        REPRO_COST  = 10      # фикс-штраф за размножение

        threshold = self.max_energy * (self.repro_threshold / 100)
        if self.energy < threshold + RESERVE:
            return                      # энергии недостаточно

        child_energy = self.energy * (self.repro_share / 100)
        self.energy -= child_energy + REPRO_COST      # родитель платит

        child_energy = min(child_energy, self.max_energy)  # не переливать
        child_genom  = self.mutate()

        offset = random.uniform(-self.size * 2, self.size * 2)
        cx = min(max(self.x + offset, self.size), WORLD_WIDTH  - self.size)
        cy = min(max(self.y + offset, self.size), WORLD_HEIGHT - self.size)

        offspring.append(
            Predator(x=cx, y=cy, energy=child_energy, genom=child_genom)
        )
