# menu.py — главное меню поверх живого мира.

import pygame

from .. import render
from ..theme   import FAINT, MUTED, TEXT, S
from ..widgets import Button
from .         import Scene


class MenuScene(Scene):
    def __init__(self, app):
        super().__init__(app)
        self.continue_btn = Button("Продолжить", app.continue_game, primary=True)
        self.new_btn      = Button("Новый мир",  app.open_setup)
        self.prefs_btn    = Button("Настройки",  app.open_prefs)
        self.help_btn     = Button("Как играть", app.open_help)
        self.quit_btn     = Button("Выход",      app.quit)
        self.widgets = [self.continue_btn, self.new_btn, self.prefs_btn,
                        self.help_btn, self.quit_btn]
        self.title_top = 0

    def layout(self, size):
        super().layout(size)
        w, h = size
        has_game = self.app.session is not None
        self.continue_btn.visible = has_game
        self.new_btn.primary = not has_game

        buttons = self.active_widgets()
        bw, bh, gap = S(300), S(46), S(10)
        title_h = S(64) + S(30) + S(40)
        total = title_h + len(buttons) * bh + (len(buttons) - 1) * gap
        top = (h - total) // 2
        self.title_top = top
        y = top + title_h
        for b in buttons:
            b.rect = pygame.Rect((w - bw) // 2, y, bw, bh)
            y += bh + gap

    def update(self, dt):
        self.app.backdrop.update(dt)

    def on_event(self, event):
        if event.type == pygame.KEYDOWN:
            if event.key in (pygame.K_RETURN, pygame.K_KP_ENTER):
                if self.app.session is not None:
                    self.app.continue_game()
                else:
                    self.app.open_setup()
                return True
            if event.key == pygame.K_ESCAPE and self.app.session is not None:
                self.app.continue_game()
                return True
        return False

    def draw(self, surf):
        self.app.backdrop.draw(surf)
        w, h = self.size
        rect = render.blit_text(surf, "title", "Tiny Life", TEXT, "midtop",
                                midtop=(w // 2, self.title_top))
        render.blit_text(surf, "body", "эволюционная песочница: растения, травоядные, хищники",
                         MUTED, "midtop", midtop=(w // 2, rect.bottom + S(4)))
        self.draw_widgets(surf)
        render.blit_text(surf, "small", "Enter — начать   ·   F11 — полный экран",
                         FAINT, "midbottom", midbottom=(w // 2, h - S(18)))
