//! Behaviour programs: a creature's behaviour is an ordered list of blocks «if TESTS → ACTION», up
//! to three tests a block. Each tick the **settings** (`Action::is_setting`) whose tests hold apply
//! first, wherever they stand — eat the other niche's food too, strike rivals at food, go no
//! farther from the layer for food, switch a mode on — the first of each kind that applies wins
//! (`Block::setting_kind`), and a test sees what the settings above it set; then the first deciding
//! block whose tests hold and whose action can be done decides the step (`strategy::plan`); an
//! action that cannot be done (no prey, no corpse in sight, a hopeless chase) falls through to the
//! next block. When nothing decides, the creature stands. Modes (`Action::Mode`, `Cond::Mode`) are
//! its memory: a setting switches one on for a while, and any block may test it.
//!
//! Every number of behaviour lives in the blocks, never in the world or the genome: thresholds of
//! the tests (`Cond::param`), and each action's parameters (`Action::params`: how much smaller its
//! prey, how careful, how patient, how long it runs, how fast and how far it wanders, whether it
//! bursts). A creature has two programs (`Creature::programs`): the juvenile one while it grows to
//! its size gene, the adult one after.
//!
//! The programs are inherited apart from the gene table: a child copies its parent's; unless it is
//! an exact copy, a third of the numbers drift, like its genes (`Program::drift`, the rule
//! `program_drift` × the gene `program_mutability`), and with the rule `program_mutation` × the
//! same gene one mutation changes each — a number, a test, a negation, an action, the order, a
//! copy, a deletion, a new block, a block switched off or on, a mode and its test together (a
//! pair), a block of the other track copied in (a transfer; `Program::mutate_with`). Founders
//! start from the template of their `strategy` gene (`Program::template`), which carries the
//! values of the behaviour genes the programs replaced.
//!
//! A program is behaviour: it costs no upkeep and creates no energy — an action only chooses where
//! to step or how to stand; `Creature::act` pays. Its catch is its consequences: a program that
//! never flees is eaten, one that never looks for food starves.
//!
//! **`Cond`, `Action` and their parameter tables are append-only**, like the gene tables: their
//! numbers are drawn by the mutation and hashed by the golden test. Labels are game UI (Russian).
//!
//! Here: the program, its founders' templates and the shared `Programs`. Its parts: the numbers'
//! specs (`params`), the tests (`cond`), the actions (`action`), a block (`block`), the cached
//! summary (`summary`) and the drift and mutations (`mutate`).

mod action;
mod block;
mod cond;
mod mutate;
mod params;
mod summary;

use super::strategy::Strategy;
use crate::config::FLEE_TICKS;
use std::sync::Arc;

pub use action::Action;
use block::FILLER;
pub use block::{Block, Test};
pub use cond::Cond;
use params::FLEE_PCT;
pub use params::{ParamSpec, Unit, ticks_word};
use summary::Summary;

/// At most this many blocks: a duplicate or an insertion into a full program does nothing. The
/// window's path masks (`Mind::applied`, `Mind::tried`) have a bit a block.
pub const MAX_BLOCKS: usize = 32;
/// At most this many parameters an action reads.
pub const MAX_ARGS: usize = 8;
/// Tests a block has; «always» fills the unused ones.
pub const TESTS: usize = 3;
/// Numbers a block has: its tests' thresholds, then its action's parameters (`Block::spec`).
const SLOTS: usize = TESTS + MAX_ARGS;
/// Modes a creature can switch on (`Action::Mode`, `Cond::Mode`), numbered from 1.
pub const MODES: usize = 4;

/// A creature's behaviour in one stage of its life: its blocks in order, and how many mutations
/// changed it since its template (the drift of its numbers aside).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Program {
    blocks: [Block; MAX_BLOCKS],
    len: u8,
    /// Mutations since the template that changed something, for showing.
    pub changes: u16,
    summary: Summary,
}

/// The stages of life a creature has a program for: `Creature::programs[stage]`.
pub const JUVENILE: usize = 0;
pub const ADULT: usize = 1;

/// The founders' modes (`Action::Mode`, `Cond::Mode`): their memory of an alarm, of a full tank
/// and of hunger. The founders remember through modes rather than the special tests («ещё
/// убегает», «отдыхает»), so every line starts with a working memory a mutation can rebuild.
pub const ALARM_MODE: u16 = 1;
pub const FULL_MODE: u16 = 2;
pub const HUNGRY_MODE: u16 = 3;
/// The founders' alarm lasts the tick it is raised and the old flight's memory after it, so the
/// template runs as long as the old strategy did.
const ALARM_TICKS: u16 = FLEE_TICKS as u16 + 1;

/// The founders' behaviour — the old hand-written strategy with the base values of the behaviour
/// genes the programs replaced — wandering at `pace` % of its speed.
const fn founders(pace: u16) -> Program {
    // the old bravery 50%: a calm stranger may come half as near as a hunting one
    let calm = FLEE_PCT / 2;
    Program::of(&[
        // what the genes and the world did until `life-behavior/14`: the layer 5‒100%, the social
        // layer's smoothing, the division, the healing, eating whatever it touches, the children
        // spared until grown
        Block::does(Action::Layer),
        Block::does(Action::Smooth),
        // it mates from half its tank, divides from 70% (both keep `REPRO_RESERVE` besides) and goes
        // to a partner it sees from 70% fullness: a body of the base tank (100) seeks from 70% and
        // divides alone at 90%, a meat founder twice its size (200) at 80% (`life-behavior/16`)
        Block::does(Action::Mate),
        Block::does(Action::Divide),
        Block::does(Action::Heal),
        Block::does(Action::Graze),
        Block::does(Action::Spare),
        // hunger remembered 60 ticks: below the old picky and rivalry (30%) it eats the other
        // niche's food and fights for its own
        Block::when(Test::at(Cond::Fullness, 30).not(), Action::Mode).with(0, HUNGRY_MODE),
        Block::when(Test::at(Cond::Mode, HUNGRY_MODE), Action::EatForeign),
        Block::when(Test::at(Cond::Mode, HUNGRY_MODE), Action::Rival),
        // an alarm remembered the old flight's memory: a hunter within the flight distance, a calm
        // stranger within half of it
        Block::when(Test::at(Cond::HunterNear, FLEE_PCT), Action::Mode)
            .with(0, ALARM_MODE)
            .with(1, ALARM_TICKS),
        Block::when(Test::at(Cond::ThreatNear, calm), Action::Mode).with(0, ALARM_MODE).with(1, ALARM_TICKS),
        // a full tank remembered 200 ticks: the old rest gene, from 95% fullness
        Block::when(Test::at(Cond::Fullness, 95), Action::Mode).with(0, FULL_MODE).with(1, 200),
        // the old bravery 50%: it defends itself down to half its health
        Block::when(Test::at(Cond::Health, 50), Action::FightBack),
        Block::when(Test::at(Cond::Mode, ALARM_MODE), Action::Flee),
        // the old parent's cover, while it stood firm: 10 points above its fight-back threshold
        Block::when(Test::at(Cond::Health, 60), Action::DefendChild),
        // nearly ready to bear, it goes to a partner it sees, before it would divide alone
        Block::when2(Test::at(Cond::Fullness, 70), Test::is(Cond::MateSeen), Action::SeekMate),
        Block::does(Action::Hunt),
        // the rest is given up 10 points below where it began
        Block::when2(Test::at(Cond::Mode, FULL_MODE), Test::at(Cond::Fullness, 85), Action::Rest),
        Block::does(Action::EatCorpse),
        Block::does(Action::EatPlant),
        Block::does(Action::Wander).with(0, pace),
    ])
}

impl Program {
    /// The standard behaviour: wandering at full speed.
    pub const STANDARD: Program = founders(100);

    /// The lurker: the same, but it wanders at a third of its speed.
    pub const LURKER: Program = founders(33);

    /// A program of these blocks; panics on none or more than `MAX_BLOCKS`.
    pub const fn of(list: &[Block]) -> Program {
        assert!(!list.is_empty() && list.len() <= MAX_BLOCKS, "a program has 1 to MAX_BLOCKS blocks");
        let mut blocks = [FILLER; MAX_BLOCKS];
        let mut i = 0;
        while i < list.len() {
            blocks[i] = list[i];
            i += 1;
        }
        Program::made(blocks, list.len())
    }

    const fn made(blocks: [Block; MAX_BLOCKS], len: usize) -> Program {
        Program { blocks, len: len as u8, changes: 0, summary: Summary::of(&blocks, len) }
    }

    /// The founders' program of a strategy.
    pub const fn template(strategy: Strategy) -> Program {
        match strategy {
            Strategy::Standard => Program::STANDARD,
            Strategy::Lurker => Program::LURKER,
        }
    }

    /// A founder's program: its strategy's template in the layer `layer` (% of the depth), with a
    /// shooting setting for a shooter (after the other settings).
    pub fn founder(strategy: Strategy, layer: (u16, u16), shoots: bool) -> Program {
        let mut p = Program::template(strategy)
            .tuned(Action::Layer, |b| b.args[..2].copy_from_slice(&[layer.0, layer.1]));
        if shoots {
            let at = p.blocks().iter().take_while(|b| b.action.is_setting()).count();
            p.insert(at, Block::does(Action::Shoot));
            p.summary = Summary::of(&p.blocks, usize::from(p.len));
        }
        p
    }

    /// Its home layer, shares of the depth: where a founder is placed (`Summary::layer`).
    pub fn home_layer(&self) -> (f64, f64) {
        let (lo, hi) = self.summary.layer;
        (f64::from(lo) / 100.0, f64::from(hi) / 100.0)
    }

    /// Its home layer, % of the depth.
    pub fn home_layer_pct(&self) -> (u16, u16) {
        self.summary.layer
    }

    /// Whether it ever shoots (a live shooting setting).
    pub fn shoots(&self) -> bool {
        self.summary.shoots
    }

    /// Whether it defends its children (a live «защищать детёныша» block).
    pub fn defends(&self) -> bool {
        self.summary.defends
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks[..usize::from(self.len)]
    }

    /// The same program with every block of `action` changed by `f` (tests, tools).
    pub fn tuned(mut self, action: Action, f: impl Fn(&mut Block)) -> Program {
        let len = usize::from(self.len);
        self.blocks[..len].iter_mut().filter(|b| b.action == action).for_each(f);
        self.summary = Summary::of(&self.blocks, len);
        self
    }

    /// How many blocks are ever reached: up to the first deciding one that always fires. The
    /// deciding blocks after it are kept — they drift without effect and may come back after a
    /// deletion or a swap; the settings after it still apply.
    pub fn reachable(&self) -> usize {
        usize::from(self.summary.reachable)
    }

    /// Whether block `i` may act: not switched off, and a deciding block that is reached or a
    /// setting that no earlier unconditional setting of its kind hides.
    pub fn live(&self, i: usize) -> bool {
        i < usize::from(self.len) && self.summary.live & (1 << i) != 0
    }

    /// Blocks that never act (`live`).
    #[cfg(test)]
    fn dead(&self) -> usize {
        usize::from(self.len) - self.summary.live.count_ones() as usize
    }

    /// The first deciding block that always fires, which ends what is reached; None when none does
    /// (nothing deciding is then a way the creature can come to stand).
    pub fn ending(&self) -> Option<usize> {
        let last = self.reachable().checked_sub(1)?;
        self.blocks[last].always_fires().then_some(last)
    }

    /// How many times smaller its most permissive hunt takes prey (`Cond::PreySeen`; what the
    /// others read of it before its first move, `Menace::of`). None — it never hunts.
    pub fn hunt_ratio(&self) -> Option<f64> {
        (self.summary.hunt > 0).then(|| f64::from(self.summary.hunt) / 100.0)
    }

    /// Its defence, from its first live fight-back block: the enemy's size ratio it still fights,
    /// and the health share down to which it does (0 when the block does not test health). None:
    /// it never fights back.
    pub fn defence(&self) -> Option<(f64, f64)> {
        let s = self.summary;
        (s.fight > 0).then(|| (f64::from(s.fight) / 100.0, f64::from(s.fight_health) / 100.0))
    }

    /// `hunt_ratio` and `defence` raw, as `Menace` keeps them (hundredths, %; 0: none).
    pub(super) fn menace_raw(&self) -> (u16, (u16, u16)) {
        let s = self.summary;
        (s.hunt, (s.fight, s.fight_health))
    }

    /// The farthest a threat test looks, a share of sight: the first threat query covers it.
    pub fn threat_range(&self) -> f64 {
        f64::from(self.summary.threat) / 100.0
    }

    /// How far its first live wander block picks a target, a share of sight (the base without
    /// one): a creature that has just eaten picks its next one as far.
    pub fn wander_reach(&self) -> f64 {
        f64::from(self.summary.wander) / 100.0
    }

    /// Whether two programs behave alike: the same blocks, whatever the mutations that led there.
    pub fn same_blocks(&self, other: &Program) -> bool {
        self.blocks() == other.blocks()
    }

    /// Its blocks without their numbers (`Block::shape`).
    pub fn shape(&self) -> Vec<u64> {
        self.blocks().iter().map(Block::shape).collect()
    }

    /// Of programs of one shape, the one with every number the median of theirs (the upper of two
    /// middle ones): what a group of drifting programs holds to. None for none or mixed shapes.
    pub fn median(programs: &[Program]) -> Option<Program> {
        let mut m = *programs.first()?;
        Program::columns(programs, |i, slot, _, xs| *m.blocks[i].number(slot) = xs[xs.len() / 2])?;
        m.summary = Summary::of(&m.blocks, usize::from(m.len));
        Some(m)
    }

    /// How far the numbers of programs of one shape have spread: the mean over its numbers of
    /// the interquartile range as a share of the number's range (0: all alike, about 0.5: as
    /// random values). None for none or mixed shapes.
    pub fn spread(programs: &[Program]) -> Option<f64> {
        let (mut sum, mut count) = (0.0, 0);
        Program::columns(programs, |_, _, spec, xs| {
            let (lo, hi) = (xs[xs.len() / 4], xs[xs.len() * 3 / 4]);
            sum += f64::from(hi - lo) / f64::from(spec.hi - spec.lo).max(1.0);
            count += 1;
        })?;
        (count > 0).then(|| sum / f64::from(count))
    }

    /// Each drifting number of programs of one shape, sorted across them, given to `f` with its
    /// block, slot and spec. None (and no call) for none or mixed shapes.
    fn columns(programs: &[Program], mut f: impl FnMut(usize, usize, ParamSpec, &[u16])) -> Option<()> {
        let first = programs.first()?;
        let shape = first.shape();
        if programs.iter().any(|p| p.shape() != shape) {
            return None;
        }
        let mut xs = Vec::with_capacity(programs.len());
        for i in 0..usize::from(first.len) {
            for (slot, spec) in first.blocks[i].numbers() {
                xs.clear();
                xs.extend(programs.iter().map(|p| p.blocks[i].get(slot)));
                xs.sort_unstable();
                f(i, slot, spec, &xs);
            }
        }
        Some(())
    }

    /// The blocks as text lines, «2. если сытость < 30% → установка: гнать соперников у еды
    /// (соперник мельче в 1,5 раза)». Game UI.
    pub fn describe(&self) -> Vec<String> {
        self.blocks()
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let args = b.args_label();
                let args = if args.is_empty() { String::new() } else { format!(" ({args})") };
                let kind = if b.action.is_setting() { "установка: " } else { "" };
                format!("{}. если {} → {kind}{}{args}", i + 1, b.condition_label(), b.action.label())
            })
            .collect()
    }
}

/// A creature's two programs (`JUVENILE`, `ADULT`), shared rather than copied: most children
/// inherit them unchanged (every exact copy does), so a creature holds a pointer, not a kilobyte,
/// and the phases that walk the creatures stay compact. Read as an array; replaced whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Programs(Arc<[Program; 2]>);

impl Programs {
    pub fn new(programs: [Program; 2]) -> Programs {
        Programs(Arc::new(programs))
    }

    /// Both tracks the same program: a founder's template.
    pub fn both(program: Program) -> Programs {
        Programs::new([program; 2])
    }
}

impl std::ops::Deref for Programs {
    type Target = [Program; 2];

    fn deref(&self) -> &[Program; 2] {
        &self.0
    }
}

impl From<[Program; 2]> for Programs {
    fn from(programs: [Program; 2]) -> Programs {
        Programs::new(programs)
    }
}

impl PartialEq<[Program; 2]> for Programs {
    fn eq(&self, other: &[Program; 2]) -> bool {
        *self.0 == *other
    }
}

#[cfg(test)]
mod tests;
