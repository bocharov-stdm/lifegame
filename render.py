# render.py — вся отрисовка мира.
#
# Единственный файл, который знает одновременно и про pygame, и про сущности.
# Сами сущности про экран не знают: раньше у каждой был свой draw(), и из-за этого
# движок тянул за собой pygame даже в headless-прогоне, которому экран не нужен.
#
# Мир большой (WORLD_WIDTH x WORLD_HEIGHT), окно маленькое — поэтому координаты
# умножаются на масштаб, который считает вызывающий: он же владеет размером окна.

import pygame

from life.config import PLANT_RADIUS

PLANT_COLOR      = (0, 255, 0)
VEGETARIAN_COLOR = (255, 100, 255)
PREDATOR_COLOR   = (255, 255, 255)


def draw_world(surf, world, scale_x, scale_y):
    """Рисует один кадр: растения, затем травоядные, затем хищники.

    Порядок задаёт перекрытие — кто нарисован позже, тот сверху.
    """
    circle = pygame.draw.circle

    for p in world.plants:
        circle(surf, PLANT_COLOR,
               (int(p.x * scale_x), int(p.y * scale_y)),
               max(1, int(PLANT_RADIUS * scale_x)))

    for v in world.vegetarians:
        circle(surf, VEGETARIAN_COLOR,
               (int(v.x * scale_x), int(v.y * scale_y)),
               max(1, int((v.size / 2) * scale_x)))

    for pr in world.predators:
        circle(surf, PREDATOR_COLOR,
               (int(pr.x * scale_x), int(pr.y * scale_y)),
               max(1, int((pr.DIAM // 2) * scale_x)))
