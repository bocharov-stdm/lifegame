# main.py — окно, ввод и рендер. Вся логика мира живёт в world.py.
#
# Управление:
#   Space   пауза
#   + / -   скорость (тиков на кадр)
#   G       панель графиков
#   M       следующий ген на графике
#   S       экспорт истории в run_<seed>.csv
#   R       рестарт с тем же сидом
#   Esc     выход

import random
import sys

import pygame

from .config import (WIDTH, HEIGHT, WORLD_WIDTH, WORLD_HEIGHT, FPS,
                    TICKS_PER_FRAME, MAX_TICKS_PER_FRAME, RANDOM_SEED,
                    GENE_NAMES, STATS_EVERY)
from .stats import History, draw_panel
from .world import World


def is_on_screen(obj, scale_x, scale_y):
    sx, sy = obj.x * scale_x, obj.y * scale_y
    return 0 <= sx < WIDTH and 0 <= sy < HEIGHT


def main():
    pygame.init()
    pygame.font.init()
    font = pygame.font.SysFont(None, 20)
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    clock = pygame.time.Clock()

    seed = RANDOM_SEED if RANDOM_SEED is not None else random.randrange(1 << 30)
    pygame.display.set_caption(f"Tiny Life Simulation — seed {seed}")

    world = World(seed=seed)
    history = History()

    scale_x = WIDTH / WORLD_WIDTH
    scale_y = HEIGHT / WORLD_HEIGHT

    speed = TICKS_PER_FRAME
    paused = False
    show_graphs = False
    gene_idx = 0
    message = ""

    while True:
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                pygame.quit()
                sys.exit()
            if event.type != pygame.KEYDOWN:
                continue
            if event.key == pygame.K_ESCAPE:
                pygame.quit()
                sys.exit()
            elif event.key == pygame.K_SPACE:
                paused = not paused
            elif event.key in (pygame.K_PLUS, pygame.K_EQUALS, pygame.K_KP_PLUS):
                speed = min(MAX_TICKS_PER_FRAME, speed * 2)
            elif event.key in (pygame.K_MINUS, pygame.K_KP_MINUS):
                speed = max(1, speed // 2)
            elif event.key == pygame.K_g:
                show_graphs = not show_graphs
            elif event.key == pygame.K_m:
                gene_idx = (gene_idx + 1) % len(GENE_NAMES)
            elif event.key == pygame.K_s:
                path = history.export_csv(f"run_{seed}.csv")
                message = f"сохранено: {path}" if path else "нечего сохранять"
            elif event.key == pygame.K_r:
                world = World(seed=seed)
                history = History()
                message = "рестарт"

        if not paused:
            for _ in range(speed):
                world.step()
                if world.tick % STATS_EVERY == 0:
                    history.record(world)

        # ── рендер ───────────────────────────────────────────────────────────
        screen.fill((20, 24, 34))
        for group in (world.plants, world.vegetarians, world.predators):
            for obj in group:
                if is_on_screen(obj, scale_x, scale_y):
                    obj.draw(screen, scale_x, scale_y)

        lines = [
            f"Тик {world.tick}   растения {len(world.plants)}   "
            f"травоядные {len(world.vegetarians)}   хищники {len(world.predators)}",
            f"x{speed}{'  ПАУЗА' if paused else ''}   FPS {clock.get_fps():.0f}   "
            f"seed {seed}   [Space +/- G M S R]   {message}",
        ]
        if world.extinct:
            lines.append("Все умерли")
        for i, line in enumerate(lines):
            screen.blit(font.render(line, True, (235, 235, 245)), (10, 8 + i * 20))

        if show_graphs:
            draw_panel(screen, font, history, GENE_NAMES[gene_idx],
                       (WIDTH - 430, HEIGHT - 320, 420, 310))

        pygame.display.flip()
        clock.tick(FPS)


if __name__ == "__main__":
    main()
