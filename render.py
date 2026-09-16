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


# ── график популяций ───────────────────────────────────────────────────────

GRAPH_SIZE   = (420, 170)
GRAPH_SERIES = (("растения",   PLANT_COLOR),
                ("травоядные", VEGETARIAN_COLOR),
                ("хищники",    PREDATOR_COLOR))


def draw_graph(surf, font, history):
    """Бегущий график численности в правом нижнем углу.

    history — последовательность (растения, травоядные, хищники), старые точки
    первыми. Если это deque с maxlen, график заполняется справа налево и дальше
    ползёт; обычный список растягивается на всю ширину.

    Каждая линия нормирована на СВОЙ максимум: хищников единицы и десятки,
    остальных — сотни, и на общей шкале хищники легли бы в ноль. А смысл
    графика как раз в том, чтобы видеть сдвиг фаз «хищник — жертва».
    """
    line_h = font.get_linesize()

    last   = history[-1] if history else (0, 0, 0)
    legend = [font.render(f"{name} {value}", True, color)
              for (name, color), value in zip(GRAPH_SERIES, last)]
    legend_w = sum(s.get_width() for s in legend) + 2 * PAD * (len(legend) - 1)

    width, height = max(GRAPH_SIZE[0], legend_w + 2 * PAD), GRAPH_SIZE[1]
    rect = _backdrop(surf, (width, height),
                     bottomright=(surf.get_width() - MARGIN, surf.get_height() - MARGIN))

    x = rect.left + PAD
    for s in legend:
        surf.blit(s, (x, rect.top + PAD))
        x += s.get_width() + 2 * PAD

    n = len(history)
    if n < 2:
        return                                   # линию из одной точки не построить

    plot = pygame.Rect(rect.left + PAD, rect.top + PAD + line_h,
                       width - 2 * PAD, height - 2 * PAD - line_h)
    span = (getattr(history, "maxlen", None) or n) - 1
    dx   = plot.width / span
    x0   = plot.right - (n - 1) * dx             # новые точки — у правого края

    for k, (_, color) in enumerate(GRAPH_SERIES):
        top = max(point[k] for point in history) or 1     # все нули — не делим на ноль
        points = [(x0 + i * dx, plot.bottom - point[k] / top * plot.height)
                  for i, point in enumerate(history)]
        pygame.draw.lines(surf, color, False, points)


# ── строка состояния ───────────────────────────────────────────────────────

# без «→»: в стандартном шрифте pygame такого символа нет, рисуется квадратик
HINT = "Пробел — пауза   Вправо — шаг   +/- — скорость   клик — существо   G — график"


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
