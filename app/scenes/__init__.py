# scenes/ — экраны игры: меню, новый мир, настройки, справка, сама игра.
#
# Сцена — это layout(size) (расставить виджеты под размер окна), handle(event)
# (нажатия), update(dt) (время) и draw(surf) (кадр). Переключает сцены App.

import pygame

from life.config import WORLD_HEIGHT, WORLD_WIDTH

from .. import render, theme
from ..camera   import Camera
from ..session  import Session
from ..settings import Settings
from ..theme    import LINE, PANEL, S


class Scene:
    def __init__(self, app):
        self.app = app
        self.widgets = []
        self.size = (0, 0)

    def layout(self, size):
        self.size = size

    def active_widgets(self):
        """Видимые виджеты — те, что получают события и проверяются на раскладку."""
        return [w for w in self.widgets if w.visible]

    def handle(self, event):
        mouse = self.app.mouse
        for w in self.active_widgets():
            if w.handle(event, mouse):
                return True
        return self.on_event(event)

    def on_event(self, event):
        return False

    def update(self, dt):
        pass

    def draw(self, surf):
        self.draw_widgets(surf)

    def draw_widgets(self, surf):
        for w in self.active_widgets():
            w.draw(surf, self.app.mouse)


class Backdrop:
    """Живой мир за меню: идёт сам по себе, приглушённый вуалью.

    Своя сессия со своим генератором (см. session.py), поэтому фон не
    влияет на партию, стоящую на паузе.
    """

    def __init__(self, app):
        self.app = app
        self.session = None
        self.cam = None
        self._renew()

    def _renew(self):
        settings = Settings()
        settings.roll_seed()
        self.session = Session(settings)
        self.session.set_speed_index(1)
        if self.cam is not None:
            self.layout(self.cam.view[2:])

    def layout(self, size):
        w, h = size
        self.cam = Camera(WORLD_WIDTH, WORLD_HEIGHT, (0, 0, w, h))
        # мир заполняет окно целиком, лишнее по краям обрезается
        cover = max(w / WORLD_WIDTH, h / WORLD_HEIGHT)
        self.cam.zoom_at(w / 2, h / 2, cover / self.cam.zoom)

    def update(self, dt):
        self.session.advance(dt, budget=0.004)
        if self.session.ended is not None:
            self._renew()

    def draw(self, surf):
        render.draw_world(surf, self.session.world, self.cam,
                          smooth=self.app.settings.smooth)
        render.veil(surf)


class PanelScene(Scene):
    """Экран-карточка по центру поверх живого фона (новый мир, настройки, справка)."""

    def __init__(self, app):
        super().__init__(app)
        self.panel = pygame.Rect(0, 0, 0, 0)

    def update(self, dt):
        self.app.backdrop.update(dt)

    def draw(self, surf):
        self.app.backdrop.draw(surf)
        render.shadow(surf, self.panel, 14)
        render.rounded(surf, PANEL, self.panel, 14)
        render.rounded(surf, LINE, self.panel, 14, 1)
        self.draw_content(surf)
        self.draw_widgets(surf)

    def draw_content(self, surf):
        pass

    @staticmethod
    def centered(size, w, h):
        rect = pygame.Rect(0, 0, w, h)
        rect.center = (size[0] // 2, size[1] // 2)
        return rect


def title_block(surf, panel, title, subtitle=None, subtitle_color=None):
    """Заголовок карточки и подзаголовок под ним; возвращает низ блока."""
    pad = S(28)
    rect = render.blit_text(surf, "h1", title, theme.TEXT, "topleft",
                            topleft=(panel.left + pad, panel.top + pad - S(4)))
    bottom = rect.bottom
    if subtitle:
        rect = render.blit_text(surf, "body", subtitle, subtitle_color or theme.MUTED,
                                "topleft", topleft=(panel.left + pad, bottom + S(2)))
        bottom = rect.bottom
    return bottom
