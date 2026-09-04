# predator.py — хищник.
#
# Тот же геном и та же формула метаболизма, что у травоядного: обе стороны
# платят в одной валюте, поэтому получается коэволюционная гонка, а не
# одностороннее преимущество.

import math

from config import PREDATOR_BASE_GENOM, PREDATION_EFFICIENCY
from organism import Organism, dist_point_to_segment


class Predator(Organism):
    BASE_GENOM = PREDATOR_BASE_GENOM
    COLOR = (255, 255, 255)

    def update(self, prey_grid):
        old_x, old_y = self._act(prey_grid)
        self.try_eat(prey_grid, old_x, old_y)
        self.pay_energy()

    def _act(self, prey_grid):
        prey = prey_grid.nearest(
            self.x, self.y, self.vision,
            predicate=lambda v: v.alive and self.in_my_layer(v.y),
        )
        if prey is not None:
            return self.step_towards(prey.x, prey.y)

        if math.hypot(self.x - self.tx, self.y - self.ty) < self.speed:
            self._pick_random_target()
        return self.step_towards(self.tx, self.ty)

    def try_eat(self, prey_grid, old_x, old_y):
        """Съесть первую задетую жертву.

        Жертва только помечается мёртвой; из списков её вычищает main один раз
        за тик. Так мы не изменяем список, по которому кто-то в этот момент идёт.
        """
        travelled = math.hypot(self.x - old_x, self.y - old_y)
        mid_x, mid_y = (self.x + old_x) / 2, (self.y + old_y) / 2

        for prey in prey_grid.query(mid_x, mid_y, self.radius + travelled / 2 + 200):
            if not prey.alive:
                continue
            reach = self.radius + prey.radius
            if dist_point_to_segment(prey.x, prey.y, old_x, old_y, self.x, self.y) <= reach:
                prey.alive = False
                self.gain(prey.energy * PREDATION_EFFICIENCY)
                self._pick_random_target()
                return True
        return False
