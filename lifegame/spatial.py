# spatial.py — равномерная сетка для поиска соседей.
#
# До неё каждый поиск еды был перебором всех растений каждым существом:
# O(травоядные × растения) на тик — главный потолок производительности.

import math


class SpatialHash:
    """Сетка ячеек фиксированного размера. Перестраивается раз в тик."""

    def __init__(self, cell_size):
        if cell_size <= 0:
            raise ValueError("cell_size must be positive")
        self.cell_size = float(cell_size)
        self.cells = {}

    def _key(self, x, y):
        return (int(math.floor(x / self.cell_size)),
                int(math.floor(y / self.cell_size)))

    def rebuild(self, items):
        """Заново разложить объекты по ячейкам (у объекта нужны .x и .y)."""
        cells = {}
        for item in items:
            cells.setdefault(self._key(item.x, item.y), []).append(item)
        self.cells = cells

    def query(self, x, y, radius):
        """Объекты в круге (x, y, radius). Точная проверка, без ложных срабатываний."""
        if radius <= 0:
            return []
        r2 = radius * radius
        cx0, cy0 = self._key(x - radius, y - radius)
        cx1, cy1 = self._key(x + radius, y + radius)

        found = []
        for cx in range(cx0, cx1 + 1):
            for cy in range(cy0, cy1 + 1):
                bucket = self.cells.get((cx, cy))
                if not bucket:
                    continue
                for item in bucket:
                    dx = item.x - x
                    dy = item.y - y
                    if dx * dx + dy * dy <= r2:
                        found.append(item)
        return found

    def nearest(self, x, y, radius, predicate=None):
        """Ближайший объект в радиусе, при желании — с фильтром."""
        best, best_d2 = None, radius * radius
        for item in self.query(x, y, radius):
            if predicate is not None and not predicate(item):
                continue
            d2 = (item.x - x) ** 2 + (item.y - y) ** 2
            if d2 <= best_d2:
                best, best_d2 = item, d2
        return best
