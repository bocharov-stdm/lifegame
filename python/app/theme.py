# theme.py — как выглядит интерфейс: палитра, размеры, шрифты, чёткость.
#
# Все размеры интерфейса задаются в «логических» пикселях и проходят через
# S(): при масштабе 125% кнопка в 40 px становится 50 px. Масштаб выставляет
# App — по системе или по выбору в настройках — и сам уменьшает его, если
# окно слишком мало для раскладки.

import os
import sys
from functools import lru_cache

import pygame

FRAME_RATE = 60

# Минимальный логический размер, под который рассчитана раскладка. Окно
# меньше этого не ломает её, а уменьшает масштаб интерфейса.
MIN_W, MIN_H = 960, 600

# ── палитра ──────────────────────────────────────────────────────────────────
BG           = (18, 21, 26)       # фон окна и всё, что вне мира
WORLD_TOP    = (31, 38, 47)       # мир у поверхности: светлее
WORLD_BOTTOM = (15, 18, 23)       # мир на глубине: темнее
PANEL        = (27, 31, 39)
CARD         = (35, 40, 50)
LINE         = (46, 52, 64)
TEXT         = (230, 232, 235)
MUTED        = (138, 147, 163)
FAINT        = (88, 96, 110)
ACCENT       = (245, 197, 66)
ACCENT_TEXT  = (28, 24, 12)       # текст на акцентной кнопке
DANGER       = (239, 99, 81)

BUTTON       = (43, 49, 61)
BUTTON_HOVER = (55, 62, 77)
BUTTON_DOWN  = (35, 40, 50)
PRIMARY_HOVER = (255, 212, 99)

PLANT_COLOR      = (93, 211, 158)
VEGETARIAN_COLOR = (205, 134, 255)
PREDATOR_COLOR   = (255, 110, 94)
SPECIES = (("растения", PLANT_COLOR),
           ("травоядные", VEGETARIAN_COLOR),
           ("хищники", PREDATOR_COLOR))

VEIL = (12, 14, 18, 190)          # затемнение под модальными окнами и в меню

# ── масштаб ──────────────────────────────────────────────────────────────────
_scale = 1.0


def set_scale(scale):
    global _scale
    _scale = scale


def get_scale():
    return _scale


def S(px):
    """Логические пиксели -> настоящие, по текущему масштабу."""
    return int(round(px * _scale))


# ── шрифты ───────────────────────────────────────────────────────────────────
# Segoe UI берём по имени файла: SysFont("segoeui") на Windows находит
# начертание Light, и текст выходит бледным. Если файлов нет (другая ОС) —
# ищем похожий шрифт с кириллицей, а в крайнем случае берём встроенный.
_FILES = {
    "regular":  ("segoeui.ttf",),
    "semibold": ("seguisb.ttf", "segoeuib.ttf"),
}
_FALLBACK = "dejavusans,notosans,liberationsans,helveticaneue,arial"

# роль -> (начертание, логический кегль)
ROLES = {
    "title": ("semibold", 46),
    "h1":    ("semibold", 26),
    "h2":    ("semibold", 17),
    "body":  ("regular",  15),
    "bodyb": ("semibold", 15),
    "small": ("regular",  13),
    "tiny":  ("regular",  12),
    "big":   ("semibold", 22),     # крупные числа в панели
}


@lru_cache(maxsize=None)
def _font_file(weight):
    fonts_dir = os.path.join(os.environ.get("WINDIR", r"C:\Windows"), "Fonts")
    for name in _FILES[weight]:
        path = os.path.join(fonts_dir, name)
        if os.path.exists(path):
            return path
    return pygame.font.match_font(_FALLBACK, bold=weight == "semibold")


@lru_cache(maxsize=64)
def _font(weight, size):
    if not pygame.font.get_init():
        pygame.font.init()
    path = _font_file(weight)
    try:
        return pygame.font.Font(path, size)
    except (OSError, pygame.error):
        return pygame.font.Font(None, int(size * 1.3))   # у встроенного кегль мельче


def font(role):
    weight, size = ROLES[role]
    return _font(weight, max(8, S(size)))


# ── окно ─────────────────────────────────────────────────────────────────────

def enable_dpi_awareness():
    """Windows: рисовать в настоящих пикселях, без размытия. Возвращает масштаб системы.

    Без этого при масштабе экрана 125% Windows растягивает готовую картинку,
    и текст мылится. С ним окно получает честные пиксели, а крупность
    интерфейса обеспечивает S().
    """
    if sys.platform != "win32":
        return 1.0
    try:
        import ctypes
        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(1)     # по системному DPI
        except (AttributeError, OSError):
            ctypes.windll.user32.SetProcessDPIAware()
        return ctypes.windll.user32.GetDpiForSystem() / 96
    except (AttributeError, OSError):
        return 1.0


def initial_window_size(scale):
    """Стартовый размер: 1440x880 логических, но не больше 90% экрана."""
    try:
        desk_w, desk_h = pygame.display.get_desktop_sizes()[0]
    except (pygame.error, IndexError):
        desk_w, desk_h = 1920, 1080
    return (min(int(1440 * scale), int(desk_w * 0.92)),
            min(int(880 * scale), int(desk_h * 0.86)))


def window_icon():
    """Значок окна: три кружка — растение, травоядное, хищник."""
    icon = pygame.Surface((32, 32), pygame.SRCALPHA)
    pygame.draw.circle(icon, PLANT_COLOR,      (9, 22), 5)
    pygame.draw.circle(icon, VEGETARIAN_COLOR, (20, 11), 8)
    pygame.draw.circle(icon, PREDATOR_COLOR,   (24, 24), 6)
    return icon


def lerp_color(a, b, t):
    return tuple(int(round(x + (y - x) * t)) for x, y in zip(a, b))
