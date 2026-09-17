# rules.py — правила мира, которые можно менять, не трогая config.py.
#
# Раньше сущности читали такие величины прямо из глобальных констант конфига.
# Поменять их на лету было нельзя: подмена глобалов — это одно состояние на
# весь процесс, а миров бывает два сразу (фоновый в меню и игровой). Теперь
# у каждого мира свой Rules, и существа получают его при рождении.
#
# Значения по умолчанию — ровно константы из config.py, и арифметика с ними
# та же бит в бит: прогоны по умолчанию не меняются ни на одно число.

import math
from dataclasses import dataclass, field, fields, replace

from .config import *

# Стоимость содержания нормирована по базовому геному (см. config.py): цену
# каждого стата при смене показателя пересчитываем так, чтобы базовый стат
# стоил столько же, сколько при показателе из конфига.
_BASE_SIZE, _BASE_SPEED, _BASE_VISION = VEGETARIAN_BASE_GENOM[:3]


@dataclass(frozen=True)
class Rules:
    """Правила одного мира. Неизменяемые: меняются заменой целиком (with_)."""

    plant_rate:             float = PLANT_SPAWN_CHANCE      # растений за тик (не вероятность)
    plant_energy:           float = ENERGY_FROM_PLANT       # энергии за одно растение
    mutation_sigma:         float = VEGETARIAN_SIGMA        # разброс мутаций травоядных
    cost_scale:             float = 1.0                     # множитель ко всей цене статов
    size_power:             float = SIZE_ENERGY_POWER       # крутизна цены размера
    speed_power:            float = SPEED_ENERGY_POWER      # крутизна цены скорости
    sight_power:            float = SIGHT_ENERGY_POWER      # крутизна цены зрения
    predator_divide_chance: float = PREDATOR_DIVIDE_CHANCE  # шанс деления хищника
    predator_max_energy:    float = PREDATOR_MAX_ENERGY     # запас энергии хищника

    # производные коэффициенты — считаются один раз в __post_init__
    size_coef:  float = field(init=False, repr=False, compare=False)
    speed_coef: float = field(init=False, repr=False, compare=False)
    sight_coef: float = field(init=False, repr=False, compare=False)

    def __post_init__(self):
        # Смена показателя меняет только КРУТИЗНУ: базовый стат стоит столько же,
        # сколько стоил. Иначе показатель 1.5 вместо 2.5 сделал бы размер почти
        # бесплатным целиком, и опыт мерил бы не то. При показателях из конфига
        # множитель — base ** 0.0, то есть ровно 1.0, и числа не сдвигаются.
        # NaN и бесконечность ломают не арифметику, а циклы: mutate() ждёт
        # gauss >= -0.9, а с сигмой NaN сравнение не выполнится никогда.
        for key in self.keys():
            value = getattr(self, key)
            if not math.isfinite(value):
                raise ValueError(f"правило {key}: нужно конечное число, а не {value!r}")
        set_ = object.__setattr__           # dataclass заморожен
        set_(self, "size_coef", SIZE_ENERGY_COEF * self.cost_scale
             * _BASE_SIZE ** (SIZE_ENERGY_POWER - self.size_power))
        set_(self, "speed_coef", SPEED_ENERGY_COEF * self.cost_scale
             * _BASE_SPEED ** (SPEED_ENERGY_POWER - self.speed_power))
        set_(self, "sight_coef", SIGHT_ENERGY_COEF * self.cost_scale
             * _BASE_VISION ** (SIGHT_ENERGY_POWER - self.sight_power))

    def upkeep(self, size, speed, vision):
        """Расход энергии за тик: COEF * стат ** POWER, суммарно по трём статам."""
        return (self.size_coef  * size   ** self.size_power +
                self.speed_coef * speed  ** self.speed_power +
                self.sight_coef * vision ** self.sight_power)

    def with_(self, **changes):
        """Копия с другими значениями; неизвестный ключ — ошибка, а не молчание."""
        return replace(self, **changes)

    @classmethod
    def keys(cls):
        """Имена настраиваемых правил (без производных коэффициентов)."""
        return tuple(f.name for f in fields(cls) if f.init)


DEFAULT_RULES = Rules()
