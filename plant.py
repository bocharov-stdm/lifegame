# plant.py

import pygame, random, math
from config import *


# Растения гуще у поверхности: плотность падает как exp(-DECAY * y / глубина мира).
# Раньше это делалось отбраковкой — тянуть случайную точку, пока не повезёт. При
# нынешнем DECAY=8 везёт в среднем с 11.5-й попытки, то есть 23 лишних обращения к
# генератору на каждое растение, да ещё циклом без гарантии завершения.
#
# Тот же закон берётся напрямую через обратную функцию распределения: тянем
# равномерное u и решаем уравнение относительно y. Одна попытка, точно то же
# распределение (проверено на 300k выборок: среднее и медиана совпадают).
PLANT_DEPTH_LAMBDA = PLANT_DEPTH_DECAY / WORLD_HEIGHT
_Y_TOP    = PLANT_RADIUS + PLANT_TOP_MARGIN
_Y_BOTTOM = WORLD_HEIGHT - PLANT_RADIUS
_E_TOP    = math.exp(-PLANT_DEPTH_LAMBDA * _Y_TOP)
_E_BOTTOM = math.exp(-PLANT_DEPTH_LAMBDA * _Y_BOTTOM)


class Plant:
    __slots__ = ("x", "y", "alive")

    def __init__(self):
        self.x = random.uniform(PLANT_RADIUS, WORLD_WIDTH - PLANT_RADIUS)

        u = random.random()
        self.y = -math.log(_E_TOP - u * (_E_TOP - _E_BOTTOM)) / PLANT_DEPTH_LAMBDA

        # Съеденное помечается, а не вырезается из списка: вырезание — линейный
        # поиск, да ещё и рушит сетку соседей. Мёртвые выметаются раз за тик.
        self.alive = True

    def draw(self, surf, scale_x, scale_y):
        pygame.draw.circle(
            surf, (0, 255, 0),
            (int(self.x * scale_x), int(self.y * scale_y)),
            max(1, int(PLANT_RADIUS * scale_x))
        )
