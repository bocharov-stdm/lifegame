# headless.py — прогон симуляции без окна, с жёсткими лимитами.
#
# Один движок на двоих: им пользуются и тесты (tests/), и отчёт для подбора
# баланса (sim_report.py). Ничего не рисует и не импортирует дисплей.
#
# ПОЧЕМУ ЛИМИТОВ ТРИ, А НЕ ОДИН
# Стоимость тика квадратична по числу существ: каждое травоядное перебирает
# все растения и всех хищников. Значит числом тиков время прогона НЕ
# ограничивается — растёт популяция, дорожает тик. Поэтому кроме лимита тиков
# есть дедлайн по часам и потолок популяции; потолок заодно и есть главный
# сторож времени, потому что именно популяция разгоняет стоимость тика.

import time

from world import World
from predator import Predator
from vegetarian import Vegetarian


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
             predator_speed=None, predator_vision=None):
    """Гоняет симуляцию под четырьмя независимыми лимитами.

      ticks           — сколько тиков максимум
      max_creatures   — потолок популяции (смысловой: ловит взрыв численности)
      max_total_work  — бюджет вычислений (временной: гарантирует завершение)
      seconds         — дедлайн по часам (страховка на совсем медленной машине)

    Про max_total_work. Стоимость тика ≈ травоядные x растения: каждое
    травоядное перебирает все растения. Ограничивать это произведение
    поштучно бесполезно — время прогона определяется его СУММОЙ за все тики,
    и последние тики перед срабатыванием уже дорогие. Поэтому считаем именно
    накопленную работу. Замеры дают 5-7 млн операций/с, так что бюджет работы
    — это предсказуемое время, примерно одинаковое на разных машинах (в
    отличие от дедлайна по часам, который на медленной машине оборвал бы и
    здоровый прогон, сделав тест пустым).

    Ориентиры на текущем балансе: здоровый прогон расходует ~3.8 млн работы
    на 400 тиков (~0.7 с), ~7.3 млн на 1000 тиков (~2.5 с).
    Метрика приблизительная: она не учитывает линейный plants.remove() и
    спаун, поэтому при огромном числе растений реальная скорость ближе к
    нижней границе. Бюджет выставлен с учётом этого.

    Одного max_creatures недостаточно: взорваться могут растения, а счётчик
    существ этого не заметит — тик при этом станет неподъёмным.

    n_* и predator_* позволяют подбирать баланс, не трогая config.py.
    """
    world = World(seed=seed)

    if n_vegetarians is not None:
        world.vegetarians = [Vegetarian() for _ in range(n_vegetarians)]

    if n_predators is not None or predator_speed is not None or predator_vision is not None:
        count  = n_predators if n_predators is not None else len(world.predators)
        kwargs = {}
        if predator_speed  is not None: kwargs["speed"]  = predator_speed
        if predator_vision is not None: kwargs["vision"] = predator_vision
        world.predators = [Predator(**kwargs) for _ in range(count)]

    history     = [world.stats()]
    stop_reason = "готово"
    deadline    = time.perf_counter() + seconds
    started     = time.perf_counter()
    done        = 0
    total_work  = 0

    for done in range(1, ticks + 1):
        world.step()

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
