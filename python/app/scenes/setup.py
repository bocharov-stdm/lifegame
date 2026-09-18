# setup.py — экран «Новый мир»: стартовые условия и лаборатория правил.

import math
from functools import partial

import pygame

from .. import render
from ..settings import FIELDS, SEED_MAX
from ..theme    import ACCENT, LINE, MUTED, TEXT, S
from ..widgets  import Button, NumberField, Segmented, Slider, Toggle
from .          import PanelScene, title_block

SUBTITLES = {
    "world": ("С чего начинается мир. Правила игры — на вкладке «Лаборатория».", MUTED),
    "lab":   ("Правила мира. Опыты могут сломать баланс — в этом и смысл.", ACCENT),
}
DEFAULT_HINT = "Наведите на параметр, чтобы прочитать, что он меняет."
SEED_HINT = ("Одинаковый сид и настройки дают один и тот же мир — "
             "его можно повторить, выключив новый сид при каждом старте.")


class SetupScene(PanelScene):
    def __init__(self, app):
        super().__init__(app)
        s = app.settings
        self.tab = "world"

        self.tabs = Segmented([("world", "Мир"), ("lab", "Лаборатория")],
                              lambda: self.tab, self._set_tab)
        self.seed_field = NumberField(lambda: s.seed, partial(setattr, s, "seed"),
                                      1, SEED_MAX)
        self.dice = Button("", self._roll, icon="dice")
        self.random_seed = Toggle("Новый сид при каждом старте",
                                  lambda: s.random_seed, partial(setattr, s, "random_seed"))
        self.sliders = [Slider(f, partial(getattr, s, f.key), partial(setattr, s, f.key),
                               partial(s.is_default, f.key))
                        for f in FIELDS]

        self.reset_btn = Button("По умолчанию", lambda: s.reset(self.tab))
        self.back_btn  = Button("Назад", app.back)
        self.start_btn = Button("Начать", app.start_game, primary=True)

        # Поле сида — первым: клик мимо него завершает ввод, и только потом
        # событие достаётся остальным (вкладкам, «Начать»).
        self.widgets = ([self.seed_field, self.tabs, self.dice, self.random_seed]
                        + self.sliders + [self.reset_btn, self.back_btn, self.start_btn])
        self.hint_rect = pygame.Rect(0, 0, 0, 0)
        self.seed_label_pos = (0, 0)

    def _set_tab(self, tab):
        self.seed_field.commit()           # скрытое поле не должно унести введённый сид
        self.tab = tab
        self.layout(self.size)

    def _roll(self):
        self.app.settings.roll_seed()

    # ── раскладка ───────────────────────────────────────────────────────────
    def layout(self, size):
        super().layout(size)
        w, h = size
        pad = S(28)
        panel_w = min(S(840), w - 2 * S(24))
        inner_w = panel_w - 2 * pad
        columns = 2 if inner_w >= S(600) else 1
        col_gap = S(32)
        col_w = (inner_w - (columns - 1) * col_gap) // columns
        slider_h, row_gap = S(54), S(8)

        per_tab = {t: [s for s in self.sliders if s.field.tab == t] for t in ("world", "lab")}
        rows = {t: math.ceil(len(v) / columns) for t, v in per_tab.items()}
        seed_h = S(44) + S(14)
        content_h = max(seed_h + rows["world"] * (slider_h + row_gap),
                        rows["lab"] * (slider_h + row_gap))
        head_h = S(34) + S(26) + S(22)
        hint_h = S(44)
        buttons_h = S(44)
        panel_h = pad + head_h + content_h + hint_h + S(18) + buttons_h + pad
        self.panel = self.centered(size, panel_w, panel_h)
        left, top = self.panel.left + pad, self.panel.top + pad

        self.tabs.rect = pygame.Rect(self.panel.right - pad - S(270), top - S(2), S(270), S(38))

        y = top + head_h
        on_world = self.tab == "world"
        for widget in (self.seed_field, self.dice, self.random_seed):
            widget.visible = on_world
        if on_world:
            self.seed_label_pos = (left + S(8), y + S(22))
            label_w = render.text_width("body", "Сид мира") + S(24)
            self.seed_field.rect = pygame.Rect(left + label_w, y, S(110), S(44))
            self.dice.rect = pygame.Rect(self.seed_field.rect.right + S(8), y, S(44), S(44))
            toggle_left = self.dice.rect.right + S(24)
            self.random_seed.rect = pygame.Rect(toggle_left, y,
                                                left + inner_w - toggle_left, S(44))
            y += seed_h

        for tab, sliders in per_tab.items():
            for i, slider in enumerate(sliders):
                slider.visible = tab == self.tab
                col, row = i % columns, i // columns
                slider.rect = pygame.Rect(left + col * (col_w + col_gap),
                                          y + row * (slider_h + row_gap),
                                          col_w, slider_h)

        bottom = self.panel.bottom - pad
        self.start_btn.rect = pygame.Rect(0, 0, S(160), buttons_h)
        self.start_btn.rect.bottomright = (self.panel.right - pad, bottom)
        self.back_btn.rect = pygame.Rect(0, 0, S(120), buttons_h)
        self.back_btn.rect.bottomright = (self.start_btn.rect.left - S(10), bottom)
        self.reset_btn.rect = pygame.Rect(left, bottom - buttons_h, S(170), buttons_h)
        self.hint_rect = pygame.Rect(left, bottom - buttons_h - S(18) - hint_h,
                                     inner_w, hint_h)

    # ── события ─────────────────────────────────────────────────────────────
    def on_event(self, event):
        if event.type == pygame.KEYDOWN:
            if event.key in (pygame.K_RETURN, pygame.K_KP_ENTER):
                self.app.start_game()
                return True
            if event.key == pygame.K_ESCAPE:
                self.app.back()
                return True
            if event.key == pygame.K_TAB:
                self._set_tab("lab" if self.tab == "world" else "world")
                return True
        return False

    # ── кадр ────────────────────────────────────────────────────────────────
    def _hint(self):
        mouse = self.app.mouse
        for slider in self.sliders:
            if slider.visible and (slider.rect.collidepoint(mouse) or slider.dragging):
                return slider.field.hint
        if self.tab == "world" and any(
                w.rect.collidepoint(mouse) for w in (self.seed_field, self.dice, self.random_seed)):
            return SEED_HINT
        return DEFAULT_HINT

    def draw_content(self, surf):
        subtitle, color = SUBTITLES[self.tab]
        title_block(surf, self.panel, "Новый мир", subtitle, color)
        if self.tab == "world":
            render.blit_text(surf, "body", "Сид мира", MUTED, "midleft",
                             midleft=self.seed_label_pos)

        r = self.hint_rect
        surf.fill(LINE, (r.left, r.top - S(1), r.w, 1))
        y = r.top + S(8)
        for line in render.wrap("small", self._hint(), r.w)[:2]:
            rect = render.blit_text(surf, "small", line, TEXT, "topleft", topleft=(r.left, y))
            y = rect.bottom
