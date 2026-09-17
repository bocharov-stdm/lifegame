# headless.py — прогон симуляции без окна, с жёсткими лимитами.
#
# Один движок на двоих: им пользуются и тесты (tests/), и отчёт для подбора
# баланса (sim_report.py). Ничего не рисует и не импортирует дисплей.
#
# ПОЧЕМУ ЛИМИТОВ ТРИ, А НЕ ОДИН
# Стоимость тика растёт вместе с популяцией, поэтому числом тиков время прогона
# НЕ ограничивается: растёт популяция — дорожает тик. Кроме лимита тиков есть
# дедлайн по часам и потолок популяции; потолок заодно и главный сторож времени,
# потому что именно популяция разгоняет стоимость тика.
#
# Сетка соседей (life/grid.py) сделала этот рост почти линейным вместо квадратичного,
# но не отменила его — лимиты нужны по-прежнему.

import time

from .world import World


class SimResult:
    """Итог прогона: траектория, причина остановки и замер скорости."""

    def __init__(self, history, ticks_done, stop_reason, elapsed, world, total_work):
        self.history     = history       # список снимков world.stats()
        self.ticks_done  = ticks_done
        self.stop_reason = stop_reason   # готово | взрыв численности | перегрузка | вымерли | дедлайн
        self.elapsed     = elapsed
        self.world       = world         # конечное состояние — для проверки инвариантов
        self.total_work  = total_work    # травоядные x растения, просуммировано по тикам

    @property
    def exploded(self):
        return self.stop_reason == "взрыв численности"

    @property
    def overloaded(self):
        return self.stop_reason == "перегрузка"

    @property
    def ok(self):
        return self.stop_reason == "готово"

    @property
    def extinct(self):
        return self.stop_reason == "вымерли"

    @property
    def final(self):
        return self.history[-1]

    def peak(self, key):
        return max(h[key] for h in self.history)

    def ms_per_tick(self):
        return self.elapsed / self.ticks_done * 1000 if self.ticks_done else 0.0


def simulate(seed=None, ticks=400, sample_every=100,
             seconds=15.0, max_creatures=3000, max_total_work=25_000_000,
             n_vegetarians=None, n_predators=None,
             predator_speed=None, predator_vision=None,
             rules=None, on_tick=None):
    """Гоняет симуляцию под четырьмя независимыми лимитами.

      ticks           — сколько тиков максимум
      max_creatures   — потолок популяции (смысловой: ловит взрыв численности)
      max_total_work  — бюджет вычислений (временной: гарантирует завершение)
      seconds         — дедлайн по часам (страховка на совсем медленной машине)

    Про max_total_work. Метрика — «травоядные x растения», просуммированные
    по тикам. Ограничивать это произведение поштучно бесполезно: время
    прогона определяется его СУММОЙ за все тики, а последние тики перед
    срабатыванием уже дорогие. Бюджет работы — это предсказуемое время,
    примерно одинаковое на разных машинах (в отличие от дедлайна по часам,
    который на медленной машине оборвал бы и здоровый прогон, сделав тест
    пустым).

    Метрика намеренно осталась прежней и после перехода на сетку соседей,
    хотя теперь она сильно КОНСЕРВАТИВНА: существо больше не перебирает все
    растения, а только соседей по клетке. То есть это по-прежнему честная
    верхняя оценка — просто с запасом, и как гарантия завершения она работает.

    Ориентиры на текущем балансе: здоровый прогон расходует ~3.6 млн работы
    на 400 тиков (~0.3 с).

    Одного max_creatures недостаточно: взорваться могут растения, а счётчик
    существ этого не заметит — тик при этом станет неподъёмным.

    n_*, predator_* и rules (life/rules.py) позволяют подбирать баланс, не
    трогая config.py; None — значение из конфига.

    on_tick(world) вызывается после каждого тика — для тех, кому мало снимков
    раз в sample_every (например, чтобы копить свои точки потиково).
    """
    start = {}
    if n_vegetarians   is not None: start["n_vegetarians"]   = n_vegetarians
    if n_predators     is not None: start["n_predators"]     = n_predators
    if predator_speed  is not None: start["predator_speed"]  = predator_speed
    if predator_vision is not None: start["predator_vision"] = predator_vision
    world = World(seed=seed, rules=rules, **start)

    history     = [world.stats()]
    stop_reason = "готово"
    deadline    = time.perf_counter() + seconds
    started     = time.perf_counter()
    done        = 0
    total_work  = 0

    for done in range(1, ticks + 1):
        world.step()
        if on_tick is not None:
            on_tick(world)

        creatures   = len(world.vegetarians) + len(world.predators)
        total_work += len(world.vegetarians) * len(world.plants)

        if creatures > max_creatures:
            stop_reason = "взрыв численности"
            break
        if total_work > max_total_work:
            stop_reason = "перегрузка"
            break
        if creatures == 0:
            stop_reason = "вымерли"
            break
        if time.perf_counter() > deadline:
            stop_reason = "дедлайн"
            break

        if done % sample_every == 0:
            history.append(world.stats())

    elapsed = time.perf_counter() - started

    if history[-1]["tick"] != world.tick:     # финальный снимок всегда в истории
        history.append(world.stats())

    return SimResult(history, done, stop_reason, elapsed, world, total_work)
