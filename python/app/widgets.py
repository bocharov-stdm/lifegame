# widgets.py — минимальный набор элементов управления.
#
# У каждого виджета есть rect (его выставляет сцена в layout), handle(event,
# mouse) -> bool — «событие моё», и draw(surf, mouse). Положение мыши
# передаётся явно, а не читается из pygame.mouse: так сцены можно гонять в
# тестах синтетическими событиями.

import pygame

from . import render
from .render import blit_text, draw_icon, rounded, text_width
from .theme import (ACCENT, ACCENT_TEXT, BUTTON, BUTTON_DOWN, BUTTON_HOVER, CARD,
                    FAINT, LINE, MUTED, PRIMARY_HOVER, TEXT, S)

LEFT = 1


def _is_click(event, rect):
    return (event.type == pygame.MOUSEBUTTONDOWN and event.button == LEFT
            and rect.collidepoint(event.pos))


class Widget:
    interactive = True          # участвует в проверке раскладки на перекрытия

    def __init__(self):
        self.rect = pygame.Rect(0, 0, 0, 0)
        self.visible = True
        self.enabled = True

    def hovered(self, mouse):
        return self.visible and self.enabled and self.rect.collidepoint(mouse)

    def handle(self, event, mouse):
        return False

    def draw(self, surf, mouse):
        pass

    def problems(self):
        """Что не так с раскладкой виджета (текст не влезает и т. п.) — для тестов."""
        return []


class Button(Widget):
    """Кнопка с текстом и/или значком. primary — акцентная (главное действие)."""

    def __init__(self, label, on_click, primary=False, icon=None, role="bodyb"):
        super().__init__()
        self.label = label
        self.on_click = on_click
        self.primary = primary
        self.icon = icon
        self.role = role
        self.pressed = False
        self.active = False       # «включена» (для кнопок-переключателей, например «Следить»)

    def handle(self, event, mouse):
        if not (self.visible and self.enabled):
            self.pressed = False
            return False
        if _is_click(event, self.rect):
            self.pressed = True
            return True
        if event.type == pygame.MOUSEBUTTONUP and event.button == LEFT and self.pressed:
            self.pressed = False
            if self.rect.collidepoint(event.pos):
                self.on_click()
            return True
        return False

    def draw(self, surf, mouse):
        if not self.visible:
            return
        hover = self.hovered(mouse)
        if self.primary:
            bg = PRIMARY_HOVER if hover else ACCENT
            fg = ACCENT_TEXT
        else:
            bg = BUTTON_DOWN if self.pressed else BUTTON_HOVER if hover else BUTTON
            fg = TEXT if self.enabled else FAINT
        if self.active:
            bg, fg = ACCENT, ACCENT_TEXT
        rounded(surf, bg, self.rect, 8)

        if self.icon and not self.label:
            draw_icon(surf, self.icon, self.rect, fg)
            return
        label = self.label
        if self.icon:
            # значок и текст — одна группа по центру кнопки
            icon_w = int(self.rect.h * 0.75)
            gap = S(4)
            group = icon_w + gap + text_width(self.role, label)
            left = self.rect.centerx - group // 2
            draw_icon(surf, self.icon, pygame.Rect(left, self.rect.top, icon_w, self.rect.h), fg)
            blit_text(surf, self.role, label, fg, "midleft",
                      midleft=(left + icon_w + gap, self.rect.centery))
        else:
            blit_text(surf, self.role, label, fg, "center", center=self.rect.center)

    def min_width(self):
        extra = int(self.rect.h * 0.75) + S(4) if self.icon else 0
        return text_width(self.role, self.label) + 2 * S(12) + extra if self.label else self.rect.h

    def problems(self):
        if self.label and self.min_width() > self.rect.w + 1:
            return [f"кнопка «{self.label}»: текст шире кнопки"]
        return []


class Slider(Widget):
    """Числовой параметр: подпись и значение сверху, дорожка снизу.

    field — app.settings.Field: пределы, шаг, формат. Значение читается и
    пишется через get/set, чтобы виджет не владел настройками.
    """

    def __init__(self, field, get, set_, is_default):
        super().__init__()
        self.field = field
        self.get = get
        self.set = set_
        self.is_default = is_default
        self.dragging = False

    # Скрытый виджет событий не получает (Scene.active_widgets), и отпускание
    # кнопки до него не дойдёт: без сброса ползунок, спрятанный вкладкой
    # посреди перетаскивания, потом ехал бы за мышью с отпущенной кнопкой.
    @property
    def visible(self):
        return self._visible

    @visible.setter
    def visible(self, value):
        self._visible = value
        if not value:
            self.dragging = False

    @property
    def track(self):
        r = self.rect
        return pygame.Rect(r.left + S(8), r.bottom - S(14), r.w - S(16), S(4))

    def _set_from_x(self, x):
        t = (x - self.track.left) / max(self.track.w, 1)
        t = min(max(t, 0.0), 1.0)
        f = self.field
        self.set(f.snap(f.lo + t * (f.hi - f.lo)))

    def handle(self, event, mouse):
        if not self.visible:
            return False
        if _is_click(event, self.rect):
            self.dragging = True
            self._set_from_x(event.pos[0])
            return True
        if event.type == pygame.MOUSEMOTION and self.dragging:
            # кнопку отпустили там, где мы этого не видели (за окном)
            if not getattr(event, "buttons", (1,))[0]:
                self.dragging = False
                return False
            self._set_from_x(event.pos[0])
            return True
        if event.type == pygame.MOUSEBUTTONUP and event.button == LEFT and self.dragging:
            self.dragging = False
            return True
        # y == 0 — горизонтальная прокрутка (тачпад, наклон колеса): не наша
        if event.type == pygame.MOUSEWHEEL and event.y and self.rect.collidepoint(mouse):
            f = self.field
            self.set(f.snap(self.get() + f.step * (1 if event.y > 0 else -1)))
            return True
        return False

    def draw(self, surf, mouse):
        f = self.field
        value = self.get()
        hover = self.hovered(mouse) or self.dragging
        r = self.rect
        label_y = r.top + S(4)
        x = r.left + S(8)
        if not self.is_default():                 # отличается от значения по умолчанию
            render.aa_circle(surf, ACCENT, (r.left + S(2), label_y + S(9)), S(3))
        blit_text(surf, "body", f.label, TEXT if hover else MUTED, "topleft",
                  topleft=(x, label_y))
        blit_text(surf, "bodyb", f.format(value), TEXT, "topright",
                  topright=(r.right - S(8), label_y))

        track = self.track
        rounded(surf, LINE, track, 2)
        t = (value - f.lo) / (f.hi - f.lo)
        done = track.copy()
        done.w = int(track.w * t)
        if done.w > 0:
            rounded(surf, ACCENT if hover else MUTED, done, 2)
        knob = (track.left + track.w * t, track.centery)
        render.aa_circle(surf, TEXT, knob, S(8) if hover else S(7))

    def problems(self):
        need = (text_width("body", self.field.label) + S(16) + S(12)
                + text_width("bodyb", self.field.format(self.field.hi)))
        if need > self.rect.w + 1:
            return [f"слайдер «{self.field.label}»: подпись и значение не влезают"]
        return []


class Toggle(Widget):
    """Переключатель-«таблетка» с подписью; кликается вся строка."""

    def __init__(self, label, get, set_):
        super().__init__()
        self.label = label
        self.get = get
        self.set = set_

    def handle(self, event, mouse):
        if self.visible and _is_click(event, self.rect):
            self.set(not self.get())
            return True
        return False

    def draw(self, surf, mouse):
        r = self.rect
        on = self.get()
        hover = self.hovered(mouse)
        blit_text(surf, "body", self.label, TEXT if hover else MUTED, "midleft",
                  midleft=(r.left + S(8), r.centery))
        pill = pygame.Rect(0, 0, S(38), S(22))
        pill.midright = (r.right - S(8), r.centery)
        rounded(surf, ACCENT if on else LINE, pill, 11)
        knob_x = pill.right - pill.h / 2 if on else pill.left + pill.h / 2
        render.aa_circle(surf, ACCENT_TEXT if on else TEXT, (knob_x, pill.centery), S(8))

    def problems(self):
        if text_width("body", self.label) + S(16) + S(38) + S(12) > self.rect.w + 1:
            return [f"переключатель «{self.label}»: подпись не влезает"]
        return []


class Segmented(Widget):
    """Выбор одного из нескольких вариантов (вкладки, масштаб, диапазон графика)."""

    def __init__(self, options, get, set_, role="body"):
        super().__init__()
        self.options = list(options)          # [(значение, подпись), ...]
        self.get = get
        self.set = set_
        self.role = role

    def _cells(self):
        n = len(self.options)
        pad = S(3)
        inner = self.rect.inflate(-2 * pad, -2 * pad)
        w = inner.w / n
        return [pygame.Rect(round(inner.left + i * w), inner.top, round(w), inner.h)
                for i in range(n)]

    def handle(self, event, mouse):
        if not self.visible or not _is_click(event, self.rect):
            return False
        for (value, _), cell in zip(self.options, self._cells()):
            if cell.collidepoint(event.pos):
                self.set(value)
        return True

    def draw(self, surf, mouse):
        rounded(surf, CARD, self.rect, 8)
        current = self.get()
        for (value, label), cell in zip(self.options, self._cells()):
            selected = value == current
            if selected:
                rounded(surf, BUTTON_HOVER, cell, 6)
            elif cell.collidepoint(mouse):
                rounded(surf, BUTTON, cell, 6)
            blit_text(surf, self.role, label, TEXT if selected else MUTED, "center",
                      center=cell.center)

    def problems(self):
        out = []
        for (_, label), cell in zip(self.options, self._cells()):
            if text_width(self.role, label) + S(8) > cell.w + 1:
                out.append(f"вариант «{label}» не влезает")
        return out


class NumberField(Widget):
    """Поле для целого числа (сид): клик — ввод цифр, Enter или клик мимо — готово."""

    def __init__(self, get, set_, lo, hi, max_digits=5):
        super().__init__()
        self.get = get
        self.set = set_
        self.lo, self.hi = lo, hi
        self.max_digits = max_digits
        self.focused = False
        self.buffer = ""

    def commit(self):
        """Завершить ввод: введённое сохраняется (пустое поле — прежнее значение)."""
        if not self.focused:
            return
        if self.buffer:
            self.set(min(max(int(self.buffer), self.lo), self.hi))
        self.focused = False
        self.buffer = ""

    def handle(self, event, mouse):
        if not self.visible:
            return False
        if event.type == pygame.MOUSEBUTTONDOWN and event.button == LEFT:
            if self.rect.collidepoint(event.pos):
                if not self.focused:
                    self.focused, self.buffer = True, ""
                return True
            if self.focused:
                self.commit()                  # клик мимо — отдаём событие дальше
            return False
        if event.type == pygame.MOUSEWHEEL and event.y and self.rect.collidepoint(mouse):
            self.set(min(max(self.get() + (1 if event.y > 0 else -1), self.lo), self.hi))
            return True
        if event.type == pygame.KEYDOWN and self.focused:
            if event.key in (pygame.K_RETURN, pygame.K_KP_ENTER, pygame.K_ESCAPE,
                             pygame.K_TAB):
                if event.key == pygame.K_ESCAPE:
                    self.buffer = ""
                self.commit()
            elif event.key == pygame.K_BACKSPACE:
                self.buffer = self.buffer[:-1]
            elif event.unicode.isdigit() and len(self.buffer) < self.max_digits:
                self.buffer += event.unicode
            return True                        # пока поле в фокусе, клавиши — его
        return False

    def draw(self, surf, mouse):
        r = self.rect
        hover = self.hovered(mouse)
        rounded(surf, CARD, r, 8)
        rounded(surf, ACCENT if self.focused else BUTTON_HOVER if hover else LINE, r, 8, 1)
        shown = self.buffer if self.focused else str(self.get())
        img_rect = blit_text(surf, "bodyb", shown, TEXT, "midleft",
                             midleft=(r.left + S(12), r.centery))
        if self.focused and (pygame.time.get_ticks() // 500) % 2 == 0:
            surf.fill(ACCENT, (img_rect.right + S(2), r.top + S(8), max(1, S(2)), r.h - S(16)))

