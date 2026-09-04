"""Метаболизм, смерть и поедание вдоль пути."""

import random

import config
from plant import Plant
from spatial import SpatialHash
from vegetarian import Vegetarian


def _veg(**genes):
    genom = list(config.VEGETARIAN_BASE_GENOM)
    for name, value in genes.items():
        genom[config.GENE_INDEX[name]] = value
    return Vegetarian(genom=genom)


def _cost(v):
    before = v.energy
    v.pay_energy()
    return before - v.energy


def test_cost_grows_with_each_expensive_gene():
    for gene, small, big in (("size", 40, 160), ("speed", 5, 40), ("vision", 500, 4000)):
        cheap = _cost(_veg(**{gene: small}))
        pricey = _cost(_veg(**{gene: big}))
        assert pricey > cheap, gene


def test_death_is_flagged_once_and_energy_not_negative():
    v = _veg()
    v.energy = 0.001
    v.pay_energy()
    assert not v.alive
    assert v.energy == 0.0


def test_alive_organism_keeps_living_while_energy_remains():
    v = _veg()
    v.energy = v.max_energy
    v.pay_energy()
    assert v.alive and 0 < v.energy < v.max_energy


def test_gain_is_capped_by_max_energy():
    v = _veg()
    v.gain(10 * v.max_energy)
    assert v.energy == v.max_energy


def test_fast_organism_does_not_jump_over_food():
    """Регрессия: без проверки по отрезку быстрая особь перепрыгивала растение."""
    random.seed(0)
    v = _veg(speed=1000, vision=5000, min_y=0, max_y=100)
    v.x, v.y = 1000.0, 5000.0

    plant = Plant(x=1500.0, y=5000.0)     # ровно на пути, но не в конце шага
    grid = SpatialHash(config.SPATIAL_CELL_SIZE)
    grid.rebuild([plant])

    old = (v.x, v.y)
    v.x = 2000.0                          # шаг перескочил растение
    assert v.try_eat(grid, *old)
    assert not plant.alive


def test_plant_outside_layer_is_not_eaten():
    """Регрессия: move уважал слой, а try_eat — нет, и биомы размывались."""
    v = _veg(min_y=0, max_y=10)
    v.x, v.y = 1000.0, 1000.0

    deep = Plant(x=1000.0, y=config.WORLD_HEIGHT * 0.9)
    grid = SpatialHash(config.SPATIAL_CELL_SIZE)
    grid.rebuild([deep])

    assert not v.try_eat(grid, v.x, v.y)
    assert deep.alive


def test_organism_stays_inside_its_layer_after_step():
    random.seed(1)
    v = _veg(min_y=20, max_y=30, speed=5000)
    y_min, y_max = v.layer_bounds()
    for _ in range(50):
        v.step_towards(v.x + 10000, v.y + 10000)
        assert y_min - 1e-6 <= v.y <= y_max + 1e-6
        assert v.radius <= v.x <= config.WORLD_WIDTH - v.radius
