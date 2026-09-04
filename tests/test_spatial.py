"""Сетка обязана давать ровно то же, что перебор в лоб."""

import random

from lifegame.spatial import SpatialHash


class Point:
    def __init__(self, x, y):
        self.x, self.y = x, y


def _brute(points, x, y, r):
    return {id(p) for p in points if (p.x - x) ** 2 + (p.y - y) ** 2 <= r * r}


def test_query_matches_brute_force():
    random.seed(0)
    points = [Point(random.uniform(0, 10000), random.uniform(0, 10000))
              for _ in range(3000)]
    grid = SpatialHash(500)
    grid.rebuild(points)

    for _ in range(200):
        x = random.uniform(-500, 10500)
        y = random.uniform(-500, 10500)
        r = random.uniform(1, 2000)
        assert {id(p) for p in grid.query(x, y, r)} == _brute(points, x, y, r)


def test_nearest_matches_brute_force():
    random.seed(1)
    points = [Point(random.uniform(0, 5000), random.uniform(0, 5000))
              for _ in range(500)]
    grid = SpatialHash(400)
    grid.rebuild(points)

    for _ in range(100):
        x, y, r = random.uniform(0, 5000), random.uniform(0, 5000), random.uniform(1, 1500)
        inside = [p for p in points if (p.x - x) ** 2 + (p.y - y) ** 2 <= r * r]
        got = grid.nearest(x, y, r)
        if not inside:
            assert got is None
        else:
            best = min((p.x - x) ** 2 + (p.y - y) ** 2 for p in inside)
            assert abs((got.x - x) ** 2 + (got.y - y) ** 2 - best) < 1e-6


def test_nearest_respects_predicate():
    points = [Point(0, 0), Point(10, 0)]
    points[0].ok = False
    points[1].ok = True
    grid = SpatialHash(100)
    grid.rebuild(points)
    assert grid.nearest(0, 0, 100, predicate=lambda p: p.ok) is points[1]


def test_rebuild_forgets_previous_contents():
    grid = SpatialHash(100)
    grid.rebuild([Point(5, 5)])
    grid.rebuild([Point(900, 900)])
    assert grid.query(5, 5, 50) == []


def test_zero_radius_returns_nothing():
    grid = SpatialHash(100)
    grid.rebuild([Point(0, 0)])
    assert grid.query(0, 0, 0) == []
