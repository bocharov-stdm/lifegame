import pygame, sys, random
from config   import *
from plant    import Plant
from creature import Creature
from predator import Predator

# ── вспомогательная утилита ────────────────────────────────────────────────────
def is_on_screen(x, y, scale_x, scale_y):
    sx = int(x * scale_x)
    sy = int(y * scale_y)
    return 0 <= sx < WIDTH and 0 <= sy < HEIGHT
# ───────────────────────────────────────────────────────────────────────────────

def main():
    pygame.init()
    pygame.font.init()
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    pygame.display.set_caption("Tiny Life Simulation")
    clock = pygame.time.Clock()
    font  = pygame.font.SysFont(None, 24)

    scale_x = WIDTH  / WORLD_WIDTH
    scale_y = HEIGHT / WORLD_HEIGHT

    creatures = [Creature() for _ in range(GREATURES_AT_START)]
    predators = [Predator() for _ in range(PREDATORS_AT_START)]
    plants    = [Plant()    for _ in range(PLANTS_AT_START)]

    tickcounter = 0
    text1, text2 = "", ""          # чтобы не было NameError до первого расчёта

    while True:
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                pygame.quit()
                sys.exit()

        # ── логика мира ────────────────────────────────────────────────────────
        for _ in range(TICKS_PER_FRAME):
            if random.random() < PLANT_SPAWN_CHANCE:
                plants.append(Plant())

            # хищники
            new_predators = []
            for pr in predators:
                pr.move(creatures)
                pr.try_eat(creatures)
                if tickcounter % 30 == 0:
                    pr.maybe_divide(predators)
                if pr.energy > 0:
                    new_predators.append(pr)
            predators = new_predators

            # травоядные
            new_creatures = []
            for c in creatures:
                c.move(plants, predators)
                c.try_eat(plants)
                if tickcounter % 30 == 0:
                    c.maybe_divide(creatures)
                if c.energy > 0:
                    new_creatures.append(c)
            creatures = new_creatures
        # ───────────────────────────────────────────────────────────────────────

        # ── рендер ─────────────────────────────────────────────────────────────
        screen.fill((30, 30, 30))

        for p in plants:
            if is_on_screen(p.x, p.y, scale_x, scale_y):
                p.draw(screen, scale_x, scale_y)

        for pr in predators:
            if is_on_screen(pr.x, pr.y, scale_x, scale_y):
                pr.draw(screen, scale_x, scale_y)

        for c in creatures:
            if is_on_screen(c.x, c.y, scale_x, scale_y):
                c.draw(screen, scale_x, scale_y)
        # ───────────────────────────────────────────────────────────────────────

        # ── статистика (раз в 30 тиков) ───────────────────────────────────────
        if tickcounter % 30 == 0:
            count_c  = len(creatures)
            count_pr = len(predators)
            avg_speed_c  = sum(c.speed for c in creatures)     / count_c  if count_c  else 0
            avg_smell_c  = sum(c.smell_radius for c in creatures) / count_c  if count_c  else 0
            avg_speed_pr = sum(p.speed for p in predators)     / count_pr if count_pr else 0
            avg_smell_pr = sum(p.smell_radius for p in predators) / count_pr if count_pr else 0

            text1 = f"Creatures: {count_c}  Avg speed: {avg_speed_c:.2f}  Avg smell: {avg_smell_c:.1f}"
            text2 = f"Predators: {count_pr}  Avg speed: {avg_speed_pr:.2f}  Avg smell: {avg_smell_pr:.1f}"

        screen.blit(font.render(text1, True, (255, 255, 255)), (10, 10))
        screen.blit(font.render(text2, True, (120, 200, 255)), (10, 35))
        # ───────────────────────────────────────────────────────────────────────

        tickcounter += 1
        pygame.display.flip()
        clock.tick(FPS)

if __name__ == "__main__":
    main()
