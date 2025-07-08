# life_sim.py — «симуляция жизни» с выводом статистики
#
#   pip install pygame
#   python life_sim.py
#
# Новое:
#   • В левом-верхнем углу всегда показываются:
#         ▸ количество существ
#         ▸ средняя скорость
#         ▸ средний радиус нюха
#     Симуляционная логика осталась прежней.

import pygame
import random
import math
import sys

# ────────────────────────── Настройки окна и мира ───────────────────────────
WIDTH, HEIGHT = 800, 800                # размер окна (пиксели экрана)
WORLD_WIDTH, WORLD_HEIGHT = 2400, 2400  # логический размер мира

SCALE_X = WIDTH  / WORLD_WIDTH          # масштаб «мир → экран»
SCALE_Y = HEIGHT / WORLD_HEIGHT

FPS = 120

# ────────────────────────── Параметры существа ──────────────────────────────
BASE_SPEED = 2.0
BASE_SMELL = 100.0

MAX_ENERGY = 100
ENERGY_FROM_PLANT = 20

# Энергозатраты
K_MOVE  = 0.01
K_SMELL = 0.001

# ────────────────────────── Параметры растений ──────────────────────────────
PLANT_RADIUS = 3
PLANTS_AT_START = 50
PLANT_SPAWN_CHANCE = 1
# ─────────────────────────────────────────────────────────────────────────────


class Plant:
    def __init__(self):
        self.x = random.randint(PLANT_RADIUS, WORLD_WIDTH  - PLANT_RADIUS)
        self.y = random.randint(PLANT_RADIUS, WORLD_HEIGHT - PLANT_RADIUS)

    def draw(self, surf):
        pygame.draw.circle(
            surf,
            (0, 200, 0),
            (int(self.x * SCALE_X), int(self.y * SCALE_Y)),
            max(1, int(PLANT_RADIUS * SCALE_X))
        )


class Creature:
    DIAM = 8  # мир-единиц

    def __init__(self, x=None, y=None, energy=MAX_ENERGY * 0.5,
                 speed=BASE_SPEED, smell_radius=BASE_SMELL):
        self.x = x if x is not None else random.randint(self.DIAM, WORLD_WIDTH  - self.DIAM)
        self.y = y if y is not None else random.randint(self.DIAM, WORLD_HEIGHT - self.DIAM)
        self.energy = energy
        self.speed = speed
        self.smell_radius = smell_radius
        self._choose_new_target()

    # ─────────────────────────── Внутренние методы ───────────────────────────
    def _choose_new_target(self):
        self.tx = random.randint(self.DIAM, WORLD_WIDTH  - self.DIAM)
        self.ty = random.randint(self.DIAM, WORLD_HEIGHT - self.DIAM)

    def _vector_towards(self, tx, ty):
        dx, dy = tx - self.x, ty - self.y
        dist = math.hypot(dx, dy)
        if dist == 0:
            return 0, 0
        k = self.speed / dist
        return dx * k, dy * k

    def _nearest_plant_in_smell(self, plants):
        nearest, best_d = None, self.smell_radius
        for p in plants:
            d = math.hypot(self.x - p.x, self.y - p.y)
            if d < best_d:
                nearest, best_d = p, d
        return nearest

    # ───────────────────────────── Основная логика ───────────────────────────
    def move(self, plants):
        target = self._nearest_plant_in_smell(plants)
        dx, dy = (
            self._vector_towards(target.x, target.y)
            if target else
            self._vector_towards(self.tx, self.ty)
        )

        self.x = min(max(self.x + dx, self.DIAM), WORLD_WIDTH  - self.DIAM)
        self.y = min(max(self.y + dy, self.DIAM), WORLD_HEIGHT - self.DIAM)

        self.energy -= K_MOVE * self.speed ** 2 + K_SMELL * self.smell_radius

        if not target and math.hypot(self.x - self.tx, self.y - self.ty) < self.speed:
            self._choose_new_target()

    def try_eat(self, plants):
        for p in plants:
            if math.hypot(self.x - p.x, self.y - p.y) < self.DIAM:
                plants.remove(p)
                self.energy = min(MAX_ENERGY, self.energy + ENERGY_FROM_PLANT)
                return True
        return False

    def _mutate(self, value):
        factor = 1 + random.uniform(-0.5, 0.5)
        return value * factor

    def maybe_divide(self, creatures):
        if self.energy >= MAX_ENERGY * 0.9:
            self.energy -= MAX_ENERGY * 0.6
            cx = min(max(self.x + random.randint(-15, 15), self.DIAM), WORLD_WIDTH  - self.DIAM)
            cy = min(max(self.y + random.randint(-15, 15), self.DIAM), WORLD_HEIGHT - self.DIAM)

            child_speed = self.speed if random.random() >= 0.4 else self._mutate(self.speed)
            child_smell = self.smell_radius if random.random() >= 0.4 else self._mutate(self.smell_radius)

            creatures.append(
                Creature(cx, cy, MAX_ENERGY * 0.30,
                         speed=child_speed, smell_radius=child_smell)
            )

    # ─────────────────────────────── Отрисовка ───────────────────────────────
    def draw(self, surf):
        sx, sy = int(self.x * SCALE_X), int(self.y * SCALE_Y)
        pygame.draw.circle(
            surf, (120, 120, 255),
            (sx, sy),
            max(1, int(self.smell_radius * SCALE_X)), 1
        )
        pygame.draw.circle(
            surf, (200, 0, 0),
            (sx, sy),
            max(1, int((self.DIAM // 2) * SCALE_X))
        )


def main():
    pygame.init()
    pygame.font.init()
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    pygame.display.set_caption("Tiny Life Simulation")
    clock = pygame.time.Clock()

    font = pygame.font.SysFont(None, 24)

    creatures = [Creature() for i in range(10)]
    plants = [Plant() for _ in range(PLANTS_AT_START)]

    while True:
        # ───── События ────────────────────────────────────────────────────────
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                pygame.quit()
                sys.exit()

        # ───── Логика ─────────────────────────────────────────────────────────
        if random.random() < PLANT_SPAWN_CHANCE:
            plants.append(Plant())

        for c in list(creatures):
            c.move(plants)
            c.try_eat(plants)
            c.maybe_divide(creatures)
            if c.energy <= 0:
                creatures.remove(c)

        # ───── Отрисовка ──────────────────────────────────────────────────────
        screen.fill((30, 30, 30))
        for p in plants:
            p.draw(screen)
        for c in creatures:
            c.draw(screen)

        # ─── Статистика ────────────────────────────────
        count = len(creatures)
        if count:
            avg_speed = sum(c.speed for c in creatures) / count
            avg_smell = sum(c.smell_radius for c in creatures) / count
        else:
            avg_speed = avg_smell = 0.0

        stat_text = (
            f"Creatures: {count}   "
            f"Avg speed: {avg_speed:.2f}   "
            f"Avg smell: {avg_smell:.1f}"
        )
        text_surf = font.render(stat_text, True, (255, 255, 255))
        screen.blit(text_surf, (10, 10))

        pygame.display.flip()
        clock.tick(FPS)


if __name__ == "__main__":
    main()
