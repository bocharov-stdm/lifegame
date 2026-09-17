# render.py — вся отрисовка: мир, графики, карточка существа, значки, текст.
#
# Единственный модуль, который знает одновременно и про pygame, и про
# сущности. Сами сущности про экран не знают: раньше у каждой был свой
# draw(), и из-за этого движок тянул за собой pygame даже в headless-прогоне.
#
# Мир рисуется через камеру (app/camera.py): она же решает, какая часть мира
# видна. Всё, что вне вьюпорта, не рисуется вовсе.

import math
from functools import lru_cache

import pygame
from pygame import gfxdraw

from life.config     import PLANT_RADIUS, WORLD_HEIGHT, WORLD_WIDTH
from life.genome     import Genom, PERCENT_GENES
from life.vegetarian import Vegetarian

from . import theme
from .theme import (ACCENT, BG, CARD, DANGER, FAINT, LINE, MUTED,
                    PLANT_COLOR, PREDATOR_COLOR, TEXT, VEGETARIAN_COLOR, S,
                    lerp_color)

# ── кэши ─────────────────────────────────────────────────────────────────────
# Рендер текста — самое дорогое в панелях, а почти все надписи от кадра к кадру
# одни и те же. Кэш ограничен, поэтому надписи, которые меняются каждый кадр
# (номер тика), его не раздувают. Отданные поверхности общие: их только
# блитят, но не рисуют на них.

@lru_cache(maxsize=1024)
def _text(font, text, color):
    return font.render(text, True, color)


# Полупрозрачная заливка: по одной поверхности на цвет, не меньше самой
# большой запрошенной; блитится её угол. Кэш по размеру держал бы по полному
# экрану на каждый размер окна и на каждый зум (полоса слоя) — сотни МиБ.
_fill_boxes = {}


def _fill_box(size, color):
    w, h = size
    box = _fill_boxes.get(color)
    if box is None or box.get_width() < w or box.get_height() < h:
        if box is not None:
            w, h = max(w, box.get_width()), max(h, box.get_height())
        box = pygame.Surface((w, h), pygame.SRCALPHA)
        box.fill(color)
        _fill_boxes[color] = box
    return box


# ── текст ────────────────────────────────────────────────────────────────────

def text(role, s, color=TEXT):
    return _text(theme.font(role), s, color)


def blit_text(surf, role, s, color, anchor, **where):
    """Надпись с привязкой как у Rect: blit_text(..., "midleft", midleft=(x, y))."""
    img = text(role, s, color)
    rect = img.get_rect(**{anchor: where[anchor]})
    surf.blit(img, rect)
    return rect


def text_width(role, s):
    return theme.font(role).size(s)[0]


def wrap(role, s, width):
    """Разбить текст на строки не шире width (по словам)."""
    lines = []
    for paragraph in s.split("\n"):
        line = ""
        for word in paragraph.split(" "):
            probe = f"{line} {word}" if line else word
            if line and text_width(role, probe) > width:
                lines.append(line)
                line = word
            else:
                line = probe
        lines.append(line)
    return lines


# ── формы ────────────────────────────────────────────────────────────────────

def rounded(surf, color, rect, radius=8, width=0):
    pygame.draw.rect(surf, color, rect, width, border_radius=S(radius))


def veil(surf, rect=None, color=theme.VEIL):
    """Полупрозрачное затемнение прямоугольника (по умолчанию — всего экрана)."""
    # не `rect or ...`: пустой Rect ложен, и пустая полоса стала бы всем экраном
    rect = surf.get_rect() if rect is None else pygame.Rect(rect)
    if rect.w > 0 and rect.h > 0:
        surf.blit(_fill_box(rect.size, color), rect, (0, 0, rect.w, rect.h))


def shadow(surf, rect, radius=12):
    """Мягкая тень под карточкой: пара полупрозрачных слоёв."""
    for grow, alpha in ((S(10), 40), (S(4), 60)):
        r = pygame.Rect(rect).inflate(grow, grow).move(0, S(4))
        box = pygame.Surface(r.size, pygame.SRCALPHA)
        pygame.draw.rect(box, (0, 0, 0, alpha), box.get_rect(),
                         border_radius=S(radius) + grow // 2)
        surf.blit(box, r)


def aa_circle(surf, color, center, radius, filled=True):
    x, y = int(round(center[0])), int(round(center[1]))
    r = int(round(radius))
    if r < 1:
        surf.fill(color, (x, y, 1, 1))
        return
    if filled:
        gfxdraw.filled_circle(surf, x, y, r, color)
    gfxdraw.aacircle(surf, x, y, r, color)


def aa_polygon(surf, color, points):
    gfxdraw.aapolygon(surf, points, color)
    gfxdraw.filled_polygon(surf, points, color)


def thick_line(surf, color, a, b, width):
    """Сглаженная линия толщиной width: четырёхугольник вместо зубчатой линии."""
    (x1, y1), (x2, y2) = a, b
    length = math.hypot(x2 - x1, y2 - y1) or 1
    nx, ny = -(y2 - y1) / length * width / 2, (x2 - x1) / length * width / 2
    aa_polygon(surf, color, [(x1 + nx, y1 + ny), (x2 + nx, y2 + ny),
                             (x2 - nx, y2 - ny), (x1 - nx, y1 - ny)])


# ── значки ───────────────────────────────────────────────────────────────────
# Рисуются примитивами: у шрифтов pygame нет стрелок и прочих символов,
# вместо них выходят квадратики. Координаты значка — от центра кнопки, в 1/30 её размера.

def draw_icon(surf, name, rect, color):
    cx, cy = pygame.Rect(rect).center
    u = min(rect[2], rect[3]) / 30          # значок занимает ~0.4 от кнопки

    def P(x, y):
        return (cx + x * u, cy + y * u)

    def R(x, y, w, h):
        return pygame.Rect(round(cx + x * u), round(cy + y * u),
                           max(1, round(w * u)), max(1, round(h * u)))

    stroke = max(2, round(2.2 * u))
    if name == "play":
        aa_polygon(surf, color, [P(-4, -6), P(-4, 6), P(6, 0)])
    elif name == "pause":
        surf.fill(color, R(-5, -6, 3.5, 12))
        surf.fill(color, R(1.5, -6, 3.5, 12))
    elif name == "step":
        aa_polygon(surf, color, [P(-6, -6), P(-6, 6), P(2, 0)])
        surf.fill(color, R(3, -6, 3, 12))
    elif name == "minus":
        surf.fill(color, R(-6, -1.2, 12, 2.4))
    elif name == "plus":
        surf.fill(color, R(-6, -1.2, 12, 2.4))
        surf.fill(color, R(-1.2, -6, 2.4, 12))
    elif name == "fit":
        for sx in (-1, 1):
            for sy in (-1, 1):
                corner = P(7 * sx, 7 * sy)
                thick_line(surf, color, corner, P(7 * sx, 2.5 * sy), stroke)
                thick_line(surf, color, corner, P(2.5 * sx, 7 * sy), stroke)
    elif name == "menu":
        for y in (-5, -1.2, 2.6):
            surf.fill(color, R(-7, y, 14, 2.4))
    elif name == "dice":
        box = R(-7, -7, 14, 14)
        pygame.draw.rect(surf, color, box, max(1, round(1.8 * u)),
                         border_radius=max(2, round(3 * u)))
        for x, y in ((-3, -3), (3, 3), (0, 0), (3, -3), (-3, 3)):
            aa_circle(surf, color, P(x, y), 1.3 * u)
    elif name == "close":
        thick_line(surf, color, P(-5, -5), P(5, 5), stroke)
        thick_line(surf, color, P(-5, 5), P(5, -5), stroke)
    elif name == "follow":
        pygame.draw.circle(surf, color, P(0, 0), round(6 * u), max(1, round(1.8 * u)))
        aa_circle(surf, color, P(0, 0), 2 * u)
        for dx, dy in ((0, -1), (0, 1), (-1, 0), (1, 0)):
            thick_line(surf, color, P(dx * 7, dy * 7), P(dx * 10, dy * 10),
                       max(1, round(1.8 * u)))
    elif name == "back":
        thick_line(surf, color, P(3, -6), P(-3, 0), stroke)
        thick_line(surf, color, P(-3, 0), P(3, 6), stroke)
    else:
        raise ValueError(f"нет значка {name!r}")


# ── мир ──────────────────────────────────────────────────────────────────────

BANDS = 32
_BAND_COLORS = [lerp_color(theme.WORLD_TOP, theme.WORLD_BOTTOM, i / (BANDS - 1))
                for i in range(BANDS)]

# Травоядное тем ярче, чем полнее его бак: голодающих видно сразу.
_SHADES = 6
_VEG_SHADES = [lerp_color(theme.WORLD_BOTTOM, VEGETARIAN_COLOR, 0.45 + 0.55 * i / (_SHADES - 1))
               for i in range(_SHADES)]
_PLANT_DIM = lerp_color(theme.WORLD_BOTTOM, PLANT_COLOR, 0.8)


def draw_world(surf, world, cam, selected=None, smooth=True):
    """Кадр мира во вьюпорте камеры: фон, растения, травоядные, хищники, выделение.

    Порядок задаёт перекрытие — кто нарисован позже, тот сверху. Круг — это
    тело: size травоядного и DIAM хищника — диаметры. Ест травоядное дальше
    своего края (см. Vegetarian.try_eat).
    """
    view = pygame.Rect(cam.view)
    old_clip = surf.get_clip()
    surf.set_clip(view)
    surf.fill(BG, view)

    zoom = cam.zoom
    ox = view.x + view.w / 2 - cam.cx * zoom       # экран = мир * zoom + (ox, oy)
    oy = view.y + view.h / 2 - cam.cy * zoom

    # ── фон: полосы глубины ─────────────────────────────────────────────────
    left  = max(int(ox), view.left)
    right = min(int(math.ceil(ox + WORLD_WIDTH * zoom)), view.right)
    band  = WORLD_HEIGHT / BANDS * zoom
    for i, color in enumerate(_BAND_COLORS):
        top, bottom = int(oy + i * band), int(oy + (i + 1) * band)
        if bottom < view.top or top > view.bottom:
            continue
        surf.fill(color, (left, top, right - left, max(1, bottom - top)))
    world_rect = pygame.Rect(int(ox), int(oy),
                             int(WORLD_WIDTH * zoom), int(WORLD_HEIGHT * zoom))
    pygame.draw.rect(surf, LINE, world_rect, 1)

    # полоса слоя выбранного травоядного — где ему можно жить и есть
    if isinstance(selected, Vegetarian) and selected.alive:
        top = int(oy + selected.layer_lo * zoom)
        bottom = int(oy + selected.layer_hi * zoom)
        band_rect = pygame.Rect(left, top, right - left, max(1, bottom - top)).clip(view)
        veil(surf, band_rect, (*VEGETARIAN_COLOR, 16))
        for y in (top, bottom):
            if view.top <= y < view.bottom:
                surf.fill(lerp_color(theme.WORLD_BOTTOM, VEGETARIAN_COLOR, 0.35),
                          (left, y, right - left, 1))

    # Отсекаем по телу, а не по центру: крупное существо (размер — ген, при
    # пологой цене он дорастает до тысяч) торчит в кадр, даже когда центр далеко.
    x0, y0, x1, y1 = cam.visible_world()

    # ── растения ────────────────────────────────────────────────────────────
    r = PLANT_RADIUS * zoom
    fill = surf.fill
    if r < 1.6:                                    # мелочь — квадратиками, быстрее кругов
        side = max(3, int(round(r * 2)))
        half = side / 2
        px0, py0, px1, py1 = x0 - PLANT_RADIUS, y0 - PLANT_RADIUS, x1 + PLANT_RADIUS, y1 + PLANT_RADIUS
        for p in world.plants:
            x, y = p.x, p.y
            if px0 < x < px1 and py0 < y < py1:
                fill(_PLANT_DIM, (int(x * zoom + ox - half), int(y * zoom + oy - half), side, side))
    else:
        circle = pygame.draw.circle
        px0, py0, px1, py1 = x0 - PLANT_RADIUS, y0 - PLANT_RADIUS, x1 + PLANT_RADIUS, y1 + PLANT_RADIUS
        for p in world.plants:
            x, y = p.x, p.y
            if px0 < x < px1 and py0 < y < py1:
                circle(surf, _PLANT_DIM, (int(x * zoom + ox), int(y * zoom + oy)), int(r))

    # ── травоядные ──────────────────────────────────────────────────────────
    smooth_r = 2.0 if smooth else 1e9
    top_shade = _SHADES - 1
    for v in world.vegetarians:
        x, y = v.x, v.y
        half = v.size / 2
        if x + half < x0 or x - half > x1 or y + half < y0 or y - half > y1:
            continue
        shade = v.energy / v.max_energy * _SHADES
        color = _VEG_SHADES[top_shade if shade >= top_shade else int(shade) if shade > 0 else 0]
        sx, sy = int(x * zoom + ox), int(y * zoom + oy)
        rr = v.size / 2 * zoom
        if rr >= smooth_r:
            ir = int(rr)
            gfxdraw.filled_circle(surf, sx, sy, ir, color)
            gfxdraw.aacircle(surf, sx, sy, ir, color)
        else:
            pygame.draw.circle(surf, color, (sx, sy), max(1, int(round(rr))))

    # ── хищники ─────────────────────────────────────────────────────────────
    half = world.predators[0].DIAM / 2 if world.predators else 0
    rr = half * zoom
    for pr in world.predators:
        x, y = pr.x, pr.y
        if x + half < x0 or x - half > x1 or y + half < y0 or y - half > y1:
            continue
        sx, sy = int(x * zoom + ox), int(y * zoom + oy)
        if rr >= smooth_r:
            ir = int(rr)
            gfxdraw.filled_circle(surf, sx, sy, ir, PREDATOR_COLOR)
            gfxdraw.aacircle(surf, sx, sy, ir, PREDATOR_COLOR)
        else:
            pygame.draw.circle(surf, PREDATOR_COLOR, (sx, sy), max(1, int(round(rr))))

    if selected is not None and selected.alive:
        draw_selection(surf, selected, cam)

    surf.set_clip(old_clip)


def body_radius(creature):
    return (creature.size if isinstance(creature, Vegetarian) else creature.DIAM) / 2


def draw_selection(surf, creature, cam):
    """Кольцо вокруг выбранного существа и круг его зрения."""
    sx, sy = cam.to_screen(creature.x, creature.y)
    vision = creature.vision * cam.zoom
    if vision < 8000:                         # гигантский круг всё равно не виден
        aa_circle(surf, lerp_color(theme.WORLD_BOTTOM, ACCENT, 0.45), (sx, sy), vision,
                  filled=False)
    body = max(2, body_radius(creature) * cam.zoom)
    pygame.draw.circle(surf, ACCENT, (int(sx), int(sy)), int(body + S(5)), max(2, S(2)))


# ── графики ──────────────────────────────────────────────────────────────────

POP_SERIES = ((1, PLANT_COLOR), (2, VEGETARIAN_COLOR), (3, PREDATOR_COLOR))


def _x_positions(n, width, slots):
    """x каждой точки: окно (slots) заполняется справа налево, иначе растягиваем."""
    span = max((slots or n) - 1, 1)
    dx = width / span
    x0 = width - (n - 1) * dx
    return x0, dx


def _hover_index(hover_x, left, x0, dx, n):
    if hover_x is None or n == 0:
        return None
    i = round((hover_x - left - x0) / dx)
    return min(max(i, 0), n - 1)


def _lines(surf, color, points, thick):
    if len(points) < 2:
        return
    pygame.draw.aalines(surf, color, False, points)
    if thick:                                     # на крупном масштабе — потолще
        pygame.draw.aalines(surf, color, False, [(x, y + 1) for x, y in points])


def draw_grid(surf, rect):
    for k in (1, 2, 3):
        y = rect.top + rect.h * k // 4
        surf.fill(LINE, (rect.left, y, rect.w, 1))


def draw_population_chart(surf, rect, points, slots=None, hover_x=None):
    """Численности трёх видов; каждая линия нормирована на свой максимум.

    Хищников единицы и десятки, остальных — сотни, и на общей шкале хищники
    легли бы в ноль. А смысл графика как раз в том, чтобы видеть сдвиг фаз
    «хищник — жертва». Реальные числа — в подсказке при наведении.

    points — список app.history.Sample; slots — ширина окна в точках (для
    бегущего графика) или None. Возвращает индекс точки под курсором.
    """
    rect = pygame.Rect(rect)
    draw_grid(surf, rect)
    n = len(points)
    if n < 2:
        blit_text(surf, "small", "график появится через пару секунд", MUTED,
                  "center", center=rect.center)
        return None

    x0, dx = _x_positions(n, rect.w, slots)
    thick = theme.get_scale() >= 1.5
    h = rect.h - 2
    for k, color in POP_SERIES:
        top = max(p[k] for p in points) or 1          # все нули — не делим на ноль
        pts = [(rect.left + x0 + i * dx, rect.bottom - 1 - p[k] / top * h)
               for i, p in enumerate(points)]
        _lines(surf, color, pts, thick)

    i = _hover_index(hover_x, rect.left, x0, dx, n)
    if i is None:
        return None
    p = points[i]
    x = int(rect.left + x0 + i * dx)
    surf.fill(FAINT, (x, rect.top, 1, rect.h))
    for k, color in POP_SERIES:
        top = max(q[k] for q in points) or 1
        aa_circle(surf, color, (x, rect.bottom - 1 - p[k] / top * h), S(3))

    rows = [(f"тик {p.tick}", MUTED)] + [
        (f"{name}: {p[k]:.0f}", color)
        for (name, _), (k, color) in zip(theme.SPECIES, POP_SERIES)]
    _tooltip(surf, rect, x, rows)
    return i


def _tooltip(surf, area, x, rows):
    line_h = theme.font("small").get_linesize()
    w = max(text_width("small", s) for s, _ in rows) + 2 * S(8)
    h = len(rows) * line_h + 2 * S(6)
    box = pygame.Rect(0, 0, w, h)
    box.top = area.top + S(4)
    box.left = x + S(8) if x + S(8) + w <= area.right else x - S(8) - w
    box.left = max(box.left, area.left)
    rounded(surf, theme.BG, box, 6)
    rounded(surf, LINE, box, 6, 1)
    y = box.top + S(6)
    for s, color in rows:
        surf.blit(text("small", s, color), (box.left + S(8), y))
        y += line_h


GENE_TITLES = {
    "size":            "размер",
    "speed":           "скорость",
    "vision":          "зрение",
    "repro_threshold": "порог деления",
    "repro_share":     "доля потомку",
    "min_y":           "слой: верх",
    "max_y":           "слой: низ",
}


def format_gene(name, value):
    return f"{value:.0f}%" if name in PERCENT_GENES else f"{value:.1f}"


def draw_genome_chart(surf, rect, points, slots=None, hover_x=None, origin=None):
    """Средний геном: по мини-графику на ген, у каждого своя шкала.

    Справа — значение (под курсором или последнее) и изменение от начала
    партии. origin — исходный средний геном (History.origin): в окне
    последних тиков первая точка уже не начало партии. Без него база —
    первая точка переданного участка. Возвращает индекс точки под курсором.
    """
    rect = pygame.Rect(rect)
    valid = [p.genom for p in points if p.genom is not None]
    if len(valid) < 1:
        blit_text(surf, "small", "травоядных нет — нет и генома", MUTED,
                  "center", center=rect.center)
        return None

    names = Genom._fields
    if rect.h / len(names) < theme.font("small").get_linesize():
        blit_text(surf, "small", "мало места: закройте карточку или растяните окно",
                  MUTED, "center", center=rect.center)
        return None
    label_w = S(98)
    value_w = S(86)
    spark = pygame.Rect(rect.left + label_w, rect.top, rect.w - label_w - value_w, rect.h)
    row_h = rect.h / len(names)
    n = len(points)
    x0, dx = _x_positions(n, spark.w, slots)
    i = _hover_index(hover_x, spark.left, x0, dx, n) if spark.collidepoint(
        hover_x if hover_x is not None else -1, spark.centery) else None
    first = origin if origin is not None else valid[0]
    thick = theme.get_scale() >= 1.5

    for g, name in enumerate(names):
        top = rect.top + g * row_h
        mid = int(top + row_h / 2)
        if g:
            surf.fill(LINE, (rect.left, int(top), rect.w, 1))
        blit_text(surf, "small", GENE_TITLES[name], MUTED, "midleft",
                  midleft=(rect.left, mid))

        values = [p.genom[g] if p.genom is not None else None for p in points]
        present = [v for v in values if v is not None]
        lo, hi = min(present), max(present)
        if hi - lo < 1e-9:
            lo, hi = lo - 1, hi + 1
        pad = row_h * 0.18
        span_h = row_h - 2 * pad

        segment = []
        for k, v in enumerate(values):
            if v is None:
                _lines(surf, VEGETARIAN_COLOR, segment, thick)
                segment = []
                continue
            segment.append((spark.left + x0 + k * dx,
                            top + pad + (1 - (v - lo) / (hi - lo)) * span_h))
        _lines(surf, VEGETARIAN_COLOR, segment, thick)

        shown = values[i] if i is not None else present[-1]
        if shown is None:              # под курсором тик, когда травоядных не было
            blit_text(surf, "bodyb", "нет", MUTED, "midright",
                      midright=(rect.right - S(40), mid))
            continue
        blit_text(surf, "bodyb", format_gene(name, shown), TEXT, "midright",
                  midright=(rect.right - S(40), mid))
        if first[g]:
            change = shown / first[g] - 1
            color = MUTED if abs(change) < 0.05 else ACCENT
            label = "0%" if abs(change) < 0.005 else f"{change:+.0%}"   # без «-0%»
            blit_text(surf, "tiny", label, color, "midright",
                      midright=(rect.right, mid))

    if i is not None:                  # номер тика под курсором пишет подпись под графиком
        x = int(spark.left + x0 + i * dx)
        surf.fill(FAINT, (x, rect.top, 1, rect.h))
    return i


# ── карточка существа ────────────────────────────────────────────────────────

CARD_PAD = 12


def card_rows(creature, avg_genom):
    """Строки карточки: (подпись, значение, сравнение со средним или "")."""
    if isinstance(creature, Vegetarian):
        rows = []
        for g, (name, value) in enumerate(zip(Genom._fields, creature.genom)):
            delta = ""
            if avg_genom is not None and avg_genom[g]:
                d = value / avg_genom[g] - 1
                delta = "≈ средн." if abs(d) < 0.05 else f"{d:+.0%} к средн."
            rows.append((GENE_TITLES[name], format_gene(name, value), delta))
        return rows
    return [("скорость", f"{creature.speed:.1f}", ""),
            ("зрение",   f"{creature.vision:.0f}", ""),
            ("расход",   f"{creature.upkeep:.3f} в тик", "")]


def card_height(creature, avg_genom, compact=False):
    """Высота карточки. compact — гены в два столбца, без сравнения со средним."""
    line = theme.font("small").get_linesize() + S(4)
    rows = len(card_rows(creature, avg_genom))
    if compact:
        rows = (rows + 1) // 2
    return S(CARD_PAD) * 2 + S(34) + S(40) + rows * line


def draw_creature_card(surf, rect, creature, avg_genom, compact=False):
    """Карточка выбранного существа: вид, энергия, гены. Кнопки кладёт сцена.

    compact — для низкого окна: гены в два столбца и без сравнения со
    средним, чтобы графику над карточкой осталось место.
    """
    rect = pygame.Rect(rect)
    rounded(surf, CARD, rect, 10)
    pad = S(CARD_PAD)
    inner = rect.inflate(-2 * pad, -2 * pad)

    is_veg = isinstance(creature, Vegetarian)
    color = VEGETARIAN_COLOR if is_veg else PREDATOR_COLOR
    head_mid = inner.top + S(14)
    aa_circle(surf, color, (inner.left + S(6), head_mid), S(6))
    title = "Травоядное" if is_veg else "Хищник"
    blit_text(surf, "h2", title, TEXT, "midleft", midleft=(inner.left + S(20), head_mid))

    # энергия (справа в заголовке — кнопки сцены, поэтому состояние пишем здесь)
    y = inner.top + S(34)
    frac = max(0.0, min(1.0, creature.energy / creature.max_energy))
    label = blit_text(surf, "small", "энергия", MUTED, "topleft", topleft=(inner.left, y))
    if is_veg and creature.flee_ticks > 0:
        blit_text(surf, "small", "убегает от хищника", DANGER, "topleft",
                  topleft=(label.right + S(10), y))
    blit_text(surf, "small", f"{max(creature.energy, 0):.0f} / {creature.max_energy:.0f}",
              TEXT, "topright", topright=(inner.right, y))
    bar = pygame.Rect(inner.left, y + S(22), inner.w, S(6))
    rounded(surf, LINE, bar, 3)
    if frac > 0:
        fill = bar.copy()
        fill.w = max(bar.h, int(bar.w * frac))
        rounded(surf, DANGER if frac < 0.25 else color, fill, 3)

    # строки
    top = inner.top + S(34) + S(40)
    line = theme.font("small").get_linesize() + S(4)
    columns = 2 if compact else 1
    col_gap = S(18)
    col_w = (inner.w - (columns - 1) * col_gap) // columns
    for k, (label, value, delta) in enumerate(card_rows(creature, avg_genom)):
        col, row = (k % 2, k // 2) if compact else (0, k)
        x = inner.left + col * (col_w + col_gap)
        y = top + row * line
        blit_text(surf, "small", label, MUTED, "topleft", topleft=(x, y))
        if compact or not delta:
            blit_text(surf, "small", value, TEXT, "topright", topright=(x + col_w, y))
        else:
            blit_text(surf, "small", value, TEXT, "topright", topright=(inner.right - S(92), y))
            blit_text(surf, "tiny", delta, FAINT, "topright", topright=(inner.right, y + S(1)))


# ── всплывающие заметки ──────────────────────────────────────────────────────

TOAST_SECONDS = 5.0


def draw_toasts(surf, area, toasts, now, top=None):
    """Заметки о событиях сверху по центру вьюпорта; старые плавно гаснут.

    top — откуда начинать (ниже плашки статуса, если она есть).
    """
    area = pygame.Rect(area)
    y = area.top + S(14) if top is None else top
    for message, born in toasts:
        age = now - born
        if age > TOAST_SECONDS:
            continue
        alpha = 255 if age < TOAST_SECONDS - 1 else int(255 * (TOAST_SECONDS - age))
        img = text("body", message, TEXT)
        box = pygame.Rect(0, 0, img.get_width() + 2 * S(16), img.get_height() + 2 * S(8))
        card = pygame.Surface(box.size, pygame.SRCALPHA)
        pygame.draw.rect(card, (*CARD, 245), box, border_radius=S(8))
        pygame.draw.rect(card, (*LINE, 255), box, 1, border_radius=S(8))
        pygame.draw.rect(card, (*ACCENT, 255), (0, S(8), S(3), box.h - 2 * S(8)))
        card.blit(img, (S(16), S(8)))
        card.set_alpha(alpha)
        box.midtop = (area.centerx, y)
        surf.blit(card, box)
        y += box.h + S(8)
