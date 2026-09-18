# help.py — экран «Как играть»: управление и коротко о мире.

import pygame

from .. import render
from ..theme   import LINE, MUTED, TEXT, S
from ..widgets import Button
from .         import PanelScene, title_block

# клавиши словами: у шрифтов pygame нет стрелок
KEYS = (
    ("Пробел",               "пауза и продолжение"),
    ("Вправо",               "один тик на паузе"),
    ("+  и  -",              "быстрее и медленнее"),
    ("1 … 6",                "скорость от x1 до x32"),
    ("Колесо мыши",          "приблизить к курсору"),
    ("Перетаскивание, WASD", "сдвинуть вид"),
    ("Home",                 "показать весь мир"),
    ("Клик по существу",     "его карточка"),
    ("F",                    "следить за выбранным"),
    ("G",                    "популяции или геном"),
    ("Tab",                  "скрыть панель"),
    ("F11",                  "полный экран"),
    ("Esc",                  "меню паузы"),
)

WORLD = (
    "Растения появляются сами, гуще всего у поверхности — вверху мира.",
    "Травоядные едят растения, хищники — травоядных. Размер, скорость и "
    "зрение стоят энергии каждый тик; у кого она кончилась, тот погиб.",
    "Сытое травоядное делится, и гены потомка немного мутируют. Среди генов "
    "есть слой — полоса глубины, где существо живёт и ест.",
    "Отбор никто не программировал: выживают те, чей геном лучше подходит "
    "миру. Куда он движется, видно на вкладке «Геном».",
    "Травоядных много — хищники плодятся и выедают их — хищники голодают — "
    "травоядные возвращаются. На графике популяций пики хищников "
    "запаздывают за пиками травоядных.",
)


class HelpScene(PanelScene):
    def __init__(self, app):
        super().__init__(app)
        self.back_btn = Button("Назад", app.back, primary=True)
        self.widgets = [self.back_btn]
        self.columns = (pygame.Rect(0, 0, 0, 0), pygame.Rect(0, 0, 0, 0))

    def layout(self, size):
        super().layout(size)
        pad = S(28)
        panel_w = min(S(940), size[0] - 2 * S(24))
        inner_w = panel_w - 2 * pad
        line = render.theme.font("small").get_linesize()
        keys_h = len(KEYS) * (line + S(6))
        panel_h = min(size[1] - 2 * S(16),
                      pad + S(62) + S(30) + keys_h + S(20) + S(44) + pad)
        self.panel = self.centered(size, panel_w, panel_h)
        top = self.panel.top + pad + S(62)
        left_w = S(380)
        gap = S(40)
        height = self.panel.bottom - pad - S(44) - S(20) - top
        self.columns = (pygame.Rect(self.panel.left + pad, top, left_w, height),
                        pygame.Rect(self.panel.left + pad + left_w + gap, top,
                                    inner_w - left_w - gap, height))
        self.back_btn.rect = pygame.Rect(0, 0, S(140), S(44))
        self.back_btn.rect.bottomright = (self.panel.right - pad, self.panel.bottom - pad)

    def on_event(self, event):
        if event.type == pygame.KEYDOWN and event.key in (
                pygame.K_ESCAPE, pygame.K_RETURN, pygame.K_KP_ENTER):
            self.app.back()
            return True
        return False

    def draw_content(self, surf):
        title_block(surf, self.panel, "Как играть",
                    "Наблюдайте, как эволюция находит равновесие сама.")
        keys, world = self.columns
        line = render.theme.font("small").get_linesize()

        render.blit_text(surf, "h2", "Управление", TEXT, "topleft", topleft=keys.topleft)
        y = keys.top + S(30)
        for key, action in KEYS:
            render.blit_text(surf, "small", key, TEXT, "topleft", topleft=(keys.left, y))
            render.blit_text(surf, "small", action, MUTED, "topleft",
                             topleft=(keys.left + S(170), y))
            y += line + S(6)

        surf.fill(LINE, (world.left - S(20), world.top, 1, world.h))
        render.blit_text(surf, "h2", "Как устроен мир", TEXT, "topleft", topleft=world.topleft)
        y = world.top + S(30)
        for paragraph in WORLD:
            for text_line in render.wrap("small", paragraph, world.w):
                if y + line > world.bottom:
                    return
                render.blit_text(surf, "small", text_line, MUTED, "topleft",
                                 topleft=(world.left, y))
                y += line
            y += S(8)
