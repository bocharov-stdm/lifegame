//! The side panel's creature tab: the selected creature's card, or the card of one that died.

use eframe::egui::{self, RichText};
use life_core::genome::creature::N;
use life_core::genome::{GeneSpec, creature};
use life_core::{Rules, units};

use crate::app::LifeApp;
use crate::charts;
use crate::frame::{CREATURE_COLOR, Selected};
use crate::sim::Command;
use crate::theme::{self, GOOD, MUTED, rgb, spaced};

/// What the card's buttons asked for.
enum CardAction {
    None,
    Behaviour,
    Follow,
    Unselect,
}

/// The card of the selected creature: who it is and the buttons first, then how it is doing
/// (fullness, health, life lived) as bars, what it eats and how it behaves, and its genes folded.
/// `own`: its diet's median of each gene the panel follows (`DIET_GENES`), None for the rest.
fn creature_card(
    ui: &mut egui::Ui,
    s: &Selected,
    own: &[Option<f64>; N],
    rules: &Rules,
    world_h: f64,
    behaviour: bool,
    following: bool,
) -> CardAction {
    let mut act = CardAction::None;
    let diet_index = (s.genome[creature::Gene::Diet as usize] as usize).min(3);
    let diet = &creature::DIET_VARIANTS[diet_index];
    let color = rgb(theme::DIET_COLORS[diet_index]);
    ui.horizontal(|ui| {
        ui.label(RichText::new("●").color(color).size(18.0));
        ui.label(theme::heading(format!("Существо № {}", s.id), theme::HEADING));
    });
    ui.colored_label(MUTED, s.state);
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(behaviour, "Поведение (B)").on_hover_text("Блок-схема его программ").clicked()
        {
            act = CardAction::Behaviour;
        }
        if ui.selectable_label(following, "Следить (F)").on_hover_text("Камера идёт за ним").clicked()
        {
            act = CardAction::Follow;
        }
        if ui.button("Снять выбор").on_hover_text("Esc").clicked() {
            act = CardAction::Unselect;
        }
    });
    ui.add_space(4.0);

    let lifespan = s.genome[creature::Gene::Lifespan as usize].max(1.0);
    let fullness = (s.energy / s.max_energy).clamp(0.0, 1.0);
    let health = (s.health / s.max_health.max(1e-9)).clamp(0.0, 1.0);
    let lived = (s.age / lifespan).clamp(0.0, 1.0);
    let bar = |ui: &mut egui::Ui, frac: f64, fill: egui::Color32, text: String, hint: &str| {
        ui.add(egui::ProgressBar::new(frac as f32).fill(fill).desired_height(16.0).text(text))
            .on_hover_text(hint);
    };
    bar(
        ui,
        fullness,
        charts::energy_color(fullness, color),
        format!("сытость {:.0}% · расход {:.2} в тик", fullness * 100.0, s.upkeep),
        "Сколько энергии в баке от полного. Пустой бак — смерть от голода.",
    );
    bar(
        ui,
        health,
        charts::energy_color(health, GOOD),
        format!("здоровье {:.0} из {:.0}", s.health.max(0.0), s.max_health),
        "Удары отнимают здоровье; лечится по своей программе.",
    );
    bar(
        ui,
        lived,
        rgb(CREATURE_COLOR).gamma_multiply(0.7),
        format!("прожито {:.0}% · возраст {:.0} тиков", lived * 100.0, s.age),
        "Доля срока жизни. С 70% тело слабеет, на 100% — смерть от старости.",
    );
    ui.add_space(4.0);

    let program = &s.programs[s.stage];
    // how much smaller its prey, by the program it lives by now; only who digests fresh meat hunts
    let prey = match program.hunt_ratio() {
        Some(ratio) if rules.diets[diet_index].digestion[1] > 0.0 => {
            format!(" · добыча мельче в {} раза", format!("{ratio:.1}").replace('.', ","))
        }
        _ => String::new(),
    };
    ui.colored_label(color, format!("Питание: {}{prey}", diet.label)).on_hover_text(diet.about);
    ui.colored_label(MUTED, diet_bonuses(s.genome[creature::Gene::Diet as usize], rules));
    if let Some(food) = s.eating {
        use life_core::corpse::Stage;
        use life_core::creature::Morsel;
        ui.label(match food {
            Morsel::Plant => "ест растение",
            Morsel::Corpse { stage: Stage::Fresh } => "ест свежее мясо",
            Morsel::Corpse { stage: Stage::Rot } => "ест гниль",
            Morsel::Corpse { stage: Stage::Bones } => "грызёт кости",
        });
    }
    let template = life_core::creature::strategy::VARIANTS
        .get(s.genome[creature::Gene::Strategy as usize] as usize)
        .map_or("?", |v| v.label);
    let track = if s.stage == life_core::creature::JUVENILE { "детская" } else { "взрослая" };
    ui.colored_label(
        MUTED,
        format!(
            "Шаблон {template} · дорожка {track} · мутаций {} · блоков {}",
            program.changes,
            program.blocks().len()
        ),
    )
    .on_hover_text(
        "Две программы поведения: детская — пока растёт, взрослая — когда вырос. Шаблон — с какой \
         программы начинал род. Блок-схемы — по кнопке «Поведение (B)».",
    );
    let ctx = ui.ctx().clone();
    let say = |text: String| crate::words::say(&ctx, &text);
    ui.colored_label(
        MUTED,
        say(format!(
            "тело {:.1} из {:.1} · глубина {:.0}%",
            s.half * 2.0,
            s.genome[creature::Gene::Size as usize],
            s.y / world_h.max(1.0) * 100.0
        )),
    )
    // the same in real units (`life_core::units`): the life clock is compressed
    .on_hover_text(say(format!(
        "Растёт до размера своего гена. В настоящих единицах: {:.0} см, {:.1} года, глубина {:.0} м \
         (базовое существо — рыба в 20 см; тик жизни ≈ 9 часов).",
        units::cm(s.half * 2.0),
        units::years(s.age),
        units::metres(s.y)
    )));
    ui.add_space(4.0);
    egui::CollapsingHeader::new("Гены").default_open(false).show(ui, |ui| {
        ui.colored_label(MUTED, format!("справа — к медиане: {}", theme::DIET_NAMES[diet_index]));
        egui::Grid::new("карточка").num_columns(3).spacing([14.0, 4.0]).show(ui, |ui| {
            gene_rows(ui, &creature::GENES, &s.genome, own);
        });
    });
    act
}

/// A creature's genes by the table, against `own` (its diet's median, where known): who is this
/// one — bigger, more far-sighted than its kind? A choice gene with one variant is not shown.
fn gene_rows(ui: &mut egui::Ui, genes: &[GeneSpec], g: &[f64], own: &[Option<f64>; N]) {
    for (i, spec) in genes.iter().enumerate().filter(|(_, spec)| charts::shown(spec)) {
        ui.colored_label(MUTED, spec.label);
        if let Some(variants) = spec.variants() {
            ui.label(variants.get(g[i] as usize).map_or("?", |v| v.label));
            ui.label("");
            ui.end_row();
            continue;
        }
        let percent = spec.is_percent();
        // small values (speed) with tenths, big ones in whole numbers
        ui.label(match (percent, g[i] < 20.0) {
            (true, _) => format!("{:.0}%", g[i]),
            (false, true) => format!("{:.1}", g[i]),
            (false, false) => format!("{:.0}", g[i]),
        });
        let delta = own[i].map(|m| {
            if percent {
                format!("{:+.0} пунктов", g[i] - m)
            } else if m > 0.0 {
                format!("{:+.0}%", (g[i] / m - 1.0) * 100.0)
            } else {
                String::new()
            }
        });
        ui.colored_label(MUTED, delta.unwrap_or_default());
        ui.end_row();
    }
}

/// A diet's edges in the world's current rules (the lab may have changed them): «удар ×1,5 ·
/// скорость дешевле на 20%».
fn diet_bonuses(gene: f64, rules: &Rules) -> String {
    let e = &rules.diets[(gene.max(0.0) as usize).min(3)];
    let times = |x: f64| format!("{}", (x * 100.0).round() / 100.0).replace('.', ",");
    let cheaper = |name: &str, x: f64| {
        if x < 1.0 {
            format!("{name} дешевле на {:.0}%", (1.0 - x) * 100.0)
        } else {
            format!("{name} дороже на {:.0}%", (x - 1.0) * 100.0)
        }
    };
    let mut parts = Vec::new();
    if e.strike != 1.0 {
        parts.push(format!("удар ×{}", times(e.strike)));
    }
    if e.health != 1.0 {
        parts.push(format!("здоровье ×{}", times(e.health)));
    }
    if e.size_upkeep != 1.0 {
        parts.push(cheaper("размер", e.size_upkeep));
    }
    if e.speed_upkeep != 1.0 {
        parts.push(cheaper("скорость", e.speed_upkeep));
    }
    if e.smell != 1.0 {
        parts.push(format!("нюх ×{}", times(e.smell)));
    }
    if e.young_plants > e.digestion[0] {
        parts.push(format!("растения в детстве {:.0}%", e.young_plants * 100.0));
    }
    // bones feed only the scavenger by default: its own niche
    if e.digestion[3] > 0.0 {
        parts.push(format!("кости {:.0}%", e.digestion[3] * 100.0));
    }
    if parts.is_empty() { "без особых сил".into() } else { parts.join(" · ") }
}

impl LifeApp {
    pub(super) fn creature_tab(&mut self, ui: &mut egui::Ui) {
        let Some((s, rules, world_h)) =
            self.view.frame.as_ref().and_then(|f| Some((f.selected?, f.rules.clone(), f.world_h)))
        else {
            if let Some(lost) = self.lost {
                self.lost_card(ui, lost);
            } else {
                ui.colored_label(MUTED, "Никто не выбран. Кликните по существу в мире.");
            }
            return;
        };
        // its own diet's medians: a carnivore is compared with carnivores, not with the herbivores
        let diet = (s.genome[creature::Gene::Diet as usize] as usize).min(3);
        let mut own = [None; N];
        if let Some(genes) = self.history.snapshots.last().and_then(|snap| snap.diets[diet].genes) {
            for (spread, gene) in genes.iter().zip(life_sim::observe::DIET_GENES) {
                own[gene as usize] = Some(spread.p50);
            }
        }
        let following = self.view.following();
        let mut act = CardAction::None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            act = creature_card(ui, &s, &own, &rules, world_h, self.behaviour_open, following);
        });
        match act {
            CardAction::None => {}
            CardAction::Behaviour => self.behaviour_open = !self.behaviour_open,
            CardAction::Follow => self.view.toggle_follow(),
            CardAction::Unselect => self.unselect(),
        }
    }

    /// The card of the selected creature that died: how and when, and who to look at next.
    fn lost_card(&mut self, ui: &mut egui::Ui, lost: crate::frame::Lost) {
        let diet = lost.diet.min(3);
        ui.horizontal(|ui| {
            ui.label(RichText::new("●").color(rgb(theme::DIET_COLORS[diet])).size(18.0));
            ui.label(theme::heading("Погибло", theme::HEADING));
            ui.colored_label(MUTED, format!("№ {}", lost.id));
        });
        ui.label(format!(
            "{} · {} на тике {} в возрасте {:.0}",
            creature::DIET_VARIANTS[diet].label,
            crate::sim::death_words(lost.cause),
            spaced(lost.tick),
            lost.age
        ));
        ui.add_space(6.0);
        let world_gen = self.view.frame.as_ref().map_or(0, |f| f.world_gen);
        ui.horizontal_wrapped(|ui| {
            let nearest = |diet| Command::SelectNearest { x: lost.x, y: lost.y, diet, world_gen };
            if ui
                .button(format!("Ближайший: {}", theme::DIET_NAMES[diet]))
                .on_hover_text("Выбрать живое существо того же питания, ближайшее к месту гибели")
                .clicked()
            {
                self.sim.send(nearest(Some(diet)));
            }
            if ui.button("Ближайший любой").clicked() {
                self.sim.send(nearest(None));
            }
            if ui.button("Закрыть").on_hover_text("Esc").clicked() {
                self.lost = None;
                self.refollow = false;
            }
        });
    }
}
