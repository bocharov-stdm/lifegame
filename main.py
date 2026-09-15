# main.py — окно, события и игровой цикл.
#
# Логика симуляции живёт в пакете life/, отрисовка — в render.py.
# Здесь остаётся только то, что связано с окном.

import pygame, sys

from life.config import *
from life.world  import World
from render      import draw_world


def format_stats(s):
    if s["avg_genom"] is None:
        return f"Все умерли   Всего хищников: {s['predators']}"

    genom_str = " ".join(f"{g:.1f}" for g in s["avg_genom"])
    return (f"Средний геном: {genom_str}   Средняя Энергия: {s['avg_energy']:.1f} "
            f"Всего вегетарианцев: {s['vegetarians']} Всего растений: {s['plants']} "
            f"Всего хищников: {s['predators']}")


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

    while True:
        # ── системные события ────────────────────────────────────────────────
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                pygame.quit()
                sys.exit()

        # ── логика ──────────────────────────────────────────────────────────
        world.step()

        # ── обновляем строку статистики раз в 60 тиков ──────────────────────
        if world.tick % 60 == 0:
            text_genom = format_stats(world.stats())

        # ── рендер ─────────────────────────────────────────────────────────
        screen.fill((30, 30, 30))

        draw_world(screen, world, scale_x, scale_y)

        # выводим статистику поверх всего
        screen.blit(font.render(text_genom, True, (255, 255, 255)), (10, 10))

        pygame.display.flip()
        clock.tick(FPS)


if __name__ == "__main__":
    main()
