#vegetarian py

import pygame, random, math
from config import *

class Vegetarian:

    # Инициализация

    def __init__(self, x=None, y=None, energy=None, genom=None):
        if genom is None:
            genom = VEGETARIAN_BASE_GENOM[:]

        self.genom = genom
        self.size = genom[0]
        self.speed = genom[1]
        self.vision = genom[2]
        self.repro_threshold = genom[3]
        self.repro_share = genom[4]

        self.max_energy = self.size * 2.5
        self.energy = energy if energy is not None else self.max_energy * 0.5
        self.alive = True

        self.x = x if x is not None else random.randint(self.size, WORLD_WIDTH - self.size)
        self.y = y if y is not None else random.randint(self.size, WORLD_HEIGHT - self.size)

    # Функция отрисовки существа

    def draw(self, surf, scale_x, scale_y):
        sx = int(self.x * scale_x)
        sy = int(self.y * scale_y)
        radius = max(1, int((self.size / 2) * scale_x))  # размер — радиус

        pygame.draw.circle(surf, (255, 100, 255), (sx, sy), radius)

    # Функция движения
    def _pick_random_target(self):
        for _ in range(10):
            angle = random.uniform(0, 2 * math.pi)
            dist = random.uniform(self.vision * 0.5, self.vision * 2)
            tx = self.x + math.cos(angle) * dist
            ty = self.y + math.sin(angle) * dist
            if self.size <= tx <= WORLD_WIDTH - self.size and \
                    self.size <= ty <= WORLD_HEIGHT - self.size:
                self.tx, self.ty = tx, ty
                return
        # fallback
        self.tx, self.ty = self.x, self.y

    def move(self, plants):

        # Найти ближайшее растение в зоне зрения
        nearest = None
        best_dist = self.vision
        for p in plants:
            dx = p.x - self.x
            dy = p.y - self.y
            dist = math.hypot(dx, dy)
            if dist < best_dist:
                best_dist = dist
                nearest = p

        if nearest:
            # Двигаемся к растению
            tx, ty = nearest.x, nearest.y
        else:
            # если ещё нет цели или дошли — выбираем новую
            if not hasattr(self, "tx") or math.hypot(self.x - self.tx, self.y - self.ty) < self.speed:
                self._pick_random_target()
            tx, ty = self.tx, self.ty

        # Вектор движения
        dx = tx - self.x
        dy = ty - self.y
        d = math.hypot(dx, dy)
        if d != 0:
            dx *= self.speed / d
            dy *= self.speed / d

        # Обновляем позицию
        self.x = min(max(self.x + dx, self.size), WORLD_WIDTH - self.size)
        self.y = min(max(self.y + dy, self.size), WORLD_HEIGHT - self.size)
        # проверяем и пересчитываем энергию
        self.energy -= (
                SIZE_ENERGY_COEF * self.size**1.5 +
                SPEED_ENERGY_COEF * (self.speed ** 2) +
                SIGHT_ENERGY_COEF * self.vision
        )
        if self.energy <= 0:
            self.alive = False
    def try_eat(self, plants: list) -> bool:
        """
        Съесть все растения, которые попали в диаметр `self.size`.
        Возвращает True, если хоть что-то съели.
        """
        eaten = []
        r2 = self.size * self.size          # квадрат диаметра
        cx, cy = self.x, self.y

        # ищем «в лоб» — пока растений мало это дёшево
        for p in plants:
            dx = cx - p.x
            dy = cy - p.y
            if dx * dx + dy * dy <= r2:
                eaten.append(p)

        if not eaten:
            return False

        # удаляем съеденное
        for p in eaten:
            plants.remove(p)

        # пополняем энергию
        gain = ENERGY_FROM_PLANT * len(eaten)
        self.energy = min(self.max_energy, self.energy + gain)

        # ► сразу берём новую случайную цель — перестаём топтаться на месте
        self._pick_random_target()

        return True

    def mutate(self, sigma = VEGETARIAN_SIGMA):
        """Возвращает новый мутированный геном."""
        new_genom = []
        for value in self.genom:
            while True:
                gauss = random.gauss(0, sigma)
                if gauss >= -0.9:
                    break
            mutated = value * (1 + gauss)
            new_genom.append(max(0.01, mutated))
        return new_genom

    def maybe_divide(self, vegetarians: list):
        """Попытка размножиться — если хватает энергии."""
        threshold = self.max_energy * (self.repro_threshold / 100)
        if self.energy >= threshold:
            # Сколько энергии отдать ребёнку
            child_energy = self.energy * (self.repro_share / 100)
            self.energy -= child_energy + 10

            # Мутируем геном для потомка
            child_genom = self.mutate()

            # Случайный сдвиг позиции, чтобы не заспавнить в себе
            offset = random.uniform(-self.size*10, self.size*10)
            child_x = min(max(self.x + offset, self.size), WORLD_WIDTH - self.size)
            child_y = min(max(self.y + offset, self.size), WORLD_HEIGHT - self.size)

            # Создаём нового Vegetarian и добавляем в список
            vegetarians.append(
                Vegetarian(x=child_x, y=child_y, energy=child_energy, genom=child_genom)
            )



