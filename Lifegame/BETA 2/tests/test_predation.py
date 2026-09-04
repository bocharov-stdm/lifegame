"""Хищничество: слой, переваривание, питательность добычи."""

import config
from predator import Predator
from spatial import SpatialHash
from vegetarian import Vegetarian


def _pred(**genes):
    genom = list(config.PREDATOR_BASE_GENOM)
    for name, value in genes.items():
        genom[config.GENE_INDEX[name]] = value
    return Predator(genom=genom)


def _grid(items):
    grid = SpatialHash(config.SPATIAL_CELL_SIZE)
    grid.rebuild(items)
    return grid


def test_predator_eats_prey_in_reach():
    pred = _pred(min_y=0, max_y=100)
    pred.x, pred.y = 5000.0, 5000.0
    prey = Vegetarian()
    prey.x, prey.y = 5000.0, 5000.0

    before = pred.energy
    assert pred.try_eat(_grid([prey]), pred.x, pred.y)
    assert not prey.alive
    assert pred.energy > before


def test_gain_includes_prey_biomass():
    """Хищник, съевший истощённую жертву, всё равно окупает переваривание."""
    pred = _pred(min_y=0, max_y=100)
    pred.x, pred.y = 5000.0, 5000.0
    pred.energy = 10.0
    prey = Vegetarian()
    prey.x, prey.y = 5000.0, 5000.0
    prey.energy = 0.0                       # совсем пустая жертва

    pred.try_eat(_grid([prey]), pred.x, pred.y)
    expected = config.PREY_BIOMASS_COEF * prey.size * config.PREDATION_EFFICIENCY
    assert abs(pred.energy - (10.0 + expected)) < 1e-9


def test_digesting_predator_does_not_hunt():
    pred = _pred(min_y=0, max_y=100)
    pred.x, pred.y = 5000.0, 5000.0
    first, second = Vegetarian(), Vegetarian()
    for prey in (first, second):
        prey.x, prey.y = 5000.0, 5000.0

    assert pred.try_eat(_grid([first, second]), pred.x, pred.y)
    assert pred.digest_ticks == config.PREDATOR_DIGEST_TICKS
    assert not pred.try_eat(_grid([second]), pred.x, pred.y)
    assert second.alive


def test_digest_counter_runs_down():
    pred = _pred()
    pred.digest_ticks = 3
    grid = _grid([])
    for expected in (2, 1, 0):
        pred.update(grid)
        assert pred.digest_ticks == expected


def test_prey_outside_layer_is_safe():
    pred = _pred(min_y=0, max_y=10)
    pred.x, pred.y = 5000.0, 500.0
    prey = Vegetarian()
    prey.x, prey.y = 5000.0, config.WORLD_HEIGHT * 0.9

    assert not pred.try_eat(_grid([prey]), pred.x, pred.y)
    assert prey.alive


def test_prey_flees_from_nearby_predator():
    prey = Vegetarian()
    prey.x, prey.y = 10000.0, 5000.0
    pred = _pred()
    pred.x, pred.y = prey.x + prey.vision * 0.1, prey.y

    prey._act(_grid([]), _grid([pred]))
    assert prey.flee_ticks > 0
    assert prey.x < 10000.0, "жертва должна уходить от хищника"


def test_distant_predator_does_not_trigger_flight():
    prey = Vegetarian()
    prey.x, prey.y = 10000.0, 5000.0
    pred = _pred()
    pred.x, pred.y = prey.x + prey.vision * 5, prey.y

    prey._act(_grid([]), _grid([pred]))
    assert prey.flee_ticks == 0
