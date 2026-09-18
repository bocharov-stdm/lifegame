"""fingerprint.py — «отпечаток» баланса Python-версии для сверки с ядром на Rust.

    python fingerprint.py                     # 8 сидов x 20 000 тиков → ../reference/fingerprint.json
    python fingerprint.py --ticks 3000 --seeds 1 2

Бит в бит новое ядро с Python не совпадёт (другой генератор случайных чисел,
параллельный тик), поэтому сверяется статистика: численность, средний геном,
доля времени с хищниками, период колебаний. Окна «3000» и «весь прогон» —
короткий баланс из CLAUDE.md и долгая жизнь мира. Первые 3000 тиков длинного
прогона и есть короткий прогон с тем же сидом, считать его отдельно незачем.

Кроме сводок в файл пишутся сами ряды (раз в SAMPLE тиков): по ним потом можно
посчитать любую метрику, не гоняя Python заново.
"""

import argparse
import json
import multiprocessing as mp
import statistics
import time
from pathlib import Path

from life.config   import DIVIDE_PERIOD
from life.headless import simulate

SAMPLE  = 60                    # шаг снимков, тиков: кратно DIVIDE_PERIOD, чтобы пила деления не шумела
WINDOWS = (3000, None)          # None — весь прогон
OUT     = Path(__file__).resolve().parent.parent / "reference" / "fingerprint.json"

GENES = ("size", "speed", "vision", "repro_threshold", "repro_share", "min_y", "max_y")


def _run(job):
    seed, ticks = job
    # Лимиты широкие: нужен честный долгий прогон, а не страховка от зависания
    # тестов. Дедлайн всё равно есть — прогон не может длиться вечно.
    res = simulate(seed=seed, ticks=ticks, sample_every=SAMPLE, seconds=900,
                   max_creatures=50_000, max_total_work=10**13)
    series = [{"tick": h["tick"], "plants": h["plants"], "vegetarians": h["vegetarians"],
               "predators": h["predators"], "genom": h["avg_genom"]}
              for h in res.history]
    return {"seed": seed, "stop": res.stop_reason, "ticks_done": res.ticks_done,
            "migrants": res.world.migrants, "ms_per_tick": round(res.ms_per_tick(), 3),
            "series": series}


def _quantiles(values):
    if not values:
        return None
    q = statistics.quantiles(values, n=10) if len(values) > 1 else [values[0]] * 9
    return {"mean": statistics.fmean(values), "p10": q[0], "p50": q[4], "p90": q[8]}


def _period(values):
    """Период колебаний, тиков: первый пик автокорреляции после нуля (>0.2), иначе None.

    Сначала ряд сглаживается окном в DIVIDE_PERIOD: деление идёт залпами, и без
    этого первым пиком оказывалась пила деления, а не «хищник — жертва».
    """
    w = max(1, DIVIDE_PERIOD // SAMPLE)
    values = [statistics.fmean(values[i:i + w]) for i in range(len(values) - w + 1)]
    n = len(values)
    if n < 20:
        return None
    mean = statistics.fmean(values)
    dev = [v - mean for v in values]
    var = sum(d * d for d in dev)
    if var == 0:
        return None
    ac = [sum(dev[i] * dev[i + lag] for i in range(n - lag)) / var for lag in range(n // 2)]
    for lag in range(2, len(ac) - 1):
        if ac[lag] > 0.2 and ac[lag] >= ac[lag - 1] and ac[lag] >= ac[lag + 1]:
            return lag * SAMPLE
    return None


def _summary(series, limit):
    rows = [s for s in series if limit is None or s["tick"] <= limit]
    alive = [s for s in rows if s["genom"]]
    out = {k: _quantiles([s[k] for s in rows]) for k in ("plants", "vegetarians", "predators")}
    out["predators_alive"] = sum(s["predators"] > 0 for s in rows) / len(rows)
    out["predator_period"] = _period([s["predators"] for s in rows])
    out["genom_final"] = dict(zip(GENES, alive[-1]["genom"])) if alive else None
    out["size_max"] = max((s["genom"][0] for s in alive), default=None)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--seeds", type=int, nargs="+", default=list(range(1, 9)))
    ap.add_argument("--ticks", type=int, default=20_000)
    ap.add_argument("--out", type=Path, default=OUT)
    args = ap.parse_args()

    started = time.perf_counter()
    with mp.Pool(min(len(args.seeds), mp.cpu_count())) as pool:
        runs = pool.map(_run, [(s, args.ticks) for s in args.seeds])

    for run in runs:
        run["windows"] = {str(w or "all"): _summary(run["series"], w) for w in WINDOWS}
        all_ = run["windows"]["all"]
        print(f"сид {run['seed']}: {run['stop']:<18} тиков {run['ticks_done']:>6}  "
              f"трав ~{all_['vegetarians']['mean']:5.0f}  хищ ~{all_['predators']['mean']:4.0f}  "
              f"хищники живы {all_['predators_alive']:4.0%}  размер макс {all_['size_max'] or 0:5.1f}  "
              f"период {all_['predator_period']}  {run['ms_per_tick']} мс/тик")

    args.out.parent.mkdir(parents=True, exist_ok=True)
    data = {"sample_every": SAMPLE, "ticks": args.ticks, "genes": GENES, "runs": runs}
    args.out.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    print(f"\n→ {args.out}  ({time.perf_counter() - started:.0f} с)")


if __name__ == "__main__":
    main()
