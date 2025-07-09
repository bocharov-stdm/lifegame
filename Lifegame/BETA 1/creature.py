# creature.py
import pygame, random, math
from config import *

class Creature:
    """Травоядное существо"""
    DIAM = 60  # диаметр коллизии/отрисовки в мировых координатах

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

    # ────────────────────── ВСПОМОГАТЕЛЬНЫЕ ────────────────────────────────────
    def _choose_new_target(self):
        """Выбрать случайную точку в квадрате WANDER_RADIUS×WANDER_RADIUS"""
        half = WANDER_RADIUS
        xmin = max(0,               self.x - half)
        xmax = min(WORLD_WIDTH - 1, self.x + half)
        ymin = max(0,               self.y - half)
        ymax = min(WORLD_HEIGHT - 1, self.y + half)
        # используем float-цели — движение выходит плавнее
        self.tx = random.uniform(xmin, xmax)
        self.ty = random.uniform(ymin, ymax)

    def _vector_towards(self, tx, ty):
        dx, dy = tx - self.x, ty - self.y
        dist   = math.hypot(dx, dy)
        return (0, 0) if dist == 0 else (dx * self.speed / dist,
                                         dy * self.speed / dist)

    def _nearby(self, items, radius):
        """Вернуть подсписок `items`, находящихся не дальше `radius`."""
        r2  = radius * radius
        cx, cy = self.x, self.y
        return [obj for obj in items
                if (cx - obj.x) ** 2 + (cy - obj.y) ** 2 <= r2]
    # ───────────────────────────────────────────────────────────────────────────

    def move(self, plants, predators):
        """Основное поведение: поиск еды, бегство от хищника или блуждание."""
        # --- ищем ближайшего хищника только среди близких ---
        nearest_pred, best_pred_d = None, self.smell_radius
        for pr in self._nearby(predators, self.smell_radius):
            d = math.hypot(self.x - pr.x, self.y - pr.y)
            if d < best_pred_d:
                nearest_pred, best_pred_d = pr, d

        fleeing = False
        if self.flee_ticks > 0:
            fleeing = True
            self.flee_ticks -= 1
        if nearest_pred and best_pred_d < self.smell_radius / 3:
            self.flee_ticks = FLEE_TICKS
            fleeing = True

        if fleeing:
            # вектор от хищника
            dx, dy = self._vector_towards(nearest_pred.x, nearest_pred.y) if nearest_pred else (0, 0)
            dx, dy = -dx, -dy
        else:
            # --- ищем ближайшее растение среди близких ---
            nearest_plant, best_d = None, self.smell_radius
            for p in self._nearby(plants, self.smell_radius):
                d = math.hypot(self.x - p.x, self.y - p.y)
                if d < best_d:
                    nearest_plant, best_d = p, d

            if nearest_plant:
                dx, dy = self._vector_towards(nearest_plant.x, nearest_plant.y)
            else:
                dx, dy = self._vector_towards(self.tx, self.ty)

        # --- перемещаемся и считаем затраты ---
        self.x = min(max(self.x + dx, self.DIAM), WORLD_WIDTH  - self.DIAM)
        self.y = min(max(self.y + dy, self.DIAM), WORLD_HEIGHT - self.DIAM)

        self.energy -= (
            K_MOVE  * self.speed ** 2 +
            K_SMELL * self.smell_radius +
            BASAL_METABOLISM
        )

        # если дошёл до цели — берём новую
        if (not fleeing) and math.hypot(self.x - self.tx, self.y - self.ty) < self.speed:
            self._choose_new_target()

    # ────────────────────── ПИТАНИЕ ────────────────────────────────────────────
    def try_eat(self, plants):
        """Пытаемся съесть все растения в радиусе DIAM (быстрое удаление)."""
        eaten = []
        r2 = self.DIAM * self.DIAM
        cx, cy = self.x, self.y
        for p in self._nearby(plants, self.DIAM):
            # точная проверка квадратом расстояния
            if (cx - p.x) ** 2 + (cy - p.y) ** 2 <= r2:
                eaten.append(p)

        if not eaten:
            return False

        for p in eaten:
            plants.remove(p)

        self.energy = min(MAX_ENERGY * 3, self.energy + ENERGY_FROM_PLANT * len(eaten))
        self._choose_new_target()
        return True
    # ───────────────────────────────────────────────────────────────────────────

    # ────────────────────── РАЗМНОЖЕНИЕ ────────────────────────────────────────
    def _mutate(self, value, sigma=0.3):
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
    # ───────────────────────────────────────────────────────────────────────────

    def draw(self, surf, scale_x, scale_y):
        sx, sy = int(self.x * scale_x), int(self.y * scale_y)
        pygame.draw.circle(
            surf, (200, 100, 0),
            (sx, sy),
            max(1, int((self.DIAM // 2) * scale_x))
        )
