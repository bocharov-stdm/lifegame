# config.py
WIDTH, HEIGHT = 1200, 800
WORLD_WIDTH, WORLD_HEIGHT = 9000, 6000

FPS = 60
TICKS_PER_FRAME = 5           # логических тиков на один рендер-кадр

BASE_SPEED = 1.2
BASE_SMELL  = 150.0

MAX_ENERGY        = 100
ENERGY_FROM_PLANT = 20

K_MOVE           = 0.005      # расход энергии за скорость²
K_SMELL          = 0.00005    # расход энергии за радиус нюха
BASAL_METABOLISM = 0.005      # «покой»

PLANT_RADIUS      = 3
PLANTS_AT_START   = 100
GREATURES_AT_START = 20

# --- Хищники ---
PREDATORS_AT_START  = 5
FLEE_DURATION_SEC   = 2                       # существо убегает 2 с
FLEE_TICKS          = int(FLEE_DURATION_SEC * FPS * TICKS_PER_FRAME)

DENSITY_PER_PIXEL  = 1e-9
PLANT_SPAWN_CHANCE = DENSITY_PER_PIXEL * WORLD_WIDTH * WORLD_HEIGHT
