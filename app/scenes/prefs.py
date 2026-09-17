# prefs.py — настройки экрана: полный экран, масштаб, сглаживание, FPS.

from functools import partial

import pygame

from .. import render
from ..settings import UI_SCALES
from ..theme    import MUTED, TEXT, S
from ..widgets  import Button, Segmented, Toggle
from .          import PanelScene, title_block


class PrefsScene(PanelScene):
    def __init__(self, app):
        super().__init__(app)
        s = app.settings
        self.fullscreen = Toggle("Полный экран (F11)", lambda: s.fullscreen, app.set_fullscreen)
        self.smooth = Toggle("Сглаживать существ", lambda: s.smooth,
                             partial(setattr, s, "smooth"))
        self.show_fps = Toggle("Показывать FPS", lambda: s.show_fps,
                               partial(setattr, s, "show_fps"))
        self.scale = Segmented([(v, "Авто" if not v else f"{v:.0%}") for v in UI_SCALES],
                               lambda: s.ui_scale, self._set_scale, role="small")
        self.done_btn = Button("Готово", app.back, primary=True)
        self.widgets = [self.fullscreen, self.smooth, self.show_fps, self.scale, self.done_btn]
        self.scale_label_y = self.scale_note_y = 0

    def _set_scale(self, value):
        self.app.settings.ui_scale = value
        self.app.layout()                       # новый масштаб — новая раскладка

    def layout(self, size):
        super().layout(size)
        pad = S(28)
        panel_w = min(S(560), size[0] - 2 * S(24))
        inner_w = panel_w - 2 * pad
        row_h = S(44)
        panel_h = pad + S(62) + 3 * row_h + S(16) + S(28) + S(40) + S(40) + S(24) + S(44) + pad
        self.panel = self.centered(size, panel_w, panel_h)
        left = self.panel.left + pad
        y = self.panel.top + pad + S(62)
        for toggle in (self.fullscreen, self.smooth, self.show_fps):
            toggle.rect = pygame.Rect(left - S(8), y, inner_w + S(16), row_h)
            y += row_h
        y += S(16)
        self.scale_label_y = y
        y += S(28)
        self.scale.rect = pygame.Rect(left, y, inner_w, S(40))
        self.scale_note_y = y + S(40) + S(8)
        self.done_btn.rect = pygame.Rect(0, 0, S(140), S(44))
        self.done_btn.rect.bottomright = (self.panel.right - pad, self.panel.bottom - pad)

    def on_event(self, event):
        if event.type == pygame.KEYDOWN and event.key in (
                pygame.K_ESCAPE, pygame.K_RETURN, pygame.K_KP_ENTER):
            self.app.back()
            return True
        return False

    def draw_content(self, surf):
        title_block(surf, self.panel, "Настройки", "Запоминаются между запусками.")
        left = self.panel.left + S(28)
        render.blit_text(surf, "body", "Масштаб интерфейса", TEXT, "topleft",
                         topleft=(left, self.scale_label_y))
        app = self.app
        note = f"Авто — как в системе: {app.system_scale:.0%}."
        wanted = app.settings.ui_scale or app.system_scale
        if app.effective_scale < wanted - 1e-6:
            note += f" Окно маловато — сейчас {app.effective_scale:.0%}."
        render.blit_text(surf, "small", note, MUTED, "topleft",
                         topleft=(left, self.scale_note_y))
