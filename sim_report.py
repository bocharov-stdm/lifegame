"""Headless-отчёт о поведении симуляции — для подбора баланса без запуска окна.

    python sim_report.py                          # прогон по умолчанию
    python sim_report.py --ticks 1500 --seed 3
    python sim_report.py --predators 20 --predator-speed 18 --predator-vision 800
    python sim_report.py --seeds 1 2 3 42         # сводка по нескольким прогонам

Прогон всегда ограничен по тикам, по часам, по потолку популяции и по бюджету
вычислений, поэтому зависнуть или уехать в бесконечность не может.
"""

import argparse

from life.genome   import Genom, GENE_LABELS
from life.headless import simulate

# Бюджет вычислений растёт с длиной прогона. Прежний фиксированный (25 млн, как
# у тестов на 400 тиков) обрывал здоровые прогоны на 3000 тиков «перегрузкой»,
# а сводка при этом рапортовала, что всё в порядке. 60 тысяч на тик — та же
# доля, что у тестов, то есть примерно шестикратный запас над здоровым расходом.
WORK_PER_TICK = 60_000


def print_run(res, title):
    print(f"\n=== {title} ===")
    print(f"{'тик':>6} {'растения':>9} {'травоядные':>11} {'хищники':>8} {'ср.энергия':>11}")
    for h in res.history:
        energy = f"{h['avg_energy']:.1f}" if h["avg_energy"] is not None else "—"
        print(f"{h['tick']:>6} {h['plants']:>9} {h['vegetarians']:>11} "
              f"{h['predators']:>8} {energy:>11}")

    final = res.final
    print(f"\nостановка: {res.stop_reason} на тике {res.ticks_done}")
    print(f"пик:       травоядные {res.peak('vegetarians')}, хищники {res.peak('predators')}, "
          f"растения {res.peak('plants')}")
    print(f"скорость:  {res.ms_per_tick():.1f} мс/тик "
          f"(бюджет кадра 60 FPS — 16.7 мс), всего {res.elapsed:.1f} с")

    if final["avg_genom"]:
        print("\nсредний геном на финише:")
        for name, value in zip(Genom._fields, final["avg_genom"]):
            print(f"  {GENE_LABELS[name]:<14} {value:>8.1f}")


def print_summary(rows):
    print(f"\n=== сводка по {len(rows)} прогонам ===")
    print(f"{'seed':>6} {'тиков':>7} {'травояд':>9} {'хищн':>6} {'растен':>8} "
          f"{'мс/тик':>8}  остановка")
    for seed, res in rows:
        f = res.final
        print(f"{seed:>6} {res.ticks_done:>7} {f['vegetarians']:>9} {f['predators']:>6} "
              f"{f['plants']:>8} {res.ms_per_tick():>8.1f}  {res.stop_reason}")

    exploded = [s for s, r in rows if r.exploded]
    extinct  = [s for s, r in rows if r.extinct]
    # перегрузка и дедлайн — не приговор балансу, но и не проверка: прогон
    # до конца не дошёл, и молчать об этом нельзя
    cut      = [f"{s} ({r.stop_reason} на тике {r.ticks_done})"
                for s, r in rows if not (r.ok or r.exploded or r.extinct)]
    print()
    if exploded:
        print(f"ВЗРЫВ ЧИСЛЕННОСТИ на seed: {exploded}")
    if extinct:
        print(f"ВЫМИРАНИЕ на seed: {extinct}")
    if cut:
        print(f"ОБОРВАНЫ РАНЬШЕ СРОКА seed: {', '.join(cut)} — поднимите --max-work или --seconds")
    if not (exploded or extinct or cut):
        print("все прогоны в разумном коридоре: без взрыва и без вымирания")


def main():
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--seed",   type=int, default=1)
    p.add_argument("--seeds",  type=int, nargs="+", help="несколько прогонов подряд")
    p.add_argument("--ticks",  type=int, default=600)
    p.add_argument("--sample", type=int, default=100, help="шаг снимка статистики")
    p.add_argument("--seconds", type=float, default=30.0, help="дедлайн одного прогона")
    p.add_argument("--max-creatures", type=int, default=3000, help="потолок популяции")
    p.add_argument("--max-work", type=int,
                   help=f"бюджет вычислений (по умолчанию {WORK_PER_TICK:,} на тик)")
    p.add_argument("--vegetarians", type=int, help="стартовое число травоядных")
    p.add_argument("--predators",   type=int, help="стартовое число хищников")
    p.add_argument("--predator-speed",  type=float)
    p.add_argument("--predator-vision", type=float)
    args = p.parse_args()

    max_work = args.max_work if args.max_work is not None else WORK_PER_TICK * args.ticks

    common = dict(
        ticks=args.ticks, sample_every=args.sample,
        seconds=args.seconds, max_creatures=args.max_creatures,
        max_total_work=max_work,
        n_vegetarians=args.vegetarians, n_predators=args.predators,
        predator_speed=args.predator_speed, predator_vision=args.predator_vision,
    )

    if args.seeds:
        rows = []
        for seed in args.seeds:
            res = simulate(seed=seed, **common)
            print_run(res, f"seed {seed}")
            rows.append((seed, res))
        print_summary(rows)
    else:
        print_run(simulate(seed=args.seed, **common), f"seed {args.seed}")


if __name__ == "__main__":
    main()
