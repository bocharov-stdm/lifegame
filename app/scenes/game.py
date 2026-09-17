# game.py — сама игра: мир слева, панель справа, модальные окна поверх.
#
# Логику партии ведёт app.session.Session; здесь только то, что про экран:
# камера, выбор существа, графики, заметки о событиях, меню паузы и
# карточка конца игры.

import math
import time

import pygame

from life.config import WORLD_HEIGHT, WORLD_WIDTH

from .. import render, theme
from ..camera  import Camera
from ..session import EXPLOSION_LIMIT, SPEEDS, spaced
from ..history import GRAPH_EVERY, RECENT_POINTS
from ..theme   import (ACCENT, ACCENT_TEXT, DANGER, FAINT, LINE, MUTED, PANEL,
                       SPECIES, TEXT, S)
from ..widgets import Button, Segmented
from .         import Scene

PICK_RADIUS_PX = 10          # насколько можно промахнуться кликом, в пикселях экрана
DRAG_THRESHOLD = 5           # сдвиг мыши, после которого клик становится перетаскиванием
PAN_SPEED      = 900         # пикселей в секунду для WASD
ZOOM_STEP      = 1.2         # за одно деление колеса
MIN_CHART_H    = 200         # ниже этого карточка существа ужимается ради графика

SPEED_KEYS = {getattr(pygame, f"K_{i + 1}"): i for i in range(len(SPEEDS))}
PAN_KEYS = {pygame.K_w: (0, 1), pygame.K_s: (0, -1),
            pygame.K_a: (1, 0), pygame.K_d: (-1, 0)}


class Overlay:
    """Модальная карточка по центру: заголовок, текст и столбик кнопок."""

    def __init__(self, kind, title, lines, buttons, on_escape):
        self.kind = kind                   # "pause" | "extinct" | "explosion"
        self.title = title
        self.lines = lines
        self.buttons = [Button(label, callback, primary=primary)
                        for label, callback, primary in buttons]
        self.on_escape = on_escape
        self.card = pygame.Rect(0, 0, 0, 0)

    def layout(self, size):
        pad = S(28)
        w = min(S(420), size[0] - 2 * S(24))
        text_h = sum(len(render.wrap("body", line, w - 2 * pad)) for line in self.lines) \
            * theme.font("body").get_linesize()
        bh, gap = S(44), S(8)
        h = (pad + S(38) + text_h + S(20)
             + len(self.buttons) * bh + (len(self.buttons) - 1) * gap + pad)
        self.card = pygame.Rect(0, 0, w, h)
        self.card.center = (size[0] // 2, size[1] // 2)
        y = self.card.bottom - pad - len(self.buttons) * bh - (len(self.buttons) - 1) * gap
        for b in self.buttons:
            b.rect = pygame.Rect(self.card.left + pad, y, w - 2 * pad, bh)
            y += bh + gap

    def handle(self, event, mouse):
        for b in self.buttons:
            if b.handle(event, mouse):
                return True
        if event.type == pygame.KEYDOWN and event.key == pygame.K_ESCAPE:
            self.on_escape()
        return True                        # модальное: мимо карточки ничего не проходит

    def draw(self, surf, mouse):
        render.veil(surf)
        render.shadow(surf, self.card, 14)
        render.rounded(surf, PANEL, self.card, 14)
        render.rounded(surf, LINE, self.card, 14, 1)
        pad = S(28)
        x, y = self.card.left + pad, self.card.top + pad - S(4)
        rect = render.blit_text(surf, "h1", self.title, TEXT, "topleft", topleft=(x, y))
        y = rect.bottom + S(6)
        for line in self.lines:
            for part in render.wrap("body", line, self.card.w - 2 * pad):
                y = render.blit_text(surf, "body", part, MUTED, "topleft",
                                     topleft=(x, y)).bottom
        for b in self.buttons:
            b.draw(surf, mouse)


class GameScene(Scene):
    SIDEBAR_W = 340

    def __init__(self, app, session):
        super().__init__(app)
        self.session = session
        self.cam = Camera(WORLD_WIDTH, WORLD_HEIGHT, (0, 0, 1, 1))
        self._cam_ready = False
        self.selected = None
        self.show_sidebar = True
        self.chart = "pop"                     # "pop" | "genome"
        self.whole = False                     # весь график партии или окно
        self.toasts = []                       # (текст, время появления)
        self.events_seen = 0
        self.overlay = None
        self.end_shown = None                  # для какого конца карточка уже была
        self.drag = None                       # {"start", "last", "moved"}
        self.pan_keys = set()
        self.clock = time.perf_counter

        self.play_btn   = Button("", self.toggle_pause, icon="pause")
        self.step_btn   = Button("", self.step, icon="step")
        self.slower_btn = Button("", session.slower, icon="minus")
        self.faster_btn = Button("", session.faster, icon="plus")
        self.fit_btn    = Button("", self.fit, icon="fit")
        self.menu_btn   = Button("", self.open_pause, icon="menu")
        self.chart_tabs = Segmented([("pop", "Популяции"), ("genome", "Геном")],
                                    lambda: self.chart, self._set_chart)
        self.range_tabs = Segmented([(False, "3000"), (True, "всё")],
                                    lambda: self.whole, self._set_whole, role="small")
        self.follow_btn = Button("Следить", self.toggle_follow, icon="follow", role="small")
        self.close_btn  = Button("", self.deselect, icon="close")
        self.widgets = [self.play_btn, self.step_btn, self.slower_btn, self.faster_btn,
                        self.fit_btn, self.menu_btn, self.chart_tabs, self.range_tabs,
                        self.follow_btn, self.close_btn]

        self.sidebar = pygame.Rect(0, 0, 0, 0)
        self.speed_label = pygame.Rect(0, 0, 0, 0)
        self.counters_top = 0
        self.chart_rect = pygame.Rect(0, 0, 0, 0)
        self.card_rect = pygame.Rect(0, 0, 0, 0)
        self.compact_card = False
        self._chart_key = None
        self._chart_surf = None
        self._chart_hover = None               # тик под курсором на графике

    # ── раскладка ───────────────────────────────────────────────────────────
    def layout(self, size):
        super().layout(size)
        w, h = size
        sw = min(S(self.SIDEBAR_W), int(w * 0.45)) if self.show_sidebar else 0
        view = (0, 0, w - sw, h)
        if self._cam_ready:
            self.cam.set_view(view)
        else:
            self.cam.set_view(view)
            self.cam.fit()
            self._cam_ready = True
        self.sidebar = pygame.Rect(w - sw, 0, sw, h)

        for widget in self.widgets:
            widget.visible = self.show_sidebar
        if not self.show_sidebar:
            if self.overlay:
                self.overlay.layout(size)
            return

        pad = S(18)
        left, inner_w = self.sidebar.left + pad, sw - 2 * pad
        right = left + inner_w
        y = pad + S(34)                                 # под заголовком

        b, gap = S(38), S(6)
        x = left
        for btn in (self.play_btn, self.step_btn):
            btn.rect = pygame.Rect(x, y, b, b)
            x += b + gap
        x += S(4)
        self.slower_btn.rect = pygame.Rect(x, y, b, b)
        self.speed_label = pygame.Rect(self.slower_btn.rect.right, y, S(44), b)
        self.faster_btn.rect = pygame.Rect(self.speed_label.right, y, b, b)
        self.menu_btn.rect = pygame.Rect(right - b, y, b, b)
        self.fit_btn.rect = pygame.Rect(self.menu_btn.rect.left - gap - b, y, b, b)
        y += b + S(16)

        self.counters_top = y
        y += S(46) + S(16)

        range_w = S(96)
        self.chart_tabs.rect = pygame.Rect(left, y, inner_w - range_w - S(8), S(34))
        self.range_tabs.rect = pygame.Rect(right - range_w, y, range_w, S(34))
        y += S(34) + S(12)

        footer_h = S(30)
        caption_h = S(20)                               # подпись под графиком
        chart_room = h - footer_h - S(12) - caption_h - y   # график и карточка вместе
        self.compact_card = False
        if self.selected is not None:
            avg = self._avg_genom()
            card_h = render.card_height(self.selected, avg)
            if chart_room - card_h < S(MIN_CHART_H):    # низкое окно — карточка плотнее
                self.compact_card = True
                card_h = render.card_height(self.selected, avg, compact=True)
        else:
            card_h = S(58)
        self.card_rect = pygame.Rect(left, h - footer_h - card_h, inner_w, card_h)
        self.chart_rect = pygame.Rect(left, y, inner_w,
                                      self.card_rect.top - S(12) - caption_h - y)

        card_pad = S(render.CARD_PAD)
        self.close_btn.rect = pygame.Rect(0, 0, S(28), S(28))
        self.close_btn.rect.topright = (self.card_rect.right - card_pad + S(4),
                                        self.card_rect.top + card_pad)
        self.follow_btn.rect = pygame.Rect(0, 0, S(104), S(28))
        self.follow_btn.rect.topright = (self.close_btn.rect.left - S(6),
                                         self.card_rect.top + card_pad)
        has_card = self.selected is not None
        self.close_btn.visible = self.follow_btn.visible = has_card

        if self.overlay:
            self.overlay.layout(size)

    def _avg_genom(self):
        last = self.session.history.last
        return last.genom if last else None

    # ── действия ────────────────────────────────────────────────────────────
    def toggle_pause(self):
        if self.session.ended is not None:
            self.show_end()
        else:
            self.session.toggle_pause()

    def step(self):
        self.session.paused = True
        self.session.step_once()

    def fit(self):
        self.cam.fit()

    def select(self, creature):
        if creature is self.selected:
            return
        if self.cam.target is not None and creature is not self.cam.target:
            self.cam.target = None
        self.selected = creature
        self.layout(self.size)

    def deselect(self):
        self.select(None)

    def toggle_follow(self):
        if self.selected is None:
            return
        if self.cam.target is self.selected:
            self.cam.target = None
        else:
            self.cam.follow(self.selected)

    def toggle_sidebar(self):
        self.show_sidebar = not self.show_sidebar
        self.layout(self.size)

    def _set_chart(self, chart):
        self.chart = chart

    def _set_whole(self, whole):
        self.whole = whole

    def toast(self, message):
        self.toasts.append((message, self.clock()))

    # ── модальные окна ──────────────────────────────────────────────────────
    def _open(self, overlay):
        self.overlay = overlay
        self.pan_keys.clear()
        self.drag = None
        overlay.layout(self.size)

    def close_overlay(self):
        self.overlay = None

    def open_pause(self):
        s = self.session
        app = self.app
        self._open(Overlay(
            "pause", "Пауза",
            [f"Мир #{s.seed}, тик {spaced(s.world.tick)}."],
            [("Продолжить", self.close_overlay, True),
             ("Заново — тот же мир", app.restart_game, False),
             ("Новый мир…", app.open_setup, False),
             ("Настройки", app.open_prefs, False),
             ("Главное меню", app.open_menu, False),
             ("Выход", app.quit, False)],
            on_escape=self.close_overlay))

    def show_end(self):
        s = self.session
        app = self.app
        plants, veg, pred = s.peaks
        tick = spaced(s.world.tick)
        if s.ended == "extinct":
            overlay = Overlay(
                "extinct", "Жизнь угасла",
                [f"Последнее существо погибло на тике {tick}.",
                 f"Пики: травоядных {veg}, хищников {pred}, растений {plants}."],
                [("Заново — тот же мир", app.restart_game, True),
                 ("Новый мир…", app.open_setup, False),
                 ("Посмотреть графики", self.close_overlay, False),
                 ("Главное меню", app.open_menu, False)],
                on_escape=self.close_overlay)
        else:
            overlay = Overlay(
                "explosion", "Взрыв численности",
                [f"Больше {spaced(EXPLOSION_LIMIT)} существ на тике {tick}. "
                 "Дальше окно начнёт тормозить."],
                [("Продолжить всё равно", self._keep_going, True),
                 ("Заново — тот же мир", app.restart_game, False),
                 ("Новый мир…", app.open_setup, False),
                 ("Главное меню", app.open_menu, False)],
                on_escape=self.close_overlay)
        self.end_shown = s.ended
        self._open(overlay)

    def _keep_going(self):
        self.session.keep_going()
        self.end_shown = None
        self.close_overlay()

    # ── события ─────────────────────────────────────────────────────────────
    def handle(self, event):
        if self.overlay is not None:
            return self.overlay.handle(event, self.app.mouse)
        if super().handle(event):
            return True
        return False

    def on_event(self, event):
        s = self.session
        t = event.type
        if t == pygame.KEYDOWN:
            key = event.key
            if key == pygame.K_SPACE:
                self.toggle_pause()
            elif key == pygame.K_RIGHT:
                if s.paused:
                    s.step_once()
            elif key in (pygame.K_PLUS, pygame.K_EQUALS, pygame.K_KP_PLUS):
                s.faster()
            elif key in (pygame.K_MINUS, pygame.K_KP_MINUS):
                s.slower()
            elif key in SPEED_KEYS:
                s.set_speed_index(SPEED_KEYS[key])
            elif key == pygame.K_f:
                self.toggle_follow()
            elif key == pygame.K_HOME:
                self.fit()
            elif key == pygame.K_g:
                self.chart = "genome" if self.chart == "pop" else "pop"
            elif key == pygame.K_TAB:
                self.toggle_sidebar()
            elif key == pygame.K_ESCAPE:
                self.open_pause()
            elif key in PAN_KEYS:
                self.pan_keys.add(key)
            else:
                return False
            return True
        if t == pygame.KEYUP and event.key in PAN_KEYS:
            self.pan_keys.discard(event.key)
            return True

        if t == pygame.MOUSEBUTTONDOWN and self.cam.contains(*event.pos):
            if event.button in (1, 2):
                self.drag = {"start": event.pos, "last": event.pos, "moved": False,
                             "button": event.button}
            elif event.button == 3:
                self.deselect()
            return True
        if t == pygame.MOUSEMOTION and self.drag is not None:
            sx, sy = self.drag["start"]
            if not self.drag["moved"] and math.hypot(event.pos[0] - sx,
                                                     event.pos[1] - sy) > S(DRAG_THRESHOLD):
                self.drag["moved"] = True
            if self.drag["moved"]:
                lx, ly = self.drag["last"]
                self.cam.pan(event.pos[0] - lx, event.pos[1] - ly)
            self.drag["last"] = event.pos
            return True
        if t == pygame.MOUSEBUTTONUP and self.drag is not None \
                and event.button == self.drag["button"]:
            if not self.drag["moved"] and event.button == 1:
                wx, wy = self.cam.to_world(*event.pos)
                self.select(s.pick(wx, wy, S(PICK_RADIUS_PX) / self.cam.zoom))
            self.drag = None
            return True
        if t == pygame.MOUSEWHEEL and self.cam.contains(*self.app.mouse):
            self.cam.zoom_at(*self.app.mouse, ZOOM_STEP ** event.y)
            return True
        return False

    # ── время ───────────────────────────────────────────────────────────────
    def update(self, dt):
        s = self.session
        if self.overlay is None:
            s.advance(dt)

            dx = sum(PAN_KEYS[k][0] for k in self.pan_keys)
            dy = sum(PAN_KEYS[k][1] for k in self.pan_keys)
            if dx or dy:
                self.cam.pan(dx * S(PAN_SPEED) * dt, dy * S(PAN_SPEED) * dt)
        self.cam.update(dt)

        if self.selected is not None and not self.selected.alive:
            # хищник, съев травоядное, обнуляет ему энергию; от голода она уходит в минус
            eaten = self.selected.energy == 0 and hasattr(self.selected, "size")
            self.toast("Выбранное существо съели" if eaten
                       else "Выбранное существо умерло от голода")
            self.deselect()

        for event in s.events[self.events_seen:]:
            self.toast(event.text)
        self.events_seen = len(s.events)

        if s.ended is not None and s.ended != self.end_shown and self.overlay is None:
            self.show_end()

        now = self.clock()
        self.toasts = [t for t in self.toasts if now - t[1] < render.TOAST_SECONDS][-4:]

        self.play_btn.icon = "play" if s.paused or s.ended else "pause"
        self.follow_btn.active = self.selected is not None and self.cam.target is self.selected

    # ── кадр ────────────────────────────────────────────────────────────────
    def draw(self, surf):
        s = self.session
        render.draw_world(surf, s.world, self.cam, self.selected,
                          smooth=self.app.settings.smooth)
        view = pygame.Rect(self.cam.view)
        chip_bottom = self._draw_view_badges(surf, view)
        render.draw_toasts(surf, view, self.toasts, self.clock(),
                           top=None if chip_bottom is None else chip_bottom + S(8))
        if self.show_sidebar:
            self._draw_sidebar(surf)
        self.draw_widgets(surf)
        if self.overlay is not None:
            self.overlay.draw(surf, self.app.mouse)

    def _draw_view_badges(self, surf, view):
        """Плашка статуса слева сверху и масштаб слева снизу; возвращает низ плашки."""
        s = self.session
        chip = None
        chip_bottom = None
        if s.ended == "extinct":
            chip = ("ЖИЗНЬ УГАСЛА", DANGER)
        elif s.ended == "explosion":
            chip = ("ОСТАНОВЛЕНО: ВЗРЫВ", DANGER)
        elif s.paused:
            chip = ("ПАУЗА", ACCENT)
        if chip:
            img = render.text("bodyb", chip[0], ACCENT_TEXT)
            box = img.get_rect().inflate(S(20), S(8))
            box.topleft = (view.left + S(14), view.top + S(14))
            render.rounded(surf, chip[1], box, 6)
            surf.blit(img, img.get_rect(center=box.center))
            chip_bottom = box.bottom
        if not self.cam.is_fit:
            zoom = self.cam.zoom / self.cam.min_zoom
            render.blit_text(surf, "small", f"×{zoom:.1f}   Home — весь мир", MUTED,
                             "bottomleft", bottomleft=(view.left + S(14), view.bottom - S(12)))
        return chip_bottom

    def _draw_sidebar(self, surf):
        s = self.session
        side = self.sidebar
        surf.fill(PANEL, side)
        surf.fill(LINE, (side.left, side.top, 1, side.h))
        pad = S(18)
        left, right = side.left + pad, side.right - pad

        # заголовок
        render.blit_text(surf, "h2", f"Мир #{s.seed}", TEXT, "topleft",
                         topleft=(left, pad))
        render.blit_text(surf, "small", f"тик {spaced(s.world.tick)}", MUTED, "topright",
                         topright=(right, pad + S(3)))

        render.blit_text(surf, "bodyb", f"x{s.speed}", TEXT, "center",
                         center=self.speed_label.center)

        # счётчики
        y = self.counters_top
        surf.fill(LINE, (left, y - S(8), right - left, 1))
        counts = (len(s.world.plants), len(s.world.vegetarians), len(s.world.predators))
        col_w = (right - left) / 3
        for i, ((name, color), n) in enumerate(zip(SPECIES, counts)):
            x = int(left + i * col_w)
            render.aa_circle(surf, color, (x + S(4), y + S(9)), S(4))
            render.blit_text(surf, "small", name, MUTED, "topleft", topleft=(x + S(13), y))
            render.blit_text(surf, "big", spaced(n), TEXT if n else FAINT, "topleft",
                             topleft=(x, y + S(17)))
        surf.fill(LINE, (left, self.chart_tabs.rect.top - S(10), right - left, 1))

        self._draw_chart(surf)

        # карточка или подсказка
        if self.selected is not None:
            render.draw_creature_card(surf, self.card_rect, self.selected, self._avg_genom(),
                                      compact=self.compact_card)
        else:
            render.rounded(surf, theme.CARD, self.card_rect, 10)
            lines = render.wrap("small", "Кликните по существу в мире, чтобы рассмотреть его. "
                                "Колесо — приблизить.", self.card_rect.w - 2 * S(14))
            line_h = theme.font("small").get_linesize()
            y = self.card_rect.centery - len(lines) * line_h // 2
            for line in lines:
                render.blit_text(surf, "small", line, MUTED, "topleft",
                                 topleft=(self.card_rect.left + S(14), y))
                y += line_h

        # подвал: темп
        footer_y = side.bottom - S(15)
        if s.ended == "extinct":
            status, color = "партия окончена", DANGER
        elif s.lagging:
            status, color = f"{s.tps:.0f} тиков/с — не успевает за x{s.speed}", ACCENT
        else:
            status, color = f"{s.tps:.0f} тиков/с", FAINT
        render.blit_text(surf, "tiny", status, color, "midleft", midleft=(left, footer_y))
        render.blit_text(surf, "tiny", "Esc — меню", FAINT, "midright",
                         midright=(right, footer_y))

    def _draw_chart(self, surf):
        rect = self.chart_rect
        if rect.h < S(40):
            return
        mouse = self.app.mouse
        hover_x = mouse[0] if rect.collidepoint(mouse) and self.overlay is None else None
        history = self.session.history
        key = (self.chart, self.whole, history.version, rect.size, hover_x,
               theme.get_scale())
        if key != self._chart_key:
            self._chart_hover = None
            surf_chart = pygame.Surface(rect.size)
            surf_chart.fill(PANEL)
            local = surf_chart.get_rect()
            points = history.series(self.whole)
            slots = None if self.whole else RECENT_POINTS
            local_hover = None if hover_x is None else hover_x - rect.left
            draw = (render.draw_population_chart if self.chart == "pop"
                    else render.draw_genome_chart)
            if self.chart == "pop":
                i = draw(surf_chart, local, points, slots, local_hover)
            else:
                i = draw(surf_chart, local, points, slots, local_hover, history.origin)
            self._chart_hover = points[i].tick if i is not None else None
            self._chart_surf, self._chart_key = surf_chart, key
        surf.blit(self._chart_surf, rect)
        if self._chart_hover is not None and self.chart == "genome":
            caption, color = f"тик {spaced(self._chart_hover)}", MUTED
        else:
            caption = ("вся партия" if self.whole
                       else f"последние {spaced(RECENT_POINTS * GRAPH_EVERY)} тиков")
            caption += (" · у каждой линии своя шкала" if self.chart == "pop"
                        else " · справа — изменение от начала")
            color = FAINT
        render.blit_text(surf, "tiny", caption, color, "topleft",
                         topleft=(rect.left, rect.bottom + S(4)))
