# main.py

import pygame, sys, random
from config import *
from plant import Plant
from creature import Creature

def main():
    pygame.init()
    pygame.font.init()
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    pygame.display.set_caption("Tiny Life Simulation")
    clock = pygame.time.Clock()
    font = pygame.font.SysFont(None, 24)

    scale_x = WIDTH / WORLD_WIDTH
    scale_y = HEIGHT / WORLD_HEIGHT

    creatures = [Creature() for _ in range(GREATURES_AT_START)]
    plants = [Plant() for _ in range(PLANTS_AT_START)]

    while True:
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                pygame.quit()
                sys.exit()

        for _ in range(TICKS_PER_FRAME):
            if random.random() < PLANT_SPAWN_CHANCE:
                plants.append(Plant())

            for c in list(creatures):
                c.move(plants)
                c.try_eat(plants)
                c.maybe_divide(creatures)
                if c.energy <= 0:
                    creatures.remove(c)

        screen.fill((30, 30, 30))
        for p in plants:
            p.draw(screen, scale_x, scale_y)  # ← передаём масштаб

        for c in creatures:
            c.draw(screen, scale_x, scale_y)

        count = len(creatures)
        avg_speed = sum(c.speed for c in creatures) / count if count else 0
        avg_smell = sum(c.smell_radius for c in creatures) / count if count else 0
        text = f"Creatures: {count}  Avg speed: {avg_speed:.2f}  Avg smell: {avg_smell:.1f}"
        screen.blit(font.render(text, True, (255, 255, 255)), (10, 10))

        pygame.display.flip()
        clock.tick(FPS)

if __name__ == "__main__":
    main()
