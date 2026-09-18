#vegetarian py

import random, math
from .config import *
from .genome import Genom, PERCENT_GENES
from .rules  import DEFAULT_RULES

class Vegetarian:

    # ── инициализация ────────────────────────────────────────────────────────
    def __init__(self, x=None, y=None, energy=None, genom=None, rules=None):
        # правила мира (цена статов, энергия растения, мутации); дети наследуют их
        self.rules = rules = rules or DEFAULT_RULES
        if genom is None:
            genom = VEGETARIAN_BASE_GENOM
        genom = Genom(*genom)          # принимаем и список (конфиг, тесты), и Genom

        self.genom = genom
        self.size  = genom.size
        self.speed = genom.speed
        self.vision = genom.vision
        self.repro_threshold = genom.repro_threshold
        self.repro_share     = genom.repro_share

        # ── гены слоя: диапазон высот (в процентах) ─────────────────────────
        min_pct = max(0, min(100, genom.min_y))   # clamp 0‒100
        max_pct = max(0, min(100, genom.max_y))
        if min_pct > max_pct:                     # гарантируем min ≤ max
            min_pct, max_pct = max_pct, min_pct

        self.min_y = min_pct      # храним в процентах, т.к. так уже использовали
        self.max_y = max_pct

        # ── границы ─────────────────────────────────────────────────────────
        # Границы слоя без запаса на тело — по ним решается, дотянемся ли до еды;
        # с запасом — полоса, в которой держится само тело, чтобы не торчало.
        self.layer_lo = (min_pct / 100) * WORLD_HEIGHT
        self.layer_hi = (max_pct / 100) * WORLD_HEIGHT
        # Запас на тело — не больше половины мира. Размер — ген, и при дешёвом
        # размере (лаборатория) тело бывает больше мира: с полным запасом границы
        # переворачивались (x_lo > x_hi), зажимы в move() перекидывали существо
        # от края к краю на сотни пикселей и выталкивали за мир. Тело шире мира
        # целиком не уместить — такое существо держится на средней линии.
        margin_x = min(self.size, WORLD_WIDTH / 2)
        margin_y = min(self.size, WORLD_HEIGHT / 2)
        body_lo = self.layer_lo + margin_y
        body_hi = self.layer_hi - margin_y
        # Слой может оказаться уже собственного тела (эволюция сводит min_y и max_y),
        # и полоса переворачивается. Схлопываем её в линию посередине — один раз и
        # для всех: и для рождения, и для move(), и для выбора цели. Середину держим
        # в мире: у слоя на самом краю она легла бы за границу.
        if body_lo > body_hi:
            body_lo = body_hi = min(max((body_lo + body_hi) / 2, margin_y),
                                    WORLD_HEIGHT - margin_y)
        self.body_lo, self.body_hi = body_lo, body_hi
        self.x_lo = margin_x
        self.x_hi = WORLD_WIDTH - margin_x

        # ─────────────────────────────────────────────────────────────────────
        self.max_energy = self.size * VEGETARIAN_ENERGY_PER_SIZE
        # не переливать: у ребёнка свой бак, и он может быть меньше родительского
        self.energy = (min(energy, self.max_energy) if energy is not None
                       else self.max_energy * 0.5)
        self.alive  = True

        self.flee_ticks = 0
        self.flee_dx = self.flee_dy = 0.0   # последний вектор бегства (единичный)

        # Позиция всегда внутри своей полосы — и случайная, и заданная. Ребёнок
        # рождается у родителя, а слой у него уже свой, мутировавший: без зажима
        # первый же ход телепортировал бы его в слой, бывало на 2000 px.
        # uniform, а не randint: размер — ген, он дробный, а randint дробных не берёт.
        if x is None:
            x = random.uniform(self.x_lo, self.x_hi)
        if y is None:
            y = random.uniform(body_lo, body_hi)
        self.x = min(max(x, self.x_lo), self.x_hi)
        self.y = min(max(y, body_lo), body_hi)

        # ── предвычисленное ──────────────────────────────────────────────────
        # Геном не меняется всю жизнь, поэтому всё производное от него считается
        # один раз здесь, а не заново на каждом тике в move(). Особенно расход
        # энергии: три возведения в степень за тик на каждое существо.
        self.upkeep = rules.upkeep(self.size, self.speed, self.vision)
        # квадраты радиусов: сравнивать квадраты расстояний дешевле, чем звать hypot
        self.vision2 = self.vision * self.vision
        self.size2   = self.size * self.size
        self.half    = self.size / 2        # радиус тела: по нему ловят хищники
        self.flee2   = (self.vision / 3) * (self.vision / 3)

    # Функция движения
    def _pick_random_target(self):
        """Случайная точка в пределах vision и своего вертикального слоя."""
        lo, hi = self.body_lo, self.body_hi
        x, y, vision = self.x, self.y, self.vision
        x_lo, x_hi = self.x_lo, self.x_hi
        # слой схлопнут в линию: случайная точка на неё не попадёт никогда,
        # и существо стояло бы столбом — гуляем только вдоль линии
        flat = lo == hi

        for _ in range(10):
            angle = random.uniform(0, 2 * math.pi)
            dist  = random.uniform(vision * 0.5, vision * 2)
            tx = x + math.cos(angle) * dist
            ty = lo if flat else y + math.sin(angle) * dist
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

        # Новое положение: по x — границы мира, по y — своя полоса. Полоса
        # (см. __init__) всегда лежит внутри мира, поэтому отдельный зажим по
        # высоте мира не нужен. Существо уже стоит внутри своих границ, так что
        # зажим только укорачивает шаг: за тик оно сдвигается не дальше speed.
        nx = x + dx
        if   nx < self.x_lo: nx = self.x_lo
        elif nx > self.x_hi: nx = self.x_hi

        ny = y + dy
        if   ny < self.body_lo: ny = self.body_lo
        elif ny > self.body_hi: ny = self.body_hi

        self.x, self.y = nx, ny

        # энергозатраты (посчитаны один раз в __init__ — геном не меняется)
        self.energy -= self.upkeep
        if self.energy <= 0:
            self.alive = False

    def try_eat(self, plants: list) -> bool:
        """
        Съесть все растения не дальше `self.size` от центра.
        Возвращает True, если хоть что-то съели.

        size — диаметр тела (так оно и рисуется), а дотягивается существо на
        целый size, то есть на полтела дальше своего края.

        `plants` — кандидаты от сетки соседей, а не весь мир (см. world.py).
        Съеденное помечается alive=False, а не вырезается из списка: вырезание
        стоило линейного поиска на каждое растение и сломало бы кэш сетки.
        Мёртвые выметаются один раз за тик в World.
        """
        eaten = 0
        r2 = self.size2                     # квадрат радиуса поедания
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
        gain = self.rules.plant_energy * eaten
        self.energy = min(self.max_energy, self.energy + gain)

        # ► сразу берём новую случайную цель — перестаём топтаться на месте
        self._pick_random_target()

        return True

    def mutate(self, sigma=None):
        if sigma is None:
            sigma = self.rules.mutation_sigma
        new_genom = []
        for name, value in zip(Genom._fields, self.genom):
            while True:
                gauss = random.gauss(0, sigma)
                if gauss >= -0.9:
                    break
            mutated = value * (1 + gauss)
            # гены-проценты (порог, доля потомку, слой) держим в 0‒100
            if name in PERCENT_GENES:
                mutated = min(100, max(0, mutated))
            new_genom.append(max(0.01, mutated))
        return Genom(*new_genom)

    def maybe_divide(self, offspring: list):
        """Размножаемся, если после деления у родителя остаётся резерв.

        Детей кладём в отдельный список: в этом тике они не ходят.
        """
        threshold = self.max_energy * (self.repro_threshold / 100)
        if self.energy < threshold + VEGETARIAN_REPRO_RESERVE:
            return                      # энергии недостаточно

        # Резерв проверяется ещё и ПОСЛЕ дележа: доля ребёнка считается от всей
        # энергии, и без этой проверки родитель, бывало, отдавал всё до нуля и
        # ниже — и умирал на следующем ходу, а хищник, съевший такого, терял энергию.
        child_energy = self.energy * (self.repro_share / 100)
        left = self.energy - child_energy - VEGETARIAN_REPRO_COST
        if left < VEGETARIAN_REPRO_RESERVE:
            return

        self.energy = left                                  # родитель платит
        child_genom = self.mutate()

        # Смещения по осям независимые: с одним общим дети ложились строго на
        # диагональ от родителя. В мир и в слой ребёнка зажимает его __init__.
        span = self.size * 2
        cx = self.x + random.uniform(-span, span)
        cy = self.y + random.uniform(-span, span)

        offspring.append(
            Vegetarian(x=cx, y=cy, energy=child_energy, genom=child_genom,
                       rules=self.rules)
        )
