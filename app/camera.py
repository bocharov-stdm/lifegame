# camera.py — какая часть мира видна во вьюпорте и в каком масштабе.
#
# Мир 6000x4000, а вьюпорт — около тысячи пикселей: целиком мир виден в
# масштабе ~0.2, где травоядное — точка в 4 пикселя. Камера даёт приблизиться
# (колесо — к курсору), сдвинуться и следить за существом.
#
# pygame здесь не нужен: модуль проверяют тесты без дисплея. Вьюпорт — это
# кортеж (x, y, ширина, высота) в пикселях экрана.

import math

MAX_ZOOM    = 2.0        # 1 единица мира = 2 пикселя: травоядное во весь палец
FOLLOW_RATE = 8.0        # плавность слежения: чем больше, тем плотнее за целью


class Camera:
    def __init__(self, world_w, world_h, view):
        self.world_w = world_w
        self.world_h = world_h
        self.view = tuple(view)
        self.target = None                 # существо, за которым следим
        self._last = None                  # где цель была в прошлом кадре
        self.fit()

    # ── масштаб ─────────────────────────────────────────────────────────────
    @property
    def min_zoom(self):
        """Мир целиком во вьюпорте — дальше отдалять незачем."""
        _, _, w, h = self.view
        return max(min(w / self.world_w, h / self.world_h), 1e-6)

    @property
    def max_zoom(self):
        return max(MAX_ZOOM, self.min_zoom)

    @property
    def is_fit(self):
        return self.zoom <= self.min_zoom * 1.0001

    def fit(self):
        """Показать весь мир."""
        self.zoom = self.min_zoom
        self.cx, self.cy = self.world_w / 2, self.world_h / 2
        self.target = None

    def set_view(self, view):
        """Новый размер вьюпорта. Центр остаётся на месте, вписанный мир — вписанным."""
        was_fit = self.is_fit
        self.view = tuple(view)
        if was_fit:
            self.zoom = self.min_zoom
        self.zoom = min(max(self.zoom, self.min_zoom), self.max_zoom)
        self._clamp()

    def zoom_at(self, sx, sy, factor):
        """Изменить масштаб так, чтобы точка мира под (sx, sy) осталась под курсором."""
        wx, wy = self.to_world(sx, sy)
        self.zoom = min(max(self.zoom * factor, self.min_zoom), self.max_zoom)
        vx, vy, w, h = self.view
        self.cx = wx - (sx - vx - w / 2) / self.zoom
        self.cy = wy - (sy - vy - h / 2) / self.zoom
        if self.target is not None:            # следим — значит, центр на цели
            self.cx, self.cy = self.target.x, self.target.y
        self._clamp()

    # ── сдвиг и слежение ────────────────────────────────────────────────────
    def pan(self, dx, dy):
        """Сдвиг на (dx, dy) пикселей экрана: мир едет вслед за мышью."""
        self.cx -= dx / self.zoom
        self.cy -= dy / self.zoom
        self.target = None
        self._clamp()

    def follow(self, creature):
        self.target = creature
        self._last = (creature.x, creature.y) if creature is not None else None

    def update(self, dt):
        """Слежение: камера едет вместе с целью и плавно подводит её к центру.

        Одного плавного догоняния мало: на скорости x32 существо за кадр
        уходит дальше, чем камера успевает подтянуться, и пропадает с экрана.
        Поэтому сначала камера сдвигается на столько же, на сколько сдвинулась
        цель, а плавность достаётся только остатку пути до центра.
        """
        target = self.target
        if target is None:
            return
        if not target.alive:
            self.target = None
            return
        lx, ly = self._last if self._last is not None else (target.x, target.y)
        self.cx += target.x - lx
        self.cy += target.y - ly
        self._last = (target.x, target.y)
        k = 1 - math.exp(-FOLLOW_RATE * dt)
        self.cx += (target.x - self.cx) * k
        self.cy += (target.y - self.cy) * k
        self._clamp()

    def _clamp(self):
        """Мир заполняет вьюпорт, насколько может; если он меньше — стоит по центру."""
        _, _, w, h = self.view
        half_w = w / 2 / self.zoom
        half_h = h / 2 / self.zoom
        if 2 * half_w >= self.world_w:
            self.cx = self.world_w / 2
        else:
            self.cx = min(max(self.cx, half_w), self.world_w - half_w)
        if 2 * half_h >= self.world_h:
            self.cy = self.world_h / 2
        else:
            self.cy = min(max(self.cy, half_h), self.world_h - half_h)

    # ── пересчёт координат ──────────────────────────────────────────────────
    def to_screen(self, wx, wy):
        vx, vy, w, h = self.view
        return (vx + w / 2 + (wx - self.cx) * self.zoom,
                vy + h / 2 + (wy - self.cy) * self.zoom)

    def to_world(self, sx, sy):
        vx, vy, w, h = self.view
        return (self.cx + (sx - vx - w / 2) / self.zoom,
                self.cy + (sy - vy - h / 2) / self.zoom)

    def contains(self, sx, sy):
        vx, vy, w, h = self.view
        return vx <= sx < vx + w and vy <= sy < vy + h

    def visible_world(self, margin=0.0):
        """Видимый прямоугольник мира (x0, y0, x1, y1) с запасом margin."""
        vx, vy, w, h = self.view
        x0, y0 = self.to_world(vx, vy)
        x1, y1 = self.to_world(vx + w, vy + h)
        return x0 - margin, y0 - margin, x1 + margin, y1 + margin
