# main.py
import pygame, sys, random
from config   import *
from plant    import Plant
from creature import Creature
from predator import Predator

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

    while True:
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                pygame.quit()
                sys.exit()

        # --- логика мира ---
        for _ in range(TICKS_PER_FRAME):
            if random.random() < PLANT_SPAWN_CHANCE:
                plants.append(Plant())

            # хищники
            for pr in list(predators):
                pr.move(creatures)
                pr.try_eat(creatures)
                pr.maybe_divide(predators)
                if pr.energy <= 0:
                    predators.remove(pr)

            # травоядные
            for c in list(creatures):
                c.move(plants, predators)
                c.try_eat(plants)
                c.maybe_divide(creatures)
                if c.energy <= 0:
                    creatures.remove(c)

        # --- рендер ---
        screen.fill((30, 30, 30))
        for p in plants:     p.draw(screen, scale_x, scale_y)
        for pr in predators: pr.draw(screen, scale_x, scale_y)
        for c in creatures:  c.draw(screen, scale_x, scale_y)

        # статистика
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

        pygame.display.flip()
        clock.tick(FPS)

if __name__ == "__main__":
    main()
