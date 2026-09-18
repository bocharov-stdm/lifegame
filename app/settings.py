# settings.py — что игрок может настроить, в каких пределах и где это хранится.
#
# FIELDS — единственное место, где описан каждый слайдер: подпись, пояснение,
# пределы, шаг и формат. По нему строится экран «Новый мир», и по нему же
# зажимаются значения из файла настроек — поэтому они не могут разойтись.
#
# pygame здесь не нужен: модуль проверяют тесты без дисплея.

import json
import math
import os
import random
import tempfile
from dataclasses import dataclass, fields, replace
from pathlib import Path
from typing import NamedTuple

from life.config import (PREDATOR_BASE_SPEED, PREDATOR_BASE_VISION,
                         PREDATORS_AT_START, VEGETARIANS_AT_START)
from life.rules  import DEFAULT_RULES
from life.world  import World

SETTINGS_PATH = Path(__file__).resolve().parent.parent / "user_settings.json"

SEED_MAX   = 99_999
UI_SCALES  = (0.0, 1.0, 1.25, 1.5, 1.75, 2.0)      # 0 — как в системе


class Field(NamedTuple):
    key:   str
    label: str
    hint:  str
    lo:    float
    hi:    float
    step:  float
    fmt:   object       # формат значения: строка для str.format или функция
    tab:   str          # "world" — вкладка «Мир», "lab" — «Лаборатория»

    @property
    def is_int(self):
        return float(self.step).is_integer() and float(self.lo).is_integer()

    def snap(self, value):
        """Значение в пределах и на сетке шага (так его ставит слайдер)."""
        value = min(max(float(value), self.lo), self.hi)
        value = self.lo + round((value - self.lo) / self.step) * self.step
        value = round(min(value, self.hi), 6)       # без хвостов вроде 0.30000000000000004
        return int(value) if self.is_int else value

    def format(self, value):
        return self.fmt(value) if callable(self.fmt) else self.fmt.format(value)


FIELDS = (
    # ── Мир ──────────────────────────────────────────────────────────────────
    Field("n_vegetarians", "Травоядных на старте",
          "Сколько травоядных в мире в первый момент.",
          1, 200, 1, "{:.0f}", "world"),
    Field("n_predators", "Хищников на старте",
          "Сколько хищников на старте. Ноль — мир без охоты.",
          0, 40, 1, "{:.0f}", "world"),
    # Множитель, а не само число: в конфиге темп — 2.50008 в тик, и на сетку
    # слайдера он не ложится. Множитель 1.0 даёт ровно конфиг, бит в бит.
    Field("plant_growth", "Рост растений",
          "Сколько растений появляется за тик. Больше еды — больше травоядных.",
          0.2, 3.0, 0.1, lambda v: f"{v * DEFAULT_RULES.plant_rate:.1f} в тик", "world"),
    Field("predator_speed", "Скорость хищников",
          "Скорость хищников на старте. У потомков она мутирует.",
          4, 30, 1, "{:.0f}", "world"),
    Field("predator_vision", "Зрение хищников",
          "С какого расстояния хищник замечает добычу. Травоядное видит на 400.",
          100, 1500, 50, "{:.0f}", "world"),
    # ── Лаборатория ─────────────────────────────────────────────────────────
    Field("mutation_sigma", "Сила мутаций",
          "Насколько гены потомка отличаются от родительских. "
          "Мало — эволюция стоит, много — хаос.",
          0.05, 1.0, 0.05, "{:.2f}", "lab"),
    Field("plant_energy", "Энергия растения",
          "Сколько энергии даёт одно растение. Полный бак базового травоядного — 100.",
          10, 150, 5, "{:.0f}", "lab"),
    Field("cost_scale", "Цена статов",
          "Множитель ко всей цене содержания: размера, скорости и зрения.",
          0.25, 4.0, 0.25, "×{:.2f}", "lab"),
    Field("size_power", "Крутизна цены размера",
          "Как быстро дорожает размер. Ниже 2 крупное тело окупается, "
          "и размер раздувается без предела.",
          1.0, 3.5, 0.1, "{:.1f}", "lab"),
    Field("sight_power", "Крутизна цены зрения",
          "Как быстро дорожает зрение. Чем ниже, тем дешевле дальнозоркость "
          "и тем дальше видят потомки.",
          1.0, 3.0, 0.1, "{:.1f}", "lab"),
    Field("predator_divide_chance", "Плодовитость хищников",
          "Шанс, что сытый хищник поделится. Высокий — хищники выедают всех "
          "и гибнут следом.",
          0.02, 1.0, 0.01, "{:.0%}", "lab"),
    Field("predator_max_energy", "Запас энергии хищника",
          "Сколько энергии вмещает хищник, то есть как долго он живёт без добычи.",
          30, 300, 5, "{:.0f}", "lab"),
    Field("predator_migration", "Миграция хищников",
          "Когда хищников почти не осталось, раз в столько тиков с края мира "
          "приходит новый. Ноль — хищники могут вымереть навсегда.",
          0, 2000, 100, lambda v: f"раз в {v:.0f}" if v else "выкл", "lab"),
)
FIELD_BY_KEY = {f.key: f for f in FIELDS}

# поля Settings, которые уходят в life.rules.Rules
RULE_KEYS = tuple(f.key for f in FIELDS if f.key in DEFAULT_RULES.keys())


@dataclass
class Settings:
    # ── старт ──────────────────────────────────────────────────────────────
    seed:            int   = 1
    random_seed:     bool  = True           # новый сид на каждый «Начать»
    n_vegetarians:   int   = VEGETARIANS_AT_START
    n_predators:     int   = PREDATORS_AT_START
    predator_speed:  float = PREDATOR_BASE_SPEED
    predator_vision: float = PREDATOR_BASE_VISION
    # ── правила мира (life/rules.py) ─────────────────────────────────────────
    plant_growth:           float = 1.0     # множитель к темпу роста из конфига
    plant_energy:           float = DEFAULT_RULES.plant_energy
    mutation_sigma:         float = DEFAULT_RULES.mutation_sigma
    cost_scale:             float = DEFAULT_RULES.cost_scale
    size_power:             float = DEFAULT_RULES.size_power
    sight_power:            float = DEFAULT_RULES.sight_power
    predator_divide_chance: float = DEFAULT_RULES.predator_divide_chance
    predator_max_energy:    float = DEFAULT_RULES.predator_max_energy
    predator_migration:     float = DEFAULT_RULES.predator_migration
    # ── экран ────────────────────────────────────────────────────────────────
    fullscreen: bool  = False
    ui_scale:   float = 0.0                 # 0 — как в системе
    smooth:     bool  = True                # сглаженные существа
    show_fps:   bool  = False

    def copy(self):
        return replace(self)

    def rules(self):
        changes = {k: getattr(self, k) for k in RULE_KEYS}
        changes["plant_rate"] = DEFAULT_RULES.plant_rate * self.plant_growth
        return DEFAULT_RULES.with_(**changes)

    def make_world(self, seed):
        return World(seed=seed, rules=self.rules(),
                     n_vegetarians=int(self.n_vegetarians),
                     n_predators=int(self.n_predators),
                     predator_speed=self.predator_speed,
                     predator_vision=self.predator_vision)

    def roll_seed(self):
        """Новый случайный сид. Отдельный генератор: общий принадлежит миру."""
        self.seed = random.Random().randint(1, SEED_MAX)
        return self.seed

    def reset(self, tab):
        """Вернуть значения по умолчанию на одной вкладке («world» или «lab»)."""
        default = Settings()
        for f in FIELDS:
            if f.tab == tab:
                setattr(self, f.key, getattr(default, f.key))
        if tab == "world":
            self.random_seed = default.random_seed

    def is_default(self, key):
        return getattr(self, key) == getattr(Settings(), key)


# ── файл настроек ────────────────────────────────────────────────────────────

_BOOL_KEYS = {f.name for f in fields(Settings) if f.type in (bool, "bool")}


def _clean(key, value):
    """Значение из файла, приведённое к допустимому, или None, если оно негодное."""
    if key in _BOOL_KEYS:
        return value if isinstance(value, bool) else None
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    if not math.isfinite(value):
        return None
    if key in FIELD_BY_KEY:
        return FIELD_BY_KEY[key].snap(value)
    if key == "seed":
        return int(min(max(round(value), 1), SEED_MAX))
    if key == "ui_scale":
        return min(UI_SCALES, key=lambda s: abs(s - value))
    return None


def load(path=SETTINGS_PATH):
    """Настройки из файла. Битый файл, мусор и чужие ключи не роняют игру."""
    settings = Settings()
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return settings
    if not isinstance(data, dict):
        return settings

    for f in fields(Settings):
        if f.name in data:
            value = _clean(f.name, data[f.name])
            if value is not None:
                setattr(settings, f.name, value)
    return settings


def save(settings, path=SETTINGS_PATH):
    """Пишет атомарно: сначала во временный файл, потом подменяет.

    Оборванная запись не оставит полфайла. Ошибка записи игру не роняет —
    настройки просто не запомнятся; возвращается False.
    """
    path = Path(path)
    data = {f.name: getattr(settings, f.name) for f in fields(Settings)}
    tmp = None
    try:
        fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=".settings-", suffix=".tmp")
        with os.fdopen(fd, "w", encoding="utf-8") as out:
            json.dump(data, out, ensure_ascii=False, indent=2)
        os.replace(tmp, path)
        return True
    except OSError:
        if tmp is not None:
            try:
                os.unlink(tmp)
            except OSError:
                pass
        return False
