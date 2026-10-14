use super::action::PACE;
use super::cond::MODE;
use super::mutate::MUTATIONS;
use super::*;
use crate::rng::Rng;

#[test]
fn tables_list_every_variant_in_order() {
    for (i, c) in Cond::ALL.iter().enumerate() {
        assert_eq!(*c as usize, i);
    }
    for (i, a) in Action::ALL.iter().enumerate() {
        assert_eq!(*a as usize, i);
        let params = a.params();
        assert!(params.len() <= MAX_ARGS, "{a:?}");
        for p in params {
            assert!(p.lo <= p.base && p.base <= p.hi, "{a:?}: {p:?}");
            assert!(p.hi < 1 << 12, "{a:?}: {p:?} fits the block's code");
            assert!(!p.drifts() || p.nudge >= 1.0, "{a:?}: {p:?} moves");
            assert!(p.drifts() || p.hi < 8, "{a:?}: {p:?} fits the block's shape");
        }
    }
    for c in Cond::ALL {
        if let Some(p) = c.param() {
            assert!(p.lo <= p.base && p.base <= p.hi, "{c:?}");
            assert!(!p.drifts() || p.nudge >= 1.0, "{c:?} moves");
            assert!(p.hi < 1 << 12 && (p.drifts() || p.hi < 8), "{c:?} fits the code and the shape");
        }
    }
    assert!(Cond::ALL.len() <= 32 && Action::ALL.len() <= 32, "5 bits each in the code");
    const { assert!(MAX_ARGS <= 10 && MAX_BLOCKS <= 32) };
    let total: f64 = MUTATIONS.iter().map(|m| m.1).sum();
    assert!((total - 1.0).abs() < 1e-12, "the shares sum to 1: {total}");
}

/// The templates carry the old behaviour genes' bases; the lurker differs only in its paces.
#[test]
fn the_templates_carry_the_old_bases() {
    let (s, l) = (Program::STANDARD, Program::LURKER);
    assert_eq!(s.reachable(), s.blocks().len(), "every block of a template is reached");
    assert_eq!(Program::template(Strategy::Standard), s);
    assert_eq!(Program::template(Strategy::Lurker), l);
    let differ: Vec<usize> = (0..s.blocks().len()).filter(|&i| s.blocks()[i] != l.blocks()[i]).collect();
    assert_eq!(differ.len(), 1, "the lurker differs only in its wander's pace: {differ:?}");
    assert_eq!(s.hunt_ratio(), Some(1.5));
    assert_eq!(s.defence(), Some((1.5, 0.5)));
    assert_eq!(s.wander_reach(), 2.0);
    assert!((s.threat_range() - 0.33).abs() < 1e-12);
    assert_eq!((s.home_layer(), s.shoots()), ((0.05, 1.0), false));
    let text = s.describe();
    assert_eq!(text.len(), 22);
    // what the genes and the world did: the settings on top; mating came with `life-behavior/16`
    assert_eq!(
        text[..7],
        [
            "1. если всегда → установка: слой (сверху 5%, снизу 100%)",
            "2. если всегда → установка: плавный ход (держит курс 30 тиков, поворот до 69°)",
            "3. если всегда → установка: спариваться (с бака 50%, потомку 40%)",
            "4. если всегда → установка: делиться (с бака 70%, потомку 40%)",
            "5. если всегда → установка: лечиться (при баке больше 50%, без ударов 60 тиков)",
            "6. если всегда → установка: есть на ходу (растения, падаль, пока сытость не больше 100%)",
            "7. если всегда → установка: щадить детей (пока ребёнок не вырос до 100%)",
        ]
    );
    // the founders' memory: hunger, an alarm, a full tank
    assert_eq!(text[7], "8. если сытость < 30% → установка: включить режим (режим 3, на 60 тиков)");
    assert_eq!(text[8], "9. если включён режим 3 → установка: есть и чужую пищу");
    assert_eq!(
        text[10],
        "11. если охотник ближе 33% зрения → установка: включить режим (режим 1, на 61 тик)"
    );
    assert_eq!(text[11], "12. если угроза ближе 16% зрения → установка: включить режим (режим 1, на 61 тик)");
    assert_eq!(text[12], "13. если сытость ≥ 95% → установка: включить режим (режим 2, на 200 тиков)");
    assert_eq!(text[13], "14. если здоровье ≥ 50% → дать сдачи (враг крупнее не более чем в 1,5 раза)");
    assert_eq!(
        text[14],
        "15. если включён режим 1 → убегать (бежать ещё 60 тиков, рывком, снова пугается угрозы ближе \
         33% зрения, темп 100%, прямо)"
    );
    assert_eq!(
        text[15],
        "16. если здоровье ≥ 60% → защищать детёныша (ребёнок ближе 50% зрения, при баке больше 50%, \
         не дольше 90 тиков, потом пауза 60 тиков, ребёнка ударили за 30 тиков)"
    );
    assert_eq!(
        text[16],
        "17. если сытость ≥ 70% и пара ближе 100% зрения → идти к паре (темп 100%, пара не дальше 100% \
         зрения)"
    );
    assert_eq!(
        text[17],
        "18. если всегда → охотиться (добыча мельче в 1,5 раза, осторожность 100%, терпение 30 тиков, \
         только если выгоднее, рывком, брошенную не трогать 180 тиков, добыча не дальше 100% зрения, \
         темп погони 100%)"
    );
    assert_eq!(
        text[18],
        "19. если включён режим 2 и сытость ≥ 85% → отдыхать (отдых 90 тиков, пауза 180 тиков)"
    );
    assert_eq!(text[20], "21. если всегда → к растению (темп 100%, держится выбранного, не самое выгодное)");
    assert_eq!(
        *text.last().unwrap(),
        format!("{}. если всегда → бродить (темп 100%, цели до 200% зрения)", text.len())
    );
    assert!(l.describe().last().unwrap().contains("(темп 33%"));
}

#[test]
fn blocks_after_one_that_always_fires_are_unreachable() {
    let p = Program::of(&[
        Block::when(Test::at(Cond::Fullness, 40).not(), Action::EatPlant),
        Block::does(Action::Ambush),
        Block::does(Action::Wander),
        Block::does(Action::EatForeign),
    ]);
    assert_eq!(p.reachable(), 2);
    assert_eq!(
        p.describe()[0],
        "1. если сытость < 40% → к растению (темп 100%, держится выбранного, не самое выгодное)"
    );
    assert!(!p.live(2), "a deciding block after one that always fires is never reached");
    assert!(p.live(3), "a setting applies wherever it stands");
    // a negated «always» is «never»: the block is off and does not end the program
    let off = Program::of(&[Block::when(Test::ALWAYS.not(), Action::Wander), Block::does(Action::Hunt)]);
    assert_eq!(off.reachable(), 2);
    assert_eq!(off.blocks()[0].condition_label(), "никогда");
    assert!(!off.live(0) && off.live(1));
    assert_eq!(off.hunt_ratio(), Some(1.5));
    // a setting never ends the program
    let setting = Program::of(&[Block::does(Action::EatForeign), Block::does(Action::Wander)]);
    assert_eq!(setting.reachable(), 2);
    // no hunt, no defence, no wander: nothing to fear, nothing to fight, the base wander
    let idle = Program::of(&[Block::does(Action::Ambush)]);
    assert_eq!((idle.hunt_ratio(), idle.defence(), idle.wander_reach()), (None, None, 2.0));
}

fn check(p: &Program) {
    assert!((1..=MAX_BLOCKS).contains(&p.blocks().len()), "{} blocks", p.blocks().len());
    for b in p.blocks() {
        for t in b.when {
            match t.cond.param() {
                Some(s) => assert!((s.lo..=s.hi).contains(&t.param), "{t:?}"),
                None => assert_eq!(t.param, 0, "{t:?}"),
            }
        }
        let params = b.action.params();
        for (i, a) in b.args.iter().enumerate() {
            match params.get(i) {
                Some(s) => assert!((s.lo..=s.hi).contains(a), "{b:?}"),
                None => assert_eq!(*a, 0, "{b:?}"),
            }
        }
    }
    assert!(p.blocks[usize::from(p.len)..].iter().all(|b| *b == FILLER), "the tail stays the filler");
    assert_eq!(p.summary, Summary::of(&p.blocks, usize::from(p.len)), "the summary is up to date");
}

/// Mutations keep a program valid (1 to `MAX_BLOCKS` blocks, every number in its range, the
/// summary current), count only what changed it, and a long line of mutants grows and shrinks.
#[test]
fn mutations_keep_a_program_valid_and_change_it() {
    let mut rng = Rng::new(5);
    let mut p = Program::STANDARD;
    let start = p.blocks().len();
    let (mut longest, mut shortest, mut changed) = (0, MAX_BLOCKS, 0);
    for _ in 0..20_000 {
        let before = p;
        p.mutate(1.0, &mut rng);
        check(&p);
        let differs = !p.same_blocks(&before);
        assert_eq!(p.changes, before.changes.saturating_add(differs as u16), "only a change counts");
        changed += differs as usize;
        longest = longest.max(p.blocks().len());
        shortest = shortest.min(p.blocks().len());
    }
    assert!(longest > start && shortest < start, "it grows and shrinks: {shortest}‒{longest}");
    // a nudge by less than half a unit, a swap of equal blocks, the same random pick or a
    // copy into a full program change nothing; most mutations change something
    assert!(changed > 12_000, "changed {changed} of 20 000");
}

/// With chance `c` about that share of children mutate; a copy draws exactly one number.
#[test]
fn a_program_mutates_with_its_chance() {
    let mut rng = Rng::new(9);
    let n = 100_000;
    let mutated = (0..n)
        .filter(|_| {
            let mut p = Program::LURKER;
            p.mutate(0.05, &mut rng);
            p.changes == 1
        })
        .count();
    // a few mutations land where they were and are not counted
    assert!((4300..=5300).contains(&mutated), "5% expected: {mutated}");
    let mut a = Rng::new(3);
    let mut b = a.clone();
    let mut p = Program::STANDARD;
    p.mutate(0.0, &mut a);
    b.random();
    assert_eq!((p, a), (Program::STANDARD, b), "no mutation: one draw, the same program");
}

#[test]
fn each_kind_of_mutation_does_its_change() {
    let mut rng = Rng::new(17);
    let (mut longer, mut shorter, mut negated, mut switched_off, mut reordered, mut nudged, mut flipped) =
        (0, 0, 0, 0, 0, 0, 0);
    let two = Program::of(&[
        Block::when(Test::at(Cond::Fullness, 50), Action::Flee),
        Block::does(Action::Wander).with(0, 60),
    ]);
    for _ in 0..8000 {
        let mut p = two;
        p.mutate(1.0, &mut rng);
        let (a, b) = (two.blocks(), p.blocks());
        longer += (b.len() > a.len()) as usize;
        shorter += (b.len() < a.len()) as usize;
        if b.len() != a.len() {
            continue;
        }
        // a negation turns a real test; only a toggle touches an «always»
        switched_off += b.iter().any(Block::off) as usize;
        negated += (b[0].when[0].cond == Cond::Fullness && b[0].when[0].negate) as usize;
        reordered += (b[0] == a[1] && b[1] == a[0]) as usize;
        let same_shape = b[0].action == a[0].action && b[1].action == a[1].action;
        nudged += (same_shape
            && b[0].when[0].cond == Cond::Fullness
            && (b[0].when[0].param != 50 || b[0].args != a[0].args || b[1].args != a[1].args))
            as usize;
        flipped += (same_shape && b[0].args[1] != a[0].args[1]) as usize;
    }
    assert!(longer > 600 && shorter > 600, "insertions, copies and pairs {longer}, deletions {shorter}");
    assert!(negated > 220 && switched_off > 150, "negated {negated}, switched off {switched_off}");
    assert!(reordered > 700, "{reordered}");
    assert!(nudged > 1600 && flipped > 180, "nudged {nudged}, a flag flipped {flipped}");
}

/// A mutation moves an index to another of its values, never where it was; the drift leaves it.
#[test]
fn an_index_is_picked_anew_and_does_not_drift() {
    let mut rng = Rng::new(8);
    let mut seen = [0; MODES + 1];
    for _ in 0..4000 {
        let k = MODE.nudged(2, &mut rng);
        assert_ne!(k, 2);
        seen[usize::from(k)] += 1;
    }
    assert!(seen[1] > 1000 && seen[3] > 1000 && seen[4] > 1000, "{seen:?}");
    let mut p = Program::of(&[
        Block::when(Test::at(Cond::Mode, 3), Action::Ambush),
        Block::does(Action::Mode).with(0, 4),
        Block::does(Action::Wander),
    ]);
    for _ in 0..100 {
        p.drift(3.0, &mut rng);
    }
    assert_eq!((p.blocks()[0].when[0].param, p.blocks()[1].args[0]), (3, 4), "the drift leaves indices");
}

/// The drift moves a third of the numbers, never the shape or a flag, keeps them in range, is
/// not counted as a mutation, and at 0 draws nothing.
#[test]
fn the_drift_moves_numbers_not_the_shape() {
    let mut rng = Rng::new(21);
    let mut p = Program::STANDARD;
    let (mut moved, mut numbers, mut total) = (0, 0, 0);
    for _ in 0..200 {
        let before = p;
        p.drift(1.0, &mut rng);
        check(&p);
        assert_eq!(p.shape(), Program::STANDARD.shape());
        assert_eq!(p.changes, 0);
        moved += !p.same_blocks(&before) as usize;
        for i in 0..p.blocks().len() {
            for (slot, _) in p.blocks[i].numbers() {
                total += 1;
                numbers += (p.blocks[i].get(slot) != before.blocks[i].get(slot)) as usize;
            }
        }
    }
    assert!(moved > 190, "almost every drift moves some number: {moved}");
    let share = numbers as f64 / total as f64;
    assert!((0.2..0.4).contains(&share), "a third of the numbers a drift, less the rounding: {share}");
    let hunt = p.blocks().iter().find(|b| b.action == Action::Hunt).unwrap();
    assert!(hunt.flag(3) && hunt.flag(4), "flags stay");
    assert_ne!(hunt.args[0], 150, "the ratio walked away in 200 generations");
    let (mut a, b) = (Rng::new(4), Rng::new(4));
    let mut still = Program::STANDARD;
    still.drift(0.0, &mut a);
    assert_eq!((still, a), (Program::STANDARD, b), "no drift, no draws");
}

/// The median of a drifting group keeps its shape and takes each number's middle.
#[test]
fn the_median_of_a_group_takes_each_numbers_middle() {
    let ratios = [120, 150, 190];
    let group: Vec<Program> =
        ratios.iter().map(|&r| Program::STANDARD.tuned(Action::Hunt, |b| b.args[0] = r)).collect();
    let m = Program::median(&group).unwrap();
    assert_eq!(m.shape(), Program::STANDARD.shape());
    assert_eq!(m.hunt_ratio(), Some(1.5));
    assert_eq!(Program::median(&[group[0], Program::of(&[Block::does(Action::Hunt)])]), None);
    assert_eq!(Program::median(&[]), None);
}

/// A pair that kills one block while its reader revives another leaves as many dead blocks as
/// before, yet it killed one: it is no junk-free addition. The count alone let it through.
#[test]
fn an_addition_that_kills_one_block_and_revives_another_still_kills() {
    let conditional_mode = Block::when(Test::at(Cond::Fullness, 50), Action::Mode).with(0, 1);
    let before = Program::of(&[
        conditional_mode,
        Block::does(Action::Divide),
        Block::does(Action::Divide).with(0, 90),
        Block::does(Action::Wander),
    ]);
    assert!(before.live(0) && before.live(1) && !before.live(2), "the second division hidden");
    // the pair: an unconditional mode 1 on top, the first division its reader
    let after = Program::of(&[
        Block::does(Action::Mode).with(0, 1),
        conditional_mode,
        Block::when(Test::at(Cond::Mode, 1), Action::Divide),
        Block::does(Action::Divide).with(0, 90),
        Block::does(Action::Wander),
    ]);
    assert_eq!(after.dead(), before.dead(), "as many dead as before");
    assert!(!after.live(1) && after.live(3), "one killed, one revived");
    assert!(after.kills(&before, 0));
    // an addition that kills nothing passes
    let harmless = Program::of(&[
        conditional_mode,
        Block::does(Action::Divide),
        Block::does(Action::Divide).with(0, 90),
        Block::when(Test::at(Cond::Fullness, 20), Action::Rest),
        Block::does(Action::Wander),
    ]);
    assert!(!harmless.kills(&before, 3));
}

/// A transfer copies any live block of the other track, an unconditional setting or an
/// always-firing block too: a track that lost them can get them back from the other one.
#[test]
fn a_transfer_brings_a_setting_and_an_ending_the_track_lacks() {
    let mut rng = Rng::new(5);
    let juvenile = Program::of(&[Block::when(Test::at(Cond::Fullness, 40).not(), Action::EatPlant)]);
    // numbers no other mutation would give, so only a transfer brings these blocks
    let adult =
        Program::of(&[Block::does(Action::Divide).with(0, 77), Block::does(Action::Wander).with(0, 37)]);
    let (mut setting, mut ending) = (false, false);
    for _ in 0..20_000 {
        let mut p = juvenile;
        p.mutate_with(1.0, Some(&adult), &mut rng);
        let b = p.blocks();
        setting |= b.contains(&adult.blocks()[0]);
        ending |= b.len() == 2 && b[1] == adult.blocks()[1];
    }
    assert!(setting && ending, "transferred: the setting {setting}, the wander {ending}");
}

/// The block that always fires ends a program whether it is the last block or settings follow
/// it (the window then draws «стоит» as never reached); a program without one may stand.
#[test]
fn the_ending_is_the_always_firing_block_wherever_it_stands() {
    let eat = Block::when(Test::at(Cond::Fullness, 40).not(), Action::EatPlant);
    let last = Program::of(&[eat, Block::does(Action::Wander)]);
    let then_setting = Program::of(&[eat, Block::does(Action::Wander), Block::does(Action::Divide)]);
    let may_stand = Program::of(&[eat, Block::does(Action::Divide)]);
    assert_eq!((last.ending(), then_setting.ending(), may_stand.ending()), (Some(1), Some(1), None));
    assert_eq!(
        Program::STANDARD.ending().map(|i| Program::STANDARD.blocks()[i].action),
        Some(Action::Wander)
    );
}

/// Structure: no block added (a copy, a new one, a pair, a transfer) is born dead or kills
/// one, a copy is of a live block, a pair brings a mode and a live reader of it together, a
/// transfer copies a live block of the other track, and a replaced action keeps the pace.
#[test]
fn structural_mutations_build_reachable_working_programs() {
    let mut rng = Rng::new(11);
    // an off block, a decider, an always-firing wander, a dead decider after it
    let base = Program::of(&[
        Block::when(Test::ALWAYS.not(), Action::Ambush),
        Block::when(Test::at(Cond::Fullness, 40).not(), Action::EatPlant).with(0, 55),
        Block::does(Action::Wander),
        Block::does(Action::Hunt).with(1, 200),
    ]);
    assert_eq!((base.dead(), base.ending()), (2, Some(2)));
    let other = Program::of(&[
        Block::when(Test::ALWAYS.not(), Action::Torpor),
        Block::does(Action::EatCorpse).with(1, 70),
    ]);
    let (mut pairs, mut transfers, mut kept_pace) = (0, 0, 0);
    for _ in 0..6000 {
        let mut p = base;
        p.mutate_with(1.0, Some(&other), &mut rng);
        check(&p);
        let (a, b) = (base.blocks(), p.blocks());
        if b.len() == a.len() + 1 {
            assert!(p.dead() <= base.dead(), "an added block is no junk: {b:?}");
            assert_eq!(p.ending().map(|i| b[i]), Some(a[2]), "the wander still ends it: {b:?}");
            let at = (0..b.len()).find(|&i| a.get(i) != Some(&b[i])).unwrap();
            let new = b[at];
            if let Some(j) = a.iter().position(|x| *x == new) {
                assert!(base.live(j), "a copy is of a live block: {j}");
            }
            assert_ne!(new, other.blocks()[0], "a transfer copies no dead block");
            transfers += (new == other.blocks()[1]) as usize;
            if let Some(setting) = b.iter().position(|x| x.action == Action::Mode) {
                let mode = b[setting].args[0];
                let tested = |x: &Block| x.when.iter().any(|t| t.cond == Cond::Mode && t.param == mode);
                if let Some(r) = (0..b.len()).find(|&i| i != setting && tested(&b[i])) {
                    assert!(p.live(r) && !b[r].always_fires(), "a pair's reader lives: {b:?}");
                    pairs += 1;
                }
            }
        } else if b.len() == a.len() && b[1].action != a[1].action && b[0] == a[0] && b[2..] == a[2..] {
            let pace = b[1].action.params().iter().position(|p| p.label == PACE.label);
            if let Some(k) = pace {
                assert_eq!(b[1].args[k], 55, "the pace goes over to {:?}", b[1].action);
                kept_pace += 1;
            }
        }
    }
    assert!(pairs > 100 && transfers > 60 && kept_pace > 5, "{pairs} {transfers} {kept_pace}");
    // an unconditional setting of a kind already set is dead, and a copy of it is not made
    let set =
        Program::of(&[Block::does(Action::Divide), Block::does(Action::Divide), Block::does(Action::Wander)]);
    assert!(set.live(0) && !set.live(1) && set.dead() == 1);
    let conditional = Program::of(&[
        Block::when(Test::at(Cond::Fullness, 50), Action::Divide),
        Block::does(Action::Divide),
        Block::does(Action::Wander),
    ]);
    assert!(conditional.live(0) && conditional.live(1), "a conditional one hides nothing");
    let mut rng = Rng::new(12);
    let single = Program::of(&[Block::does(Action::Divide), Block::does(Action::Wander)]);
    for _ in 0..500 {
        let mut p = single;
        p.mutate_with(1.0, Some(&single), &mut rng);
        if p.blocks().len() > 2 {
            assert_eq!(p.dead(), 0, "{:?}", p.blocks());
        }
    }
}

/// The spread of a group's numbers: none for copies, about a half for random values.
#[test]
fn the_spread_of_a_group_tells_copies_from_random_numbers() {
    assert_eq!(Program::spread(&[Program::STANDARD; 5]), Some(0.0));
    let mut rng = Rng::new(2);
    let random: Vec<Program> = (0..400)
        .map(|_| {
            let mut p = Program::of(&[Block::does(Action::Hunt), Block::does(Action::Wander)]);
            for b in &mut p.blocks[..2] {
                for (slot, spec) in b.numbers().collect::<Vec<_>>() {
                    *b.number(slot) = spec.random(&mut rng);
                }
            }
            p
        })
        .collect();
    let spread = Program::spread(&random).unwrap();
    assert!((0.4..0.6).contains(&spread), "{spread}");
    assert_eq!(Program::spread(&[Program::STANDARD, Program::of(&[Block::does(Action::Hunt)])]), None);
    assert_eq!(Program::spread(&[]), None);
}

#[test]
fn codes_tell_blocks_apart() {
    let mut codes: Vec<[u64; 3]> = Vec::new();
    for c in Cond::ALL {
        for a in Action::ALL {
            for negate in [false, true] {
                let t = Test { cond: c, negate, param: c.param().map_or(0, |p| p.hi) };
                let a_ = Test::ALWAYS;
                codes.push(Block::when3(t, a_, a_, a).code());
                codes.push(Block::when3(a_, t, a_, a).code());
                codes.push(Block::when3(a_, a_, t, a).code());
            }
        }
    }
    let n = codes.len();
    codes.sort_unstable();
    codes.dedup();
    // a test in any slot with «always» in the others is the same block only for «always»
    assert_eq!(codes.len(), n - 2 * Action::ALL.len());
    let b = Block::does(Action::Hunt);
    for i in 0..MAX_ARGS {
        assert_ne!(b.code(), b.with(i, b.args[i] + 1).code(), "parameter {i} counts");
    }
    assert_eq!(b.shape(), b.with(5, 0).with(0, 300).with(7, 50).shape(), "numbers are no part of the shape");
    assert_ne!(b.shape(), b.with(4, 0).shape(), "flags are");
    let mode = Block::does(Action::Mode);
    assert_ne!(mode.shape(), mode.with(0, 2).shape(), "a mode's number is");
    assert_eq!(mode.shape(), mode.with(1, 300).shape());
    let tested = |k| Block::when(Test::at(Cond::Mode, k), Action::Ambush).shape();
    assert_ne!(tested(1), tested(2), "so is a tested one's");
}

/// Of the settings of one kind the first that applies wins; each mode is a kind of its own.
#[test]
fn settings_of_a_kind_and_modes_have_their_kinds() {
    let rival = Block::does(Action::Rival);
    assert_eq!(rival.setting_kind(), rival.with(0, 300).setting_kind());
    assert_ne!(rival.setting_kind(), Block::does(Action::Reach).setting_kind());
    let mode = Block::does(Action::Mode);
    let kinds: Vec<u64> = (1..=MODES as u16).map(|k| mode.with(0, k).setting_kind()).collect();
    for (i, a) in kinds.iter().enumerate() {
        assert_eq!(a.count_ones(), 1);
        assert!(kinds[i + 1..].iter().all(|b| a & b == 0), "modes differ in kind");
        assert!(Action::ALL.iter().all(|&x| Block::does(x).setting_kind() & a == 0 || x == Action::Mode));
    }
    assert_eq!(mode.setting_kind(), mode.with(1, 5).setting_kind(), "the time is no part of it");
}

#[test]
fn parameters_read_and_show_in_their_units() {
    let hunt = Block::does(Action::Hunt);
    assert_eq!(hunt.arg(0), 1.5);
    assert_eq!(hunt.arg(1), 1.0);
    assert_eq!(hunt.arg(2), 30.0);
    assert_eq!(hunt.arg(5), 180.0);
    assert!(hunt.flag(3) && hunt.flag(4));
    assert_eq!(Action::Hunt.params()[4].show(0), "не рывком");
    assert_eq!(Test::at(Cond::ThreatNear, 17).label(), "угроза ближе 17% зрения");
    assert_eq!(Test::at(Cond::ThreatNear, 17).value(), 0.17);
    assert_eq!(Test::is(Cond::Struck).label(), "его ударили за 1 тик");
    assert_eq!(Test::at(Cond::Struck, 22).not().label(), "его не били за 22 тика");
    let words: Vec<&str> = [1, 2, 5, 11, 12, 21, 24, 111].into_iter().map(ticks_word).collect();
    assert_eq!(words, ["тик", "тика", "тиков", "тиков", "тиков", "тик", "тика", "тиков"]);
    // the new units: a tilt, an angle, an index
    let tilt = Action::Flee.params()[4];
    assert_eq!([tilt.show(130), tilt.show(70), tilt.show(100)], ["вниз 30%", "вверх 30%", "прямо"]);
    assert_eq!((tilt.value(130), tilt.value(100)), (0.3, 0.0));
    let angle =
        ParamSpec { label: "поворот", unit: Unit::Angle, lo: 0, hi: 314, base: 120, nudge: 10.0 };
    assert_eq!((angle.show(120), angle.value(120)), ("поворот 69°".to_string(), 1.2));
    assert_eq!(Test::at(Cond::Mode, 2).label(), "включён режим 2");
    assert_eq!(Test::at(Cond::Mode, 3).not().label(), "выключен режим 3");
    assert_eq!(Test::at(Cond::Age, 70).not().label(), "возраст < 70%");
    assert_eq!(Block::does(Action::Mode).args_label(), "режим 1, на 60 тиков");
    let three = Block::when3(
        Test::at(Cond::Fullness, 40).not(),
        Test::is(Cond::PlantSeen),
        Test::is(Cond::Winded).not(),
        Action::EatPlant,
    );
    assert_eq!(three.condition_label(), "сытость < 40% и видит растение и не запыхался");
}

/// Programs are shared, compared by content, read as an array.
#[test]
fn programs_are_shared_and_compared_by_content() {
    let a = Programs::both(Program::LURKER);
    let b = a.clone();
    assert!(Arc::ptr_eq(&a.0, &b.0), "a copy shares the programs");
    assert_eq!(a, Programs::from([Program::LURKER; 2]));
    assert_eq!(a, [Program::LURKER; 2]);
    assert_eq!(a[ADULT].blocks().len(), Program::LURKER.blocks().len());
    assert!(std::mem::size_of::<Programs>() <= 8);
}
