"""Размножение: энергетический баланс и изоляция потомства от текущего тика."""

import random

import lifegame.config as config
from lifegame.predator import Predator
from lifegame.vegetarian import Vegetarian
from lifegame.world import World


def _veg(**genes):
    genom = list(config.VEGETARIAN_BASE_GENOM)
    for name, value in genes.items():
        genom[config.GENE_INDEX[name]] = value
    return Vegetarian(genom=genom)


def test_parent_never_goes_below_reserve():
    random.seed(0)
    for _ in range(300):
        v = _veg(repro_threshold=random.uniform(5, 100),
                 repro_share=random.uniform(5, 90))
        v.energy = random.uniform(0, v.max_energy)
        offspring = []
        v.maybe_divide(offspring)
        assert v.energy >= 0
        if offspring:
            assert v.energy >= v.max_energy * config.REPRO_RESERVE_FRAC - 1e-9


def test_child_energy_never_exceeds_its_max():
    random.seed(1)
    for _ in range(200):
        v = _veg(repro_share=random.uniform(5, 90))
        v.energy = v.max_energy
        offspring = []
        v.maybe_divide(offspring)
        for child in offspring:
            assert 0 < child.energy <= child.max_energy


def test_energy_is_conserved_minus_cost():
    v = _veg()
    v.energy = v.max_energy
    before = v.energy
    offspring = []
    v.maybe_divide(offspring)
    assert offspring
    spent = before - v.energy - offspring[0].energy
    assert abs(spent - v.max_energy * config.REPRO_COST_FRAC) < 1e-9


def test_starving_organism_does_not_divide():
    v = _veg()
    v.energy = v.max_energy * 0.01
    offspring = []
    assert v.maybe_divide(offspring) is None
    assert offspring == []


def test_child_is_same_species():
    for parent in (_veg(), Predator()):
        parent.energy = parent.max_energy
        offspring = []
        parent.maybe_divide(offspring)
        assert offspring and type(offspring[0]) is type(parent)


def test_newborn_does_not_act_in_its_birth_tick(monkeypatch):
    """Регрессия: раньше дети попадали в список, по которому шёл текущий тик."""
    random.seed(2)
    world = World(seed=2)
    while world.tick % config.REPRO_EVERY != 0:
        world.step()

    acted = set()
    original = Vegetarian.update

    def spy(self, *args, **kwargs):
        acted.add(id(self))
        return original(self, *args, **kwargs)

    monkeypatch.setattr(Vegetarian, "update", spy)

    before = {id(v) for v in world.vegetarians}
    world.step()
    newborns = [v for v in world.vegetarians if id(v) not in before]

    assert newborns, "в этом тике никто не размножился — тест бессмыслен"
    for child in newborns:
        assert id(child) not in acted, "новорождённый успел походить в свой первый тик"


def test_population_lists_never_contain_dead():
    random.seed(3)
    world = World(seed=3)
    for _ in range(300):
        world.step()
        assert all(p.alive for p in world.plants)
        assert all(v.alive for v in world.vegetarians)
        assert all(p.alive for p in world.predators)
