"""Мутации: границы, процентные гены, инварианты слоя."""

import random

import lifegame.config as config
from lifegame.organism import Organism
from lifegame.vegetarian import Vegetarian


def test_percent_genes_stay_in_range():
    random.seed(0)
    v = Vegetarian()
    for _ in range(500):
        genom = v.mutate()
        for value, (_n, _b, _k, lo, hi) in zip(genom, config.GENES):
            assert lo is None or value >= lo - 1e-9
            assert hi is None or value <= hi + 1e-9


def test_zero_percent_gene_can_escape_zero():
    """Регрессия: мультипликативная мутация запирала ген min_y=0 в нуле навсегда."""
    random.seed(1)
    genom = list(config.VEGETARIAN_BASE_GENOM)
    genom[config.GENE_INDEX["min_y"]] = 0.0
    genom[config.GENE_INDEX["max_y"]] = 0.0
    v = Vegetarian(genom=genom)

    i = config.GENE_INDEX["min_y"]
    assert any(v.mutate()[i] > 0 for _ in range(200))


def test_multiplicative_genes_stay_positive():
    random.seed(2)
    v = Vegetarian()
    for _ in range(500):
        genom = v.mutate()
        for value, (_n, _b, kind, _lo, _hi) in zip(genom, config.GENES):
            if kind == "mul":
                assert value > 0


def test_min_y_never_exceeds_max_y():
    random.seed(3)
    v = Vegetarian()
    i_min, i_max = config.GENE_INDEX["min_y"], config.GENE_INDEX["max_y"]
    for _ in range(500):
        genom = v.mutate()
        assert genom[i_min] <= genom[i_max]
        child = Vegetarian(genom=genom)
        assert child.min_y <= child.max_y


def test_swapped_layer_genes_are_normalised():
    genom = list(config.VEGETARIAN_BASE_GENOM)
    genom[config.GENE_INDEX["min_y"]] = 80.0
    genom[config.GENE_INDEX["max_y"]] = 20.0
    v = Vegetarian(genom=genom)
    assert (v.min_y, v.max_y) == (20.0, 80.0)


def test_genome_maps_to_named_attributes():
    v = Vegetarian()
    for name, i in config.GENE_INDEX.items():
        assert getattr(v, name) == v.genom[i]


def test_layer_bounds_never_inverted_for_thin_layer():
    genom = list(config.VEGETARIAN_BASE_GENOM)
    genom[config.GENE_INDEX["min_y"]] = 50.0
    genom[config.GENE_INDEX["max_y"]] = 50.0
    v = Vegetarian(genom=genom)          # слой тоньше тела
    y_min, y_max = v.layer_bounds()
    assert y_min <= y_max
    assert isinstance(v, Organism)
