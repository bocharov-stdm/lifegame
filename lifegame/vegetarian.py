# vegetarian.py — травоядное.

import math

from .config import (ENERGY_FROM_PLANT, PLANT_RADIUS, FLEE_TICKS,
                    FLEE_TRIGGER_FRAC, VEGETARIAN_BASE_GENOM)
from .organism import Organism, dist_point_to_segment


class Vegetarian(Organism):
    BASE_GENOM = VEGETARIAN_BASE_GENOM
    COLOR = (255, 100, 255)

    def __init__(self, x=None, y=None, energy=None, genom=None):
        super().__init__(x=x, y=y, energy=energy, genom=genom)
        self.flee_ticks = 0

    # ── один тик жизни ───────────────────────────────────────────────────────
    def update(self, plant_grid, predator_grid=None):
        """Убежать / поесть / побродить, затем заплатить метаболизм."""
        old_x, old_y = self._act(plant_grid, predator_grid)
        self.try_eat(plant_grid, old_x, old_y)
        self.pay_energy()

    def _act(self, plant_grid, predator_grid):
        predator = None
        if predator_grid is not None:
            predator = predator_grid.nearest(self.x, self.y, self.vision)

        if self.flee_ticks > 0:
            self.flee_ticks -= 1
        if predator is not None:
            d = math.hypot(self.x - predator.x, self.y - predator.y)
            if d < self.vision * FLEE_TRIGGER_FRAC:
                self.flee_ticks = FLEE_TICKS

        if self.flee_ticks > 0 and predator is not None:
            return self.step_towards(predator.x, predator.y, away=True)

        plant = plant_grid.nearest(
            self.x, self.y, self.vision,
            predicate=lambda p: p.alive and self.in_my_layer(p.y),
        )
        if plant is not None:
            return self.step_towards(plant.x, plant.y)

        if math.hypot(self.x - self.tx, self.y - self.ty) < self.speed:
            self._pick_random_target()
        return self.step_towards(self.tx, self.ty)

    # ── питание ──────────────────────────────────────────────────────────────
    def try_eat(self, plant_grid, old_x, old_y):
        """Съесть растения, задетые телом на всём пути за этот тик.

        Радиус поедания — радиус тела (то, что рисуется) плюс радиус растения,
        а проверка идёт по отрезку пройденного пути: иначе быстрая особь
        перепрыгивала бы еду между тиками и скорость наказывалась бы дважды.
        """
        reach = self.radius + PLANT_RADIUS
        travelled = math.hypot(self.x - old_x, self.y - old_y)
        # ищем вокруг середины пути, с запасом на половину пути и радиус тела
        mid_x, mid_y = (self.x + old_x) / 2, (self.y + old_y) / 2
        candidates = plant_grid.query(mid_x, mid_y, reach + travelled / 2 + 1)

        eaten = 0
        for p in candidates:
            if not p.alive or not self.in_my_layer(p.y):
                continue
            if dist_point_to_segment(p.x, p.y, old_x, old_y, self.x, self.y) <= reach:
                p.alive = False          # физическое удаление — один раз за тик в main
                eaten += 1

        if eaten:
            self.gain(ENERGY_FROM_PLANT * eaten)
            self._pick_random_target()
        return eaten > 0
