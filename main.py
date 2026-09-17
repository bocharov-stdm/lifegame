# main.py — окно, события и игровой цикл.
#
# Логика симуляции живёт в пакете life/, отрисовка — в render.py.
# Здесь остаётся только то, что связано с окном: управление и состояние экрана.
#
# Управление:
#   Пробел      пауза / продолжить
#   →           один тик, пока стоит пауза
#   + / -       скорость: 1, 2, 4, 8, 16 тиков за кадр
#   ЛКМ         выбрать существо под курсором
#   ПКМ, Esc    снять выбор
#   G           показать / скрыть график популяций

import pygame, sys
from collections import deque

from life.config import *
from life.world  import World
from render      import (draw_world, draw_selection, draw_panel, draw_stats,
                         draw_hud, draw_graph)

# Скорость — сколько раз за кадр вызывается world.step(). Это НЕ TICKS_PER_FRAME
# из конфига: тот работает внутри движка (спаун растений, длительность бегства),
# и если крутить его, поменяется баланс, а не темп показа.
SPEEDS = (1, 2, 4, 8, 16)

STATS_EVERY_FRAMES = 60      # строка статистики — раз в секунду
PICK_RADIUS_PX     = 10      # насколько можно промахнуться кликом, в пикселях экрана

# График: точка раз в GRAPH_EVERY тиков, всего GRAPH_POINTS точек — окно в
# 3000 тиков, этого хватает, чтобы разглядеть колебания «хищник — жертва».
# Это настройки экрана, а не движка, поэтому они здесь, а не в config.py.
GRAPH_POINTS = 300
GRAPH_EVERY  = 10


def format_stats(s):
    if s["avg_genom"] is None:
        return f"Все умерли   Всего хищников: {s['predators']}"

    genom_str = " ".join(f"{g:.1f}" for g in s["avg_genom"])
    return (f"Средний геном: {genom_str}   Средняя Энергия: {s['avg_energy']:.1f} "
            f"Всего вегетарианцев: {s['vegetarians']} Всего растений: {s['plants']} "
            f"Всего хищников: {s['predators']}")


def pick_creature(world, wx, wy, radius):
    """Ближайшее к точке (wx, wy) травоядное или хищник не дальше radius, иначе None.

    Координаты мировые. pygame здесь не нужен — функцию проверяют тесты.
    """
    best, best_d2 = None, radius * radius
    for creatures in (world.vegetarians, world.predators):
        for c in creatures:
            dx = c.x - wx
            dy = c.y - wy
            d2 = dx * dx + dy * dy
            if d2 <= best_d2:
                best, best_d2 = c, d2
    return best


def main():
    pygame.init()
    pygame.font.init()
    font   = pygame.font.SysFont(None, 24)
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    pygame.display.set_caption("Tiny Life Simulation")
    clock  = pygame.time.Clock()

    scale_x = WIDTH  / WORLD_WIDTH
    scale_y = HEIGHT / WORLD_HEIGHT

    world      = World()
    text_genom = ""                        # строка-буфер для вывода статистики

    frame    = 0
    paused   = False
    speed    = 0                           # индекс в SPEEDS
    selected = None                        # существо, на которое кликнули

    history    = deque(maxlen=GRAPH_POINTS)   # (растения, травоядные, хищники)
    show_graph = True

    while True:
        # ── события ─────────────────────────────────────────────────────────
        step_once = False
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                pygame.quit()
                sys.exit()

            elif event.type == pygame.KEYDOWN:
                key = event.key
                if key == pygame.K_SPACE:
                    paused = not paused
                elif key == pygame.K_RIGHT and paused:
                    step_once = True
                elif key in (pygame.K_PLUS, pygame.K_EQUALS, pygame.K_KP_PLUS):
                    speed = min(speed + 1, len(SPEEDS) - 1)
                elif key in (pygame.K_MINUS, pygame.K_KP_MINUS):
                    speed = max(speed - 1, 0)
                elif key == pygame.K_ESCAPE:
                    selected = None
                elif key == pygame.K_g:
                    show_graph = not show_graph

            elif event.type == pygame.MOUSEBUTTONDOWN:
                if event.button == 1:
                    mx, my = event.pos
                    selected = pick_creature(world, mx / scale_x, my / scale_y,
                                             PICK_RADIUS_PX / scale_x)
                elif event.button == 3:
                    selected = None

        # ── логика ──────────────────────────────────────────────────────────
        if paused:
            ticks = 1 if step_once else 0
        else:
            ticks = SPEEDS[speed]
        for _ in range(ticks):
            world.step()
            # внутри цикла, по тикам: на ускорении точки не теряются
            if world.tick % GRAPH_EVERY == 0:
                history.append((len(world.plants),
                                len(world.vegetarians),
                                len(world.predators)))

        if selected is not None and not selected.alive:     # съели или умер с голоду
            selected = None

        # ── строка статистики ───────────────────────────────────────────────
        # по кадрам, а не по тикам: на ускорении world.tick перескакивает
        # через кратные 60, и строка перестала бы обновляться
        if frame % STATS_EVERY_FRAMES == 0 or step_once:
            text_genom = format_stats(world.stats())

        # ── рендер ─────────────────────────────────────────────────────────
        screen.fill((30, 30, 30))

        draw_world(screen, world, scale_x, scale_y)
        if selected is not None:
            draw_selection(screen, selected, scale_x, scale_y)
            draw_panel(screen, font, selected)

        # выводим статистику поверх всего
        draw_stats(screen, font, text_genom)
        draw_hud(screen, font, paused, SPEEDS[speed], world.tick)
        if show_graph:
            draw_graph(screen, font, history)

        pygame.display.flip()
        clock.tick(FPS)
        frame += 1


if __name__ == "__main__":
    main()
