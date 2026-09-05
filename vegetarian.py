#vegetarian py

import pygame, random, math
from config import *

class Vegetarian:

    # ── инициализация ────────────────────────────────────────────────────────
    def __init__(self, x=None, y=None, energy=None, genom=None):
        if genom is None:
            genom = VEGETARIAN_BASE_GENOM[:]

        self.genom = genom
        self.size  = genom[0]
        self.speed = genom[1]
        self.vision = genom[2]
        self.repro_threshold = genom[3]
        self.repro_share     = genom[4]

        # ── НОВЫЕ ГЕНЫ: диапазон высот (в процентах) ────────────────────────
        min_pct = max(0, min(100, genom[5]))      # clamp 0‒100
        max_pct = max(0, min(100, genom[6]))
        if min_pct > max_pct:                     # гарантируем min ≤ max
            min_pct, max_pct = max_pct, min_pct

        self.min_y = min_pct      # храним в процентах, т.к. так уже использовали
        self.max_y = max_pct
        # абсолютные границы слоя
        y_min_abs = int((self.min_y / 100) * WORLD_HEIGHT + self.size)
        y_max_abs = int((self.max_y / 100) * WORLD_HEIGHT - self.size)
        # слой может оказаться уже собственного тела (эволюция сводит min_y и max_y):
        # тогда границы переворачиваются и randint ниже падает — схлопываем в точку
        if y_min_abs > y_max_abs:
            y_min_abs = y_max_abs = (y_min_abs + y_max_abs) // 2

        # ─────────────────────────────────────────────────────────────────────
        self.max_energy = self.size * VEGETARIAN_ENERGY_PER_SIZE
        self.energy = energy if energy is not None else self.max_energy * 0.5
        self.alive  = True

        self.flee_ticks = 0
        self.flee_dx = self.flee_dy = 0.0   # последний вектор бегства (единичный)

        self.x = x if x is not None else random.randint(self.size, WORLD_WIDTH - self.size)
        # стартовая y строго в пределах своего биома
        self.y = y if y is not None else random.randint(y_min_abs, y_max_abs)

        # ── предвычисленное ──────────────────────────────────────────────────
        # Геном не меняется всю жизнь, поэтому всё производное от него считается
        # один раз здесь, а не заново на каждом тике в move(). Особенно расход
        # энергии: три возведения в степень за тик на каждое существо.
        self.upkeep = (
                SIZE_ENERGY_COEF  * self.size   ** SIZE_ENERGY_POWER +
                SPEED_ENERGY_COEF * self.speed  ** SPEED_ENERGY_POWER +
                SIGHT_ENERGY_COEF * self.vision ** SIGHT_ENERGY_POWER
        )
        # границы слоя: без запаса на тело — решить, дотянемся ли до еды;
        # с запасом — прижать само тело, чтобы не торчало из слоя
        self.layer_lo = (self.min_y / 100) * WORLD_HEIGHT
        self.layer_hi = (self.max_y / 100) * WORLD_HEIGHT
        self.body_lo  = self.layer_lo + self.size
        self.body_hi  = self.layer_hi - self.size
        self.x_lo     = self.size
        self.x_hi     = WORLD_WIDTH - self.size
        # квадраты радиусов: сравнивать квадраты расстояний дешевле, чем звать hypot
        self.vision2 = self.vision * self.vision
        self.size2   = self.size * self.size
        self.flee2   = (self.vision / 3) * (self.vision / 3)

    # Функция отрисовки существа

    def draw(self, surf, scale_x, scale_y):
        sx = int(self.x * scale_x)
        sy = int(self.y * scale_y)
        radius = max(1, int((self.size / 2) * scale_x))
        pygame.draw.circle(surf, (255, 100, 255), (sx, sy), radius)

    # Функция движения
    def _pick_random_target(self):
        """Случайная точка в пределах vision и своего вертикального слоя."""
        lo, hi = self.body_lo, self.body_hi
        x, y, vision = self.x, self.y, self.vision
        x_lo, x_hi = self.x_lo, self.x_hi

        for _ in range(10):
            angle = random.uniform(0, 2 * math.pi)
            dist  = random.uniform(vision * 0.5, vision * 2)
            tx = x + math.cos(angle) * dist
            ty = y + math.sin(angle) * dist
            if x_lo <= tx <= x_hi and lo <= ty <= hi:
                self.tx, self.ty = tx, ty
                return
        self.tx, self.ty = x, y  # fallback

    # move() — самое горячее место всей симуляции: он вызывается за каждое
    # травоядное каждый тик. Отсюда локальные переменные вместо self.x в циклах,
    # квадраты расстояний вместо hypot и развёрнутые вручную min(max(...)).
    def move(self, plants, predators):
        x, y, speed = self.x, self.y, self.speed

        # ── ищем ближайшего хищника в пределах vision ───────────────────────
        nearest_pred, best_pred_d2 = None, self.vision2
        for pr in predators:
            dx = pr.x - x
            dy = pr.y - y
            d2 = dx * dx + dy * dy
            if d2 < best_pred_d2:
                nearest_pred, best_pred_d2 = pr, d2

        fleeing = False
        if self.flee_ticks > 0:
            fleeing = True
            self.flee_ticks -= 1
        if nearest_pred is not None and best_pred_d2 < self.flee2:
            self.flee_ticks = FLEE_TICKS
            fleeing = True

        if fleeing:
            # пока хищник виден — обновляем направление бегства;
            # когда он пропал из виду, продолжаем бежать по последнему вектору
            if nearest_pred is not None:
                dx, dy = x - nearest_pred.x, y - nearest_pred.y
                d = math.hypot(dx, dy)
                if d != 0:
                    self.flee_dx, self.flee_dy = dx / d, dy / d

            tx = x + self.flee_dx * speed
            ty = y + self.flee_dy * speed
        else:
            # поиск ближайшего растения
            nearest, best_d2 = None, self.vision2
            for p in plants:
                if not p.alive:            # съедено раньше в этом же тике
                    continue
                dx = p.x - x
                dy = p.y - y
                d2 = dx * dx + dy * dy
                if d2 < best_d2:
                    nearest, best_d2 = p, d2

            # проверяем попадает ли еда в наш вертикальный слой.
            # ВАЖНО: сперва ищем ближайшее вообще и только потом отбрасываем.
            # Слить проверку слоя внутрь цикла — значит искать ближайшее В СЛОЕ,
            # а это уже другое поведение: существо перестанет отвлекаться на еду,
            # до которой не дотянуться, и пойдёт к следующей вместо блуждания.
            if nearest is not None and not (self.layer_lo <= nearest.y <= self.layer_hi):
                nearest = None

            if nearest is not None:
                tx, ty = nearest.x, nearest.y
            else:
                # hasattr, а не tx=x в __init__: существо, родившееся во время
                # бегства, иначе пошло бы потом к месту своего рождения
                if not hasattr(self, "tx"):
                    self._pick_random_target()
                else:
                    dx, dy = x - self.tx, y - self.ty
                    if dx * dx + dy * dy < speed * speed:
                        self._pick_random_target()
                tx, ty = self.tx, self.ty

        # вектор и шаг
        dx, dy = tx - x, ty - y
        d = math.hypot(dx, dy)
        if d != 0:
            step = speed / d      # именно так, а не (dx*speed)/d — округление разное
            dx *= step
            dy *= step

        # новое положение: границы мира, затем свой вертикальный слой
        size = self.size
        nx = x + dx
        if   nx < self.x_lo: nx = self.x_lo
        elif nx > self.x_hi: nx = self.x_hi

        ny = y + dy
        if   ny < size:                 ny = size
        elif ny > WORLD_HEIGHT - size:  ny = WORLD_HEIGHT - size
        if   ny < self.body_lo: ny = self.body_lo
        elif ny > self.body_hi: ny = self.body_hi

        self.x, self.y = nx, ny

        # энергозатраты (посчитаны один раз в __init__ — геном не меняется)
        self.energy -= self.upkeep
        if self.energy <= 0:
            self.alive = False

    def try_eat(self, plants: list) -> bool:
        """
        Съесть все растения, которые попали в диаметр `self.size`.
        Возвращает True, если хоть что-то съели.

        `plants` — кандидаты от сетки соседей, а не весь мир (см. world.py).
        Съеденное помечается alive=False, а не вырезается из списка: вырезание
        стоило линейного поиска на каждое растение и сломало бы кэш сетки.
        Мёртвые выметаются один раз за тик в World.
        """
        eaten = 0
        r2 = self.size2                     # квадрат диаметра
        cx, cy = self.x, self.y

        for p in plants:
            if not p.alive:
                continue
            dx = cx - p.x
            dy = cy - p.y
            if dx * dx + dy * dy <= r2:
                p.alive = False
                eaten += 1

        if not eaten:
            return False

        # пополняем энергию
        gain = ENERGY_FROM_PLANT * eaten
        self.energy = min(self.max_energy, self.energy + gain)

        # ► сразу берём новую случайную цель — перестаём топтаться на месте
        self._pick_random_target()

        return True

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
        """Размножаемся, если остаётся запас энергии; детей кладём в отдельный список."""
        threshold = self.max_energy * (self.repro_threshold / 100)
        if self.energy < threshold + VEGETARIAN_REPRO_RESERVE:
            return                      # энергии недостаточно

        child_energy = self.energy * (self.repro_share / 100)
        self.energy -= child_energy + VEGETARIAN_REPRO_COST      # родитель платит

        child_energy = min(child_energy, self.max_energy)  # не переливать
        child_genom  = self.mutate()

        offset = random.uniform(-self.size * 2, self.size * 2)
        cx = min(max(self.x + offset, self.size), WORLD_WIDTH  - self.size)
        cy = min(max(self.y + offset, self.size), WORLD_HEIGHT - self.size)

        offspring.append(
            Vegetarian(x=cx, y=cy, energy=child_energy, genom=child_genom)
        )
