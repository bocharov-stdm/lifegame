# predator py

import random, math
from .config import *
from .rules  import DEFAULT_RULES


class Predator:
    DIAM = PREDATOR_DIAM

    def __init__(self, x=None, y=None,
                 energy=None,
                 speed=PREDATOR_BASE_SPEED,
                 vision=PREDATOR_BASE_VISION,
                 rules=None):
        # правила мира (цена статов, запас энергии, плодовитость); дети наследуют их
        self.rules = rules = rules or DEFAULT_RULES
        self.x = x if x is not None else random.randint(self.DIAM, WORLD_WIDTH  - self.DIAM)
        self.y = y if y is not None else random.randint(self.DIAM, WORLD_HEIGHT - self.DIAM)
        self.max_energy = rules.predator_max_energy
        self.energy = energy if energy is not None else self.max_energy * 0.5
        self.speed  = speed
        self.vision = vision
        self.alive  = True
        # скорость и зрение не меняются всю жизнь — расход считаем один раз
        self.upkeep = rules.upkeep(self.DIAM, self.speed, self.vision)
        self._choose_new_target()

    # ---------- утилиты ----------
    def _choose_new_target(self):
        # Цель — случайная точка квадрата со стороной 2*WANDER_RADIUS вокруг себя.
        # Квадрат сдвигается внутрь тех же границ, в которых держится сам хищник
        # (DIAM от края). Раньше цель бралась до самой стены, и если она ложилась
        # ближе DIAM - speed к краю, хищник упирался в стену и стоял, пока мимо не
        # пройдёт добыча: один так простоял 426 тиков и умер от голода.
        # float-цели — так плавнее.
        self.tx = random.uniform(*self._wander_span(self.x, WORLD_WIDTH))
        self.ty = random.uniform(*self._wander_span(self.y, WORLD_HEIGHT))

    def _wander_span(self, pos, world_size):
        """Отрезок длиной 2*WANDER_RADIUS вокруг pos, сдвинутый внутрь мира."""
        lo, hi = self.DIAM, world_size - self.DIAM
        a, b = pos - WANDER_RADIUS, pos + WANDER_RADIUS
        if a < lo:                      # залезли за левый/верхний край
            b += lo - a                 # двигаем отрезок внутрь
            a = lo
        if b > hi:                      # залезли за правый/нижний край
            a = max(a - (b - hi), lo)   # двигаем внутрь, но не за другой край
            b = hi
        return a, b

    def _vector_towards(self, tx, ty):
        dx, dy = tx - self.x, ty - self.y
        dist   = math.hypot(dx, dy)
        return (0, 0) if dist == 0 else (dx * self.speed / dist,
                                         dy * self.speed / dist)

    def _nearest_prey(self, vegetarians):
        # Видим добычу по краю её тела, а не по центру: крупное заметно издалека.
        # Ближайшая — по расстоянию между центрами; квадраты — чтобы без hypot.
        x, y, vision = self.x, self.y, self.vision
        nearest, best_d2 = None, math.inf
        for v in vegetarians:
            if not v.alive:             # съеден другим хищником в этом же тике
                continue
            dx = x - v.x
            dy = y - v.y
            d2 = dx * dx + dy * dy
            reach = vision + v.half
            if d2 < best_d2 and d2 < reach * reach:
                nearest, best_d2 = v, d2
        return nearest
    # ----------------------------------

    def hungry(self):
        return self.energy < self.max_energy * PREDATOR_HUNGRY

    def move(self, vegetarians):
        prey = self._nearest_prey(vegetarians) if self.hungry() else None
        energy_cost = self.upkeep
        if prey:
            dx, dy = prey.x - self.x, prey.y - self.y
            dist = math.hypot(dx, dy)
            step = self.speed
            # Рывок: вблизи хищник догоняет и того, кто быстрее его на дистанции.
            # Он стоит энергии, поэтому гнаться рывком бесконечно нельзя.
            if dist - prey.half - self.DIAM / 2 < PREDATOR_SPRINT_RANGE:
                step *= PREDATOR_SPRINT_MULT
                energy_cost += PREDATOR_SPRINT_COST
            step = min(step, dist)          # не проскакивать добычу насквозь
            if dist != 0:
                dx, dy = dx * step / dist, dy * step / dist
        else:
            dx, dy = self._vector_towards(self.tx, self.ty)

        self.x = min(max(self.x + dx, self.DIAM), WORLD_WIDTH  - self.DIAM)
        self.y = min(max(self.y + dy, self.DIAM), WORLD_HEIGHT - self.DIAM)

        self.energy -= energy_cost
        if self.energy <= 0:
            self.alive = False

        if (not prey) and math.hypot(self.x - self.tx, self.y - self.ty) < self.speed:
            self._choose_new_target()

    def try_eat(self, vegetarians):
        # поймал, если тела соприкоснулись: крупную добычу схватить проще
        if not self.hungry():
            return False
        x, y = self.x, self.y
        own = self.DIAM / 2
        for v in vegetarians:
            if not v.alive:
                continue
            dx = x - v.x
            dy = y - v.y
            reach = own + v.half
            if dx * dx + dy * dy < reach * reach:
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
                and random.random() > 1 - self.rules.predator_divide_chance):
            self.energy -= self.max_energy * PREDATOR_REPRO_COST
            cx = min(max(self.x + random.randint(-300, 300), self.DIAM), WORLD_WIDTH  - self.DIAM)
            cy = min(max(self.y + random.randint(-300, 300), self.DIAM), WORLD_HEIGHT - self.DIAM)
            child_speed  = self.speed  if random.random() > 0.6 else self._mutate(self.speed)
            child_vision = self.vision if random.random() > 0.6 else self._mutate(self.vision)
            predators.append(Predator(cx, cy, self.max_energy * PREDATOR_CHILD_ENERGY,
                                      child_speed, child_vision, rules=self.rules))
            self._choose_new_target()
    # ----------------------------------
