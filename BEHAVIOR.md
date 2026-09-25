# Реформа поведения Tiny Life

## Flocks as feeding circles, battles for room (this stage)

Formats: `life-report/9`, model `life-behavior/7`. This section is in English; the rest of the file
is translated in a separate commit.

**The circle.** A family flock of two or more members is a circle that moves as one object, and
its members feed inside it. Radius: `flock_spacing · √n`, clamped to 80–600 (`flock::MIN_RADIUS`,
`MAX_RADIUS`); `flock_spacing` is a free inherited gene (effect 50–500, base 200). Members take
plants, corpses and prey only inside the circle (its border plus half a body); a chase already
started may lead out of it. With no food in sight they wander inside the circle, and outside it
they walk back (`Mode::Return`, shown as «собираются»). A member below 40% of its store forages
anywhere it sees food and keeps foraging until it has 70% again, so it does not dart back to its
circle after every bite. A member that stays outside its circle for 300 ticks without being on
guard, fighting or foraging leaves the flock with a new label (`strays`). Removed: the old «below
85% search on your own» rule and the pull towards the neighbours' centre.

**A young family** (fewer than 4 members) roams: its circle follows the members' mean position,
never faster than they walk, and is at least as wide as they see. A new family thus forages
almost like loners; from four members on the circle is an object that moves by the flock kind.
Without this rule flocks died out early in about half of the calm worlds: a small circle feeds
worse than free search while the world is empty.

**Flock kinds** (`flock_kind`, dealt in turn among the flocking founders, 25% each):
- settled: the circle stays; it moves when mean fullness stays below 0.4 for 600 ticks or no
  member sees a plant inside for 300 ticks;
- nomadic: a slow drift along x, turning at the world's edge or after 30 ticks pushed back;
- scouts: the best fresh food report of a member (every 60 ticks, held 180); without reports a
  wander 0.5–3 vision away;
- vertical migrants: a period of 1200 ticks, half up and half down; within the layer when bound,
  between 5% and 50% of depth when free.

A circle moves at `SLOW_PACE` of its members' mean speed and waits while fewer than 60% of them
are inside. `layer_bound` (a quarter of the founders is free, by a hash, not a draw): a bound
flock's circle stays in the members' mean layer; a free creature's home band is the whole depth.
Flock kind and the layer switch are part of the family mode: a child with another one leaves.

**Overlaps.** Two circles without territoriality overlap freely. With a moderate one involved the
pair is soft: 5% of the overlap is pushed apart per tick on each side, and both shrink by 2.5% of
it. With a hard one involved the pair is strict: the non-hard circle yields fully, two hard ones
half each; whatever overlap six passes of pushing leave (a wall of the world or of the layer, a
crowd) is removed by shrinking, on every flock update, so no strict pair ever overlaps. A
squeezed circle grows back by 0.5% of its nominal radius per tick. Kinship and the grace after a
split forbid only strikes: kin circles are separated like any others.

**No room.** A circle squeezed to at most half of its radius for 60 ticks looks for room for its
full circle nearby (four rings of candidates, 2r to 8r away, in the flock's layer). If there is
some, the flock moves there, even to a poorer, deeper place.

**Borders.** The areas are the circles of moderate and hard flocks. Everyone walks around a hard
circle; a moderate one only while it sees a member of that flock (a leaky border: a sparse or too
wide circle is not respected everywhere). A member inside its own circle and going somewhere
inside it is not pushed out by a neighbour's overlapping circle. Below 25% of its store any
creature ignores borders: hunger outweighs the risk. With combat on the warning and strike rules
of the previous stages apply to intruders.

**Battles for room** (combat on only). A cornered flock, squeezed with no room nearby, fights
instead of moving: a hard one at once, a moderate one only after one such move did not help; a
flock without territoriality never starts one. The battle gathers every flock whose circle
touches the cornered one (unless under grace with it); battles that share a flock merge. Adult
fighters of territorial flocks (fuller than 25%, not wounded) strike the nearest adult of another
flock of the battle they see; a flock without territoriality only strikes back; kin and freshly
split groups never strike each other. A flock that has lost half of the adults it brought leaves
the battle and moves away; the others keep the place. A battle lasts at most 300 ticks, and a
flock that left one neither starts nor joins another for 600 ticks (`battle.rs`).

**Care and growth** (unchanged from the start of this stage). A parent no longer feeds its child
every tick: the inherited `care` gene only controls protection. The base inherited share of
energy at birth is 40%. A child gets no more than its tank holds; the parent pays only the energy
passed and the fixed penalty. Growth costs 2.25 energy per unit of diameter with the tank still
`2.5 × diameter`; the food value of a corpse's grown part uses the new price. The countdown of a
lasting split holds for every separated component of three or more members; a change of the
largest component does not reset its 600 ticks.

**Observability.** Snapshots and JSON: flock circle radius (p50/p90), flocks by kind, the share of
members inside their circle, strict and soft overlaps, squeezed flocks, battles and the flocks in
them; social counters `strays`, `relocations`, `battles`, `battle_retreats`. The chronicle adds
flock moves, battles and retreats, aggregated per interval like alarms. On screen: one circle per
flock, its stroke thin, normal or thick by territoriality, orange with a warned intruder, red in a
battle; circles glide between frames like the bodies; labels «№ · members · kind» go biggest flock
first and never over another, and a circle under 14 points gets none. The flock card shows kind,
layer, circle radius and squeeze, spacing, spread, members inside and the battle.

### Acceptance

The user left the criterion to us; combat is on by default. Proposed and used: survival ≥ 7/8 in
all four modes; in each profile with combat, flocks persist (≥ 2 flocks and flocking carriers ≥
10% at the end) in ≥ 75% of the worlds; no strict overlap in any snapshot; circle radius p90 ≤
600; members inside their circle, median ≥ 80%. Loners may vanish: a flock takeover is a
legitimate outcome of combat.

Seeds 1–8, 20 000 ticks, `--max-work 1e15`, every run finished by itself:

| Mode | Alive | Flocks persist | Both lines ≥ 10% | Median | vs old reference | Inside, median (min) |
|---|---:|---:|---:|---:|---:|---:|
| base, no combat | 8/8 | 7/8 | 5/8 | 1346 | −8% (1464) | 0.93 (0.82) |
| base, combat | 8/8 | 5/8 | 5/8 | 755 | −11% (848.5) | 0.82 (0.44) |
| calm, no combat | 8/8 | 5/8 | 4/8 | 981.5 | −5% (1032) | 0.99 (0.81) |
| calm, combat | 8/8 | 6/8 | 4/8 | 780.5 | +27% (614) | 0.85 (0.69) |

Eight seeds cannot tell 5/8 from 7/8 apart, so the combat profiles were also run on seeds 1–16:
base — flocks persist in 12/16, both lines in 11/16, median 900.5; calm — flocks persist in
14/16, both lines in 6/16 (flocks took over 8 worlds), median 731.5. Strict overlaps: 0 in every
snapshot of every run. Circle radius p90: at most 600. Battles for room: 87 in 5 of 8 base worlds
with combat (12 retreats), 116 in 4 of 8 calm ones (21 retreats); none without combat, as
designed. The `flock_spacing` median drifts to 45–270, in one calm world to 664 (the effect stays
clamped at 500).

**`repro_cost`.** The plan asked for the smallest value in 10–20 that lowers the median by 15–25%
in all four modes. None does (seeds 1–8, 20 000 ticks, change vs the old references):

| repro_cost | base, no combat | base, combat | calm, no combat | calm, combat |
|---:|---:|---:|---:|---:|
| 10 | −20% | −7% | −13% | +32% |
| 12 | +2% | −7% | −21% | 0% |
| 14 | −21% | −15% | −31% | −5% |
| 16 | −26% | −20% | −34% | −10% |
| 18 | −29% | +2% | −37% | −15% |
| 20 | −31% | −16% | −47% | −29% |

(all rows come from the model just before two last small fixes: flock kinds dealt in turn, and
circles on one centre parted along x). The medians of eight chaotic worlds
are not even monotone in the price, and a higher price made flocks persist less often in the calm
profile with combat. `repro_cost` stays 10.

Balance tried and rejected (16–32 seeds × 12 000 ticks unless noted): a narrower territory rule
where loners walk around only hard circles (flocks died out in 6–7 of 8 worlds), always respected
moderate borders (flocks took over 5–8 of 8), foraging members searching like loners (flocks
survive, but only 25–50% of members stay inside their circle), a leash of circle + vision for
foragers, a faster circle (0.6 of the members' speed), other hunger thresholds, founders dealt
20/60/20 by territoriality, splitting a flock that outgrew the largest circle (flocks persisted in
72% of base worlds with combat instead of 91%).

## Стайность, забота и режимы территорий (предыдущий этап)

У половины основателей есть стайность. Среди стайных 50% не охраняют границу,
40% защищают её после 30 тиков вторжения, 10% атакуют допустимого чужака сразу.
Способность стрелять есть у 5% основателей независимо от режима территории.
Стайность, территориальность, стратегия и способность стрелять наследуются как
единый режим семейной стаи: потомок с изменившимся режимом получает новую метку.
Одиночки и их дети живут с отдельными метками. После ухода, отделения или
рождения изменившегося потомка бывшие группы 600 тиков взаимно не охотятся и
не защищают территорию друг от друга. Близкое родство защищает бессрочно.

Наследуемая забота о потомстве начинается с 50%. Достаточно здоровый и сытый
родитель может занять одно из двух мест помощников, когда собственному
невзрослому ребёнку угрожают. При контакте тел и энергии ребёнка ниже 35% он отдаёт часть
своего запаса через обычное питание ребёнка, сохраняя себе не менее 60%.
Передача происходит до жизненных расходов; новорождённый не действует в тик
рождения. Разделение стай не разрывает связь родителя и ребёнка.

Графики и сводки ограничены последними 10 000 тиками, хроника сохраняет свою
историю. Панель отдельно показывает мировой выключатель боёв и наследуемую
плотоядность, допустимый размер добычи, долю стрелков и выстрелы за окно.
Рендер можно выключить без остановки движка; при далёком обзоре тела и трупы
становятся двухпиксельными квадратами. На близком масштабе труп сохраняет
диаметр тела при смерти, а расход пищи меняет только прозрачность. Кольцо
контактного ближнего боя показано лишь у выбранного существа.

### Проверка модели `life-behavior/5` до этих изменений

Профили без ручной настройки прошли seed 1–8 по 20 000 тиков с боями и без.
Все 32 прогона завершились по числу тиков, без остановки по лимиту; во всех
срезах сошлись рождения, причины смерти и численность, а числовые поля остались
конечными. Медианная численность в последнем срезе:

| Профиль | Без боёв | С боями |
|---|---:|---:|
| Базовый | 1464 | 848,5 |
| Спокойный (`cost_scale=3`) | 1032 | 614 |

По сравнению с прежней моделью население заметно выросло. Забота о детёнышах
снижает их гибель, но не объясняет весь прирост: отдельные прогоны без заботы
тоже стали многочисленнее. В нескольких мирах, особенно без боёв, стайные линии
к концу 20 000 тиков почти исчезают из-за преимущества одиночных линий.
Стабильность стай и плотность мира требуют отдельной балансировки; ради прохождения
проверок не менялись согласованные стоимость содержания, питания и боя.

Обновлены оба эталона `life-behavior/5`; они фиксируют именно эту версию модели.
Старые эталоны отклоняются до начала симуляции с указанием несовместимой версии.
Golden включает режимы стаи, родительскую заботу и сроки взаимной защиты.
На нагрузке 4000 существ/4000 растений тик занял 4,999 мс без боёв и 6,551 мс
с боями против примерно 3,9 мс у `d395fd2`; новая логика дороже, но оба
результата ниже предела 20 мс на измеренной машине.
Безэкранные последовательности при 960×600 и 1600×900 проверили дальние
квадраты, близкие тела, размер трупа, выключенный рендер и кольцо выбранного.
При той же нагрузке сборка кадра заняла 0,439 мс для тел, 0,118 мс для
квадратов и 0,001 мс при выключенном рендере; реальную частоту кадров окна
эти замеры не подменяют.

## Территории, порции пищи и дальний бой (предыдущий этап)

Территориальная стая из двух и более участников защищает круг вокруг своего центра. Его радиус
`clamp(1,4 × разброс + 40; 120; 320)` следует за стаей. Чужое существо
обходит замеченную границу или покидает область; после 30 тиков вторжения
здоровые взрослые могут защищать её. Нападение на своего даёт право на защиту
сразу. При выключенных боях территориальная агрессия прекращается. Кнопка
«Стаи» показывает разброс и отдельный контур территории (у разобщённой стаи
контур разброса может оказаться шире охраняемой области); карточка
показывает радиус и число предупреждённых чужаков.

Порог численности стаи — 50: при превышении взрослый с края группы может
получить новую метку и уйти. Это не блокирует рождения и не меняет родство.
Редкая наследуемая способность позволяет стрелять по видимому врагу на
расстоянии до четырёх собственных диаметров. Выстрел наносит 1% собственного
диаметра урона, стоит 2% диаметра энергии и требует пяти тиков восстановления;
при контакте приоритет у ближнего удара. Нападающие по-прежнему ограничены
размером добычи, защитники могут атаковать крупного противника. Удары одного
тика применяются одновременно.

Растение отдаёт пищу за пять контактных тиков и уменьшается на экране после
каждой порции. Часть сырой пищи теряется при обработке: усваивается 44% порции
растения и 10% порции трупа до поправки генов пищеварения. Эти коэффициенты
компенсируют возросшую доступность пищи при порционном питании; прежние
коэффициенты роста, содержания и урона не менялись. Каждая смерть оставляет труп
с конечной питательной ценностью, доступный со следующего тика; едоки берут
порции по возрастанию ID. Остаток разлагается за 600 тиков. При выключении
боёв трупы и мясные цели очищаются. JSON-отчёт `life-report/8` содержит трупы,
порции пищи, выстрелы, столкновения на границе и уходы взрослых. Текущий
эталон модели имеет `life-behavior/5`; старые эталоны отклоняются до сравнения.

«Лаборатория» позволяет менять стоимость рождения и выстрела, силу ближнего
удара и выстрела, паузу между выстрелами и усвоение растений. Это общие правила
мира, а не гены: изменения действуют на уже живущих существ, не меняя их
геном. У каждой строки есть сброс; несколько строк можно отметить и вернуть
к исходным значениям вместе. Начальные коэффициенты остались прежними,
поэтому мир без ручной настройки воспроизводит старый баланс. Боковая панель
с графиками стала уже и плотнее, ползунки лаборатории помещаются в
прокручиваемом окне на экранах 960×600 и 1600×900.

После добавления регуляторов пересняты эталоны штатным `--save-reference`.
При исходных значениях их ряды по восьми seed и 20 000 тиков побитно
совпали с прежними. Во всех четырёх сочетаниях базового/спокойного профиля
и выключенных/включённых боёв выжили 8 из 8 миров; все дошли до конца.
Новые поля присутствуют в JSON, а эталон старой версии отклоняется до прогона.

### Проверка предыдущей модели

Сравнение с `d395fd2`: seed 1–8, по 20 000 тиков, без преждевременной
остановки. Каждый тик проверены конечность координат, энергии и здоровья,
неотрицательное здоровье и точный баланс рождений и всех причин смерти.

| Профиль | Бои | Выжило | Медиана численности прежде → теперь | Доля резких разворотов прежде → теперь |
|---|---|---|---|---|
| Базовый | нет | 8/8 | 704,5 → 496,5 | 3,52% → 3,67% |
| Базовый | да | 8/8 | 513,5 → 623 | 4,53% → 8,76% |
| Спокойный | нет | 8/8 | 337,5 → 313,5 | 2,81% → 3,57% |
| Спокойный | да | 8/8 | 247 → 301,5 | 3,20% → 6,63% |

При боях рост медианы численности составляет 21–22%, ниже порога 25%.
Чужие движущиеся территории повышают число разворотов при боях; устойчивый
обход уменьшил их по сравнению с первоначальным прямым отступлением.
До изменения состава основателей стрелки появлялись редко: в спокойном прогоне seed 6
возникли 1416 выстрелов, а в остальных семи seed их не было. Тест с тремя
стрелками проверяет совместный залп по крупному нападающему, одновременную
смерть цели и доступность трупа со следующего тика.

Границу, обход, предупреждение, залп и кормёжку проверили последовательными
безэкранными снимками 960×600 и 1600×900 в `target/territory-shots/`.
Golden обновлён после этих прогонов; оба эталона 20 000 тиков повторно совпали,
эталон старой модели отклонён с кодом 2. Нагрузка 4000 существ и 4000 растений:
5,924 мс/тик без боёв и 8,515 мс/тик с боями против 3,659 мс/тик в `d395fd2`;
оба значения ниже 20 мс/тик. Это локальный замер, не гарантия для любого ПК.
`cargo test --workspace` (209 тестов), `cargo fmt --all -- --check`, строгий
Clippy и release-сборка окна прошли.

## Живая стайная жизнь (предыдущий этап)

Социальная память отделена от генома (`social.rs`). Ген `sociability`
добавлен после предыдущих генов: 0–100%, база 50%. Его цена — время сбора и помощи,
конкуренция с соседями и потерянные возможности личного поиска, без отдельного
списания энергии. Воспроизводимые фазы сбора разнесены по ID: доля времени
перед новым поиском пропорциональна общительности. При энергии ниже 25%
сбор не мешает питанию; выбранное растение и начатый бой не бросаются ради него.

Ближайшие восемь видимых своих дают локальный центр и расхождение при тесноте.
Итоговый вес притяжения к локальному центру: `12 × s × min(расстояние / зрение, 1)`;
вес общей цели — 1. При движении к личной пище притяжения нет, только расхождение.
При совпадении координат направление определяется парой ID. Курс перехода
удерживается 30 тиков; спокойный поворот ограничен 1,2 радиана за тик.
Личная еда, бой и бегство поворачивают без этого ограничения. Личное растение
остаётся целью, пока живо и видно. Скорость и расходы движения не менялись.

Сведения о растении живут 180 тиков, тревога — 60. Получатель видит сообщение
на следующем тике только от непосредственно видимого наблюдателя; чужие
сообщения не ретранслируются. Готовность принять чужую находку пропорциональна
общительности и удерживается 180 тиков. Общая кормовая цель пересматривается
раз в 60 тиков, удерживается минимум 180; без свежих сведений стая исследует
новое место. Пустое место забывается, повтор того же сообщения его не восстанавливает.

Отдых длится 60–120 тиков, допускается и одиночкам, стоит медленное содержание.
Начало при сытости выше 95%, выход ниже 85%, при уходе своих или опасности.
После отдыха выдерживается 180 тиков до нового. Вне домашнего слоя сначала
возвращаются домой. Бесплатного лечения, энергии или замедления возраста нет.

Тревога действует при общительности не ниже 25%; направление к безопасным
своим не разворачивает бегущего к угрозе. Затаившийся убегает и помогает на
полной скорости. Подтверждённый удар позволяет назначить максимум двух ближайших
взрослых помощников: общительность ≥50%, сытость >50%, здоровье выше порога
отступления на 0,1. Помощь ограничена 90 тиками, 30 тиками без новых ударов и
видимостью обоих участников; после окончания — перерыв 60 тиков.
Выключение боёв немедленно очищает боевую социальную память.

Компоненты взаимной видимости проверяются раз в 60 тиков. Отделённая группа
из трёх и более участников основывает стаю после 600 тиков разлуки. Смерть
якоря или соединение с основной группой сбрасывают ожидание. Метки выдаются
отдельным монотонным счётчиком; родство от отделения не меняется. Дети в тик
рождения не действуют. Бой, питание победителя и причины смерти остались прежними.

«Стаи» показывает центр, разброс, численность и занятие большинства. Клик
открывает карточку стаи; попадание в тело имеет преимущество. JSON `life-report/5`
содержит занятия, разброс, тревоги, вмешательства и отделения; модель эталона
`life-behavior/2`. Старая модель отклоняется до сравнения. Хроника агрегирует
начала/окончания тревог и отделения между срезами, без каждого отдельного сигнала.

В golden добавлены социальная память, сообщения, курсы, таймеры, сведения о
кормовой цели, ожидания отделений и счётчик меток. Новые мутации, отдых, сообщения
и движение намеренно меняют поведение уже с первых тиков.

Проверка движения и численности: `cargo run -p life-core --example social_probe --release -- 8`.
Каждый из 32 миров проходит ровно 20 000 тиков; на каждом тике проверяются
численность по смертям/рождениям и конечность жизненного состояния. Разворот —
отрицательное скалярное произведение двух последовательных ненулевых перемещений
одного существа без боя и бегства; после остановки или рождения пары нет.
Seed считаются независимо в диагностической программе, сам движок последовательный.

### Приёмка социальной модели

Сравнение с `3745cc3`, seed 1–8, по 20 000 тиков, без остановки по лимитам:

| Профиль | Бои | Выжило | Медиана прежде → теперь | Снижение доли резких разворотов |
|---|---|---|---|---|
| Базовый | нет | 8/8 | 810 → 704,5 | 81,84% |
| Базовый | да | 8/8 | 523,5 → 513,5 | 75,81% |
| Спокойный | нет | 8/8 | 294,5 → 337,5 | 80,48% |
| Спокойный | да | 8/8 | 216 → 247 | 76,85% |

В спокойном профиле финальные численности: 262–554 без боёв и 94–405 с боями.
Рост медиан 14,60% и 14,35% укладывается в 25%; жёсткого потолка населения нет.
Полные численности по seed и исходные показатели — `reference/social-validation.json`.
Тесты экранов снимают кормёжку, переход, тревогу, сбор и выбор карточки при
960×600 и 1600×900 (`target/social-shots/` при заданном `TINYLIFE_SHOTS`).

Заключительная проверка: 168 тестов прошли (служебная перезапись golden игнорируется),
форматирование, строгий Clippy и release-сборка окна успешны. Оба эталона повторно
совпали; эталон прежней модели отклонён с кодом 2. Фиксированная нагрузка 4000/4000
в последовательном замере: 1,918 мс/тик прежде, 3,659 мс/тик теперь, ниже лимита 20 мс.
Это локальное измерение времени, не гарантия для любого компьютера.

Прежние варианты с плавным поворотом при личном питании и слабой сплочённостью
давали перенаселение и не приняты. Итог меняет только социальные веса и пороги:
содержание, урон, пищевые коэффициенты, рост и возраст не перенастраивались.

## Спокойный игровой профиль и выбор области

В игре новый профиль по умолчанию: содержание `cost_scale=3`, стартовый темп
30 тиков/с. Кнопка «Спокойнее» применяет эти значения к текущей партии и сохраняет
стоимость для будущих миров. Старые настройки читаются как раньше; для них можно
применить кнопку. Это настройка существующих правил, формулы движка не менялись.

При 20 000 тиков на seed 1–8 выжили все миры: с боями финальная численность
72–311 (медиана 216), без боёв 153–542 (медиана 295). Это снижение плотности,
а не жёсткий потолок: результат зависит от эволюции и правил.
Эталон игрового профиля: `reference/calm-fingerprint.json`.

«Стаи» включает устойчивые цвета групп, одиночки серые. Раскраска работает
на паузе, на миникарте и карте плотности. «Убрать рамку» и Esc снимают область.
После выделения включается обычный выбор, рамка сохраняется при смене вкладок;
запоздавшая сводка снятой области игнорируется.

```text
cargo run -p life-report --release -- --compare reference/calm-fingerprint.json --rule cost_scale=3 --rule cannibalism=1 --max-work 1e15
```

Реализовано 20 сентября 2026. Исходная точка — `f209d97` и незакоммиченная работа
Claude над родством и бегством. Она сохранена и включена в новый жизненный цикл.
Параллельный тик, бенчмарк компьютера и `Relict/` не изменялись.

## Механики

- Родитель, ребёнок и дети одного родителя — близкие родственники. Родню и носителей
  одинаковой метки стаи нельзя атаковать. Угрозы читаются из снимка соседей.
  Совпадающие координаты дают воспроизводимое направление; выключение каннибализма
  сбрасывает бегство и цель атаки.
- Ген размера задаёт взрослый диаметр. Ребёнок рождается с половиной диаметра;
  основатели и добавленные вручную существа взрослые. Только усвоенная пища растит
  тело: доля `p/(1+p)`, текущая цена единицы диаметра `GROWTH_ENERGY_PER_SIZE = 2.25`.
  Остальное пополняет энергию. Рост ограничен взрослым размером и пространством
  у стен без смещения центра; неиспользованная доля переходит в энергию.
  Голод и рождение ребёнка не уменьшают тело.
- `life_pace`: база 1, диапазон 0.5–2. Содержание и биологический возраст
  умножаются на темп, скорость движения независима. Размножение доступно взрослым
  после индивидуального ожидания `ceil(DIVIDE_PERIOD/p)`; прежние расходы сохранены.
- Предел возраста 12 000. В последние 20% жизни максимум здоровья плавно падает
  от диаметра до половины диаметра. Рост сохраняет долю здоровья. После 60 тиков
  без боя при энергии выше половины восстанавливается 0.2% максимума здоровья
  за тик, по одной единице энергии за единицу здоровья.
- Бой начинается при контакте тел. Не более одного удара за тик: 5% своего
  диаметра, максимум 25% максимального здоровья цели. Энергетическая цена равна
  базовому урону. Все удары рассчитываются до нанесения повреждений и применяются
  одновременно; взаимная гибель допустима. Добычу получает живой участник с
  максимальным нанесённым уроном, при равенстве — с меньшим ID. Трупов нет.
- Bravery: base 50%, range 0–100%; retreat threshold `0.8 − 0.6b`.
  Carnivory: base 25%; plants are digested at `1 − 0.8c`, prey at `0.2 + 0.8c`.
  Prey holds its remaining energy and the cost of the body part it grew.
  `prey_ratio`: base 2.5, range 1–5, a free behaviour gene. It alone decides whom a creature
  attacks first (a body at most `size / prey_ratio`) and, in the eyes of others, whom it
  threatens. The world rule `cannibal_ratio` (2.5) is gone: it came from swallowing prey whole
  and only set a floor under the gene. What restrains a low ratio is behaviour: an equal fights
  back. Defence and territory strike regardless of size. A target is chosen by the energy it
  yields over the time of the chase and the fight. Above 90% of the tank no new hunt starts.
  `cannibalism` turns all combat off.
- The flock label is kept apart from the gene table: unique for founders, inherited with a 99%
  chance. Two living carriers make a flock, and a flock has a circle (see the first section).
  There is no cooperative hunting. Empty flocks are removed.

Порядок тика: растения → снимок и решения → движение и жизненные расходы →
питание растениями → одновременный бой → питание победителей → размножение →
удаление погибших и добавление детей. Дети не действуют в тик рождения.
Обе стратегии используют общие правила; затаившийся медленно блуждает.

## Форматы и отображение

Карточка показывает текущий и взрослый размер, возраст, здоровье, состояние и стаю.
Статистика показывает молодых, стаи и причины смерти. Рисование и выбор мышью
используют фактический размер. JSON is `life-report/9`, the balance reference has
`model: life-behavior/7`. Несовместимый эталон отклоняется с кодом 2 и объяснением.
Старые настройки получают новые значения по умолчанию.

Golden переснят намеренно: старое мгновенное поедание заменено боем, рост и
индивидуальные таймеры меняют размножение, добавлены выбор охоты и движение стаи.
Первое прежнее расхождение каннибализма на тике 100 вызвано уже изменёнными
решениями о родстве и бегстве. Новый отпечаток включает жизненное состояние,
таймеры, намерения, родство, метки, цели и генераторы стай.

## Проверка баланса

Все прогоны завершили ровно 20 000 тиков, без остановок по лимитам. Во всех
снимках проверено: начальная численность + рождения − все причины смерти =
живая численность. Стартовые согласованные коэффициенты менять не потребовалось.

| Seed | Без боёв, финал | С боями, финал |
|---|---:|---:|
| 1 | 858 | 376 |
| 2 | 762 | 461 |
| 3 | 906 | 665 |
| 4 | 683 | 797 |
| 5 | 323 | 366 |
| 6 | 1107 | 572 |
| 7 | 1066 | 475 |
| 8 | 581 | 684 |

Выживаемость — **8/8 в обоих режимах**, требование — минимум 7/8.
Промежуточные этапы также проверены на seed 1–3 с боями и без них.

```text
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 --rule cannibalism=0
cargo run -p life-report --release -- --seeds 1 2 3 4 5 6 7 8 --ticks 20000 --max-work 1e15 --rule cannibalism=1
cargo run -p life-report --release -- --save-reference reference/fingerprint.json --max-work 1e15
cargo run -p life-report --release -- --compare reference/fingerprint.json --max-work 1e15
```

Итог: 150 тестов прошли, один служебный тест печати golden пропущен;
форматирование и строгий Clippy проходят. Повторная сверка нового эталона проходит,
старый эталон отклоняется с кодом 2.

Тесты покрывают рост и энергию, края мира, возраст, ожидание размножения,
восстановление, многотиковые и взаимные бои, распределение добычи, выбор целей,
наследование и исчезновение стай, воспроизводимость и сохранение численности.
Headless-тесты интерфейса проверены на 960×600 и 1600×900; карточка с 13 генами
и новая статистика помещаются. Окно для проверки не запускалось.

На фиксированной нагрузке 4000 существ / 4000 растений три чередующихся замера:
исходная версия 1.350–1.475 мс/тик, новая 1.880–2.258 мс/тик (тестовый профиль
с оптимизацией, одна машина). Медианы: 1.386 и 2.027 мс/тик.
Оба ниже предела 20 мс. Поиск соседей использует сетки, полного перебора
всех существ для каждого участника нет. Замеры не являются бенчмарком компьютера.
