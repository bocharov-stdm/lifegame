# stats.py — история прогона и её отрисовка.
#
# Средние гены в одной строке текста не позволяют увидеть, идёт эволюция или
# нет. История + графики + экспорт в CSV — минимум, чтобы прогон можно было
# разобрать.

import csv
import statistics

import pygame

from .config import GENE_NAMES, STATS_HISTORY


class History:
    """Кольцевой буфер срезов состояния мира."""

    def __init__(self, maxlen=STATS_HISTORY):
        self.maxlen = maxlen
        self.rows = []

    def record(self, world):
        vs, ps = world.vegetarians, world.predators
        row = {
            "tick": world.tick,
            "plants": len(world.plants),
            "vegetarians": len(vs),
            "predators": len(ps),
            "veg_energy": _mean(v.energy for v in vs),
            "pred_energy": _mean(p.energy for p in ps),
        }
        for i, name in enumerate(GENE_NAMES):
            row[f"veg_{name}"] = _mean(v.genom[i] for v in vs)
            row[f"veg_{name}_sd"] = _sd(v.genom[i] for v in vs)
            row[f"pred_{name}"] = _mean(p.genom[i] for p in ps)
        self.rows.append(row)
        if len(self.rows) > self.maxlen:
            del self.rows[0]
        return row

    def export_csv(self, path):
        if not self.rows:
            return None
        with open(path, "w", newline="", encoding="utf-8") as fh:
            writer = csv.DictWriter(fh, fieldnames=list(self.rows[0].keys()))
            writer.writeheader()
            writer.writerows(self.rows)
        return path


def _mean(values):
    values = list(values)
    return sum(values) / len(values) if values else 0.0


def _sd(values):
    values = list(values)
    return statistics.pstdev(values) if len(values) > 1 else 0.0


# ── отрисовка ────────────────────────────────────────────────────────────────
PANEL_BG = (18, 18, 24)
SERIES_COLORS = {
    "plants": (0, 220, 0),
    "vegetarians": (255, 100, 255),
    "predators": (220, 220, 220),
}


def draw_panel(surf, font, history, gene_name, rect):
    """Две панели: численности во времени и выбранный ген травоядных."""
    x, y, w, h = rect
    panel = pygame.Surface((w, h))
    panel.set_alpha(230)
    panel.fill(PANEL_BG)
    surf.blit(panel, (x, y))
    pygame.draw.rect(surf, (70, 70, 90), rect, 1)

    if len(history.rows) < 2:
        surf.blit(font.render("Собираю данные…", True, (200, 200, 200)), (x + 8, y + 8))
        return

    half = h // 2
    _plot(surf, font, history, ["plants", "vegetarians", "predators"],
          (x, y, w, half), "Численность")
    _plot(surf, font, history, [f"veg_{gene_name}", f"pred_{gene_name}"],
          (x, y + half, w, h - half), f"Ген: {gene_name}  (M — следующий)",
          colors=[(255, 100, 255), (220, 220, 220)])


def _plot(surf, font, history, keys, rect, title, colors=None):
    x, y, w, h = rect
    pad_l, pad_t, pad_b = 46, 18, 12
    plot_w = w - pad_l - 8
    plot_h = h - pad_t - pad_b
    rows = history.rows

    peak = max((row[k] for row in rows for k in keys), default=0.0)
    if peak <= 0:
        peak = 1.0

    surf.blit(font.render(title, True, (180, 180, 200)), (x + 6, y + 2))
    surf.blit(font.render(f"{peak:.0f}", True, (120, 120, 140)), (x + 4, y + pad_t))
    surf.blit(font.render("0", True, (120, 120, 140)), (x + 4, y + pad_t + plot_h - 12))

    step = plot_w / max(1, len(rows) - 1)
    for i, key in enumerate(keys):
        color = (colors[i] if colors else
                 SERIES_COLORS.get(key, (200, 200, 100)))
        points = [
            (x + pad_l + int(j * step),
             y + pad_t + plot_h - int(row[key] / peak * plot_h))
            for j, row in enumerate(rows)
        ]
        if len(points) > 1:
            pygame.draw.lines(surf, color, False, points, 1)
        label = key.replace("veg_", "трав. ").replace("pred_", "хищн. ")
        surf.blit(font.render(label, True, color),
                  (x + pad_l + 6 + i * 110, y + pad_t + plot_h - 14))
