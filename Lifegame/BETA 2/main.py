#main py

import pygame, sys, random
from config   import *
from plant    import Plant
from vegetarian import Vegetarian

def main():
    pygame.init()
    pygame.font.init()
    font = pygame.font.SysFont(None, 24)
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    pygame.display.set_caption("Tiny Life Simulation")
    clock = pygame.time.Clock()

    scale_x = WIDTH  / WORLD_WIDTH
    scale_y = HEIGHT / WORLD_HEIGHT

    plants    = [Plant()    for _ in range(PLANTS_AT_START)]
    vegetarians = [Vegetarian() for _ in range(VEGETARIANS_AT_START)]

    tickcounter = 0


    while True:
        # Какая-то системная хуйня с окном
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                pygame.quit()
                sys.exit()
        # Спаун растений

        for i in range(TICKS_PER_FRAME):
            if random.random() < PLANT_SPAWN_CHANCE:
                plants.append(Plant())

        new_vegetarians = []
        for v in vegetarians:
            v.move(plants)
            v.try_eat(plants)
            if tickcounter % 30 == 0:
                v.maybe_divide(vegetarians)
            if v.alive:
                new_vegetarians.append(v)
        vegetarians = new_vegetarians

        if tickcounter % 60 == 0:
            if vegetarians:
                avg_genom = [sum(v.genom[i] for v in vegetarians) / len(vegetarians) for i in
                             range(len(vegetarians[0].genom))]
                genom_str = " ".join(f"{g:.1f}" for g in avg_genom)
                avg_energy = [sum(v.energy for v in vegetarians) / len(vegetarians)]
                text3 = f"Средний геном: {genom_str}, Средняя энергия {avg_energy}"
            else:
                text3 = "Все умерли"
        # Отрисовка
        screen.fill((30, 30, 30))
        for p in plants:
            p.draw(screen, scale_x, scale_y)

        for v in vegetarians:
                v.draw(screen, scale_x, scale_y)
        # счётчик
        screen.blit(font.render(text3, True, (255, 255, 255)), (10, 10))

        tickcounter += 1
        pygame.display.flip()
        clock.tick(FPS)

if __name__ == "__main__":
    main()