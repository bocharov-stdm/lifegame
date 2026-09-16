# render.py — вся отрисовка: мир, выделение, панели.
#
# Единственный файл, который знает одновременно и про pygame, и про сущности.
# Сами сущности про экран не знают: раньше у каждой был свой draw(), и из-за этого
# движок тянул за собой pygame даже в headless-прогоне, которому экран не нужен.
#
# Мир большой (WORLD_WIDTH x WORLD_HEIGHT), окно маленькое — поэтому координаты
# умножаются на масштаб, который считает вызывающий: он же владеет размером окна.

import pygame

from life.config     import PLANT_RADIUS
from life.genome     import Genom, GENE_LABELS
from life.vegetarian import Vegetarian

PLANT_COLOR      = (0, 255, 0)
VEGETARIAN_COLOR = (255, 100, 255)
PREDATOR_COLOR   = (255, 255, 255)

TEXT_COLOR      = (255, 255, 255)
HINT_COLOR      = (140, 140, 140)
ACCENT_COLOR    = (255, 220, 0)      # выделение, заголовки, пауза
PANEL_BG        = (0, 0, 0, 170)     # полупрозрачная подложка

MARGIN = 10                          # отступ панелей от края окна
PAD    = 8                           # отступ текста внутри панели
PANEL_TOP = 60                       # панели — ниже строки статистики и HUD


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


# ── выбранное существо ─────────────────────────────────────────────────────

def draw_selection(surf, creature, scale_x, scale_y):
    """Кольцо вокруг выбранного существа — чуть шире его самого."""
    if isinstance(creature, Vegetarian):
        body = max(1, int((creature.size / 2) * scale_x))
    else:
        body = max(1, int((creature.DIAM // 2) * scale_x))

    pygame.draw.circle(surf, ACCENT_COLOR,
                       (int(creature.x * scale_x), int(creature.y * scale_y)),
                       body + 4, width=2)


def draw_panel(surf, font, creature):
    """Плашка справа сверху: геном и энергия выбранного существа.

    У травоядного есть геном; у хищника генома нет — только скорость и зрение.
    """
    if isinstance(creature, Vegetarian):
        title = "Травоядное"
        rows = [(GENE_LABELS[name], f"{value:.1f}")
                for name, value in zip(Genom._fields, creature.genom)]
    else:
        title = "Хищник"
        rows = [("скорость", f"{creature.speed:.1f}"),
                ("зрение",   f"{creature.vision:.1f}")]
    rows.append(("энергия", f"{creature.energy:.1f} / {creature.max_energy:.0f}"))

    head   = font.render(title, True, ACCENT_COLOR)
    labels = [font.render(label, True, TEXT_COLOR) for label, _ in rows]
    values = [font.render(value, True, TEXT_COLOR) for _, value in rows]

    line_h = font.get_linesize()
    width  = max(head.get_width(),
                 max(s.get_width() for s in labels) + 3 * PAD +
                 max(s.get_width() for s in values))
    rect = _backdrop(surf, (width + 2 * PAD, (len(rows) + 1) * line_h + 2 * PAD),
                     topright=(surf.get_width() - MARGIN, PANEL_TOP))

    y = rect.top + PAD
    surf.blit(head, (rect.left + PAD, y))
    for label, value in zip(labels, values):
        y += line_h
        surf.blit(label, (rect.left + PAD, y))
        surf.blit(value, (rect.right - PAD - value.get_width(), y))   # числа — вправо


# ── строка состояния ───────────────────────────────────────────────────────

HINT = "Пробел — пауза   → — шаг   +/- — скорость   клик — существо"


def draw_hud(surf, font, paused, speed, tick):
    """Состояние симуляции под строкой статистики и подсказка внизу."""
    text = f"тик {tick}   x{speed}"
    if paused:
        text = "ПАУЗА   " + text
    surf.blit(font.render(text, True, ACCENT_COLOR if paused else TEXT_COLOR),
              (MARGIN, 34))

    hint = font.render(HINT, True, HINT_COLOR)
    surf.blit(hint, (MARGIN, surf.get_height() - MARGIN - hint.get_height()))


def _backdrop(surf, size, **anchor):
    """Полупрозрачная подложка; anchor — как у Rect (topright=..., bottomright=...)."""
    box = pygame.Surface(size, pygame.SRCALPHA)
    box.fill(PANEL_BG)
    rect = box.get_rect(**anchor)
    surf.blit(box, rect)
    return rect
