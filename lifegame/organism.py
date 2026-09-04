# organism.py — общая база для всех существ.
#
# Здесь живёт всё, что одинаково у травоядного и хищника: раскладка генома по
# именованным признакам, слой обитания, шаг движения, расход энергии, мутация и
# деление. Наследники описывают только своё поведение (куда идти и что есть).

import math
import random

from .config import (
    GENES, GENE_INDEX, MUTATION_SIGMA, MUTATION_SIGMA_PCT,
    MAX_ENERGY_PER_SIZE, REPRO_RESERVE_FRAC, REPRO_COST_FRAC,
    SIZE_ENERGY_COEF, SPEED_ENERGY_COEF, SIGHT_ENERGY_COEF, BASAL_METABOLISM,
    WORLD_WIDTH, WORLD_HEIGHT,
)


def dist_point_to_segment(px, py, ax, ay, bx, by):
    """Расстояние от точки до отрезка [a, b].

    Нужно, чтобы быстрая особь не «перепрыгивала» еду между тиками: проверяем
    весь путь за тик, а не только его конец.
    """
    dx, dy = bx - ax, by - ay
    seg2 = dx * dx + dy * dy
    if seg2 == 0.0:
        return math.hypot(px - ax, py - ay)
    t = ((px - ax) * dx + (py - ay) * dy) / seg2
    t = min(1.0, max(0.0, t))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


class Organism:
    BASE_GENOM = [g[1] for g in GENES]
    COLOR = (255, 255, 255)

    # ── создание ─────────────────────────────────────────────────────────────
    def __init__(self, x=None, y=None, energy=None, genom=None):
        genom = list(genom) if genom is not None else list(self.BASE_GENOM)
        self.genom = self._clamp_genom(genom)

        # раскладываем геном по именам из таблицы GENES — никаких genom[5]
        for name, i in GENE_INDEX.items():
            setattr(self, name, self.genom[i])

        self.max_energy = self.size * MAX_ENERGY_PER_SIZE
        self.energy = energy if energy is not None else self.max_energy * 0.5
        self.alive = True

        y_min, y_max = self.layer_bounds()
        self.x = x if x is not None else random.uniform(self.radius, WORLD_WIDTH - self.radius)
        self.y = y if y is not None else random.uniform(y_min, y_max)
        self.y = min(max(self.y, y_min), y_max)

        self.tx, self.ty = self.x, self.y
        self._pick_random_target()

    @staticmethod
    def _clamp_genom(genom):
        """Загнать геном в допустимые границы и гарантировать min_y <= max_y."""
        out = []
        for value, (_name, _base, _kind, lo, hi) in zip(genom, GENES):
            if lo is not None:
                value = max(lo, value)
            if hi is not None:
                value = min(hi, value)
            out.append(float(value))
        i_min, i_max = GENE_INDEX["min_y"], GENE_INDEX["max_y"]
        if out[i_min] > out[i_max]:
            out[i_min], out[i_max] = out[i_max], out[i_min]
        return out

    # ── геометрия и слой обитания ────────────────────────────────────────────
    @property
    def radius(self):
        """Радиус тела. Ровно то, что рисуется, — и ровно то, чем едят."""
        return self.size / 2

    def layer_bounds(self):
        """Абсолютные границы вертикального слоя (в мировых координатах)."""
        y_min = (self.min_y / 100.0) * WORLD_HEIGHT + self.radius
        y_max = (self.max_y / 100.0) * WORLD_HEIGHT - self.radius
        if y_min > y_max:                      # слой тоньше тела — схлопываем в точку
            y_min = y_max = (y_min + y_max) / 2
        y_min = min(max(y_min, self.radius), WORLD_HEIGHT - self.radius)
        y_max = min(max(y_max, self.radius), WORLD_HEIGHT - self.radius)
        return y_min, y_max

    def in_my_layer(self, y):
        """Доступна ли эта высота. Одна проверка и для поиска цели, и для еды —
        раньше move() уважал слой, а try_eat() нет, и отбор по биомам размывался."""
        y_min = (self.min_y / 100.0) * WORLD_HEIGHT
        y_max = (self.max_y / 100.0) * WORLD_HEIGHT
        return y_min <= y <= y_max

    # ── движение ─────────────────────────────────────────────────────────────
    def _pick_random_target(self):
        """Случайная точка в пределах vision и своего слоя."""
        y_min, y_max = self.layer_bounds()
        for _ in range(10):
            angle = random.uniform(0, 2 * math.pi)
            dist = random.uniform(self.vision * 0.5, self.vision * 2)
            tx = self.x + math.cos(angle) * dist
            ty = self.y + math.sin(angle) * dist
            if self.radius <= tx <= WORLD_WIDTH - self.radius and y_min <= ty <= y_max:
                self.tx, self.ty = tx, ty
                return
        self.tx, self.ty = self.x, self.y

    def step_towards(self, tx, ty, away=False):
        """Сделать шаг к цели (или от неё). Возвращает прежнюю позицию —
        по отрезку [прежняя, новая] потом проверяется поедание.

        Шаг не превышает оставшегося расстояния до цели, поэтому существо не
        дрожит вокруг точки и не проскакивает её.
        """
        old_x, old_y = self.x, self.y
        dx, dy = tx - self.x, ty - self.y
        if away:
            dx, dy = -dx, -dy
        dist = math.hypot(dx, dy)
        if dist > 0:
            step = self.speed if away else min(self.speed, dist)
            dx *= step / dist
            dy *= step / dist

        y_min, y_max = self.layer_bounds()
        self.x = min(max(self.x + dx, self.radius), WORLD_WIDTH - self.radius)
        self.y = min(max(self.y + dy, y_min), y_max)
        return old_x, old_y

    def pay_energy(self):
        """Списать метаболизм за тик. Единственное место, где выставляется смерть."""
        self.energy -= (
            SIZE_ENERGY_COEF * self.size ** 1.5 +
            SPEED_ENERGY_COEF * self.speed ** 2 +
            SIGHT_ENERGY_COEF * self.vision +
            BASAL_METABOLISM
        )
        if self.energy <= 0:
            self.energy = 0.0
            self.alive = False

    def gain(self, amount):
        self.energy = min(self.max_energy, self.energy + amount)

    # ── размножение ──────────────────────────────────────────────────────────
    def mutate(self):
        """Мутация с учётом типа гена из таблицы GENES."""
        new_genom = []
        for value, (_name, _base, kind, lo, hi) in zip(self.genom, GENES):
            if kind == "mul":
                gauss = max(-0.9, random.gauss(0, MUTATION_SIGMA))
                value = value * (1 + gauss)
            else:                       # "add" — процентные гены
                value = value + random.gauss(0, MUTATION_SIGMA_PCT)
            if lo is not None:
                value = max(lo, value)
            if hi is not None:
                value = min(hi, value)
            new_genom.append(value)
        return self._clamp_genom(new_genom)

    def maybe_divide(self, offspring):
        """Поделиться, если хватает энергии. Ребёнок кладётся в буфер, а не в
        список, по которому идёт текущий тик, — иначе он ходит в день рождения."""
        reserve = self.max_energy * REPRO_RESERVE_FRAC
        cost = self.max_energy * REPRO_COST_FRAC
        threshold = self.max_energy * (self.repro_threshold / 100.0)

        child_energy = self.energy * (self.repro_share / 100.0)
        if self.energy < threshold or self.energy - child_energy - cost < reserve:
            return None

        self.energy -= child_energy + cost

        child_genom = self.mutate()
        offset_x = random.uniform(-self.size * 2, self.size * 2)
        offset_y = random.uniform(-self.size * 2, self.size * 2)
        cx = min(max(self.x + offset_x, self.radius), WORLD_WIDTH - self.radius)
        cy = self.y + offset_y

        child = type(self)(x=cx, y=cy, energy=child_energy, genom=child_genom)
        child.energy = min(child.energy, child.max_energy)
        offspring.append(child)
        return child

    # ── отрисовка ────────────────────────────────────────────────────────────
    def draw(self, surf, scale_x, scale_y):
        import pygame
        pygame.draw.circle(
            surf, self.COLOR,
            (int(self.x * scale_x), int(self.y * scale_y)),
            max(1, int(self.radius * scale_x)),
        )
