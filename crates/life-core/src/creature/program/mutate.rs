//! How a program changes in a child: the drift of its numbers (`Program::drift`) and one
//! structural mutation now and then (`Program::mutate_with`, `MUTATIONS`). No junk is born: an added
//! block that would leave any block dead is not added.

use crate::config::PROGRAM_DRIFT_SHARE;
use crate::rng::Rng;

use super::cond::MODE;
use super::{Action, Block, Cond, FILLER, MAX_BLOCKS, ParamSpec, Program, SLOTS, Summary, TESTS, Test};

impl Program {
    /// A child's drift: a third of the numbers of the program (`PROGRAM_DRIFT_SHARE`; never the
    /// flags and the indices) move by gauss(0, its nudge × `share`), held in range — `share` is
    /// the rule `program_drift` × the parent's `program_mutability` — as its genes drift. One draw
    /// a number, and two more (a gauss) for each that moves; none at all when `share` is 0. Not
    /// counted in `changes`.
    pub fn drift(&mut self, share: f64, rng: &mut Rng) {
        if share <= 0.0 {
            return;
        }
        let len = usize::from(self.len);
        for b in &mut self.blocks[..len] {
            // the slots of `Block::numbers`, in its order, without collecting them
            for slot in 0..SLOTS {
                if let Some(spec) = b.spec(slot).filter(ParamSpec::drifts)
                    && rng.random() < PROGRAM_DRIFT_SHARE
                {
                    let raw = b.number(slot);
                    *raw = spec.moved(*raw, spec.nudge * share, rng);
                }
            }
        }
        self.summary = Summary::of(&self.blocks, len);
    }

    /// The child's program without the other track: no transfer (tests, tools).
    pub fn mutate(&mut self, chance: f64, rng: &mut Rng) {
        self.mutate_with(chance, None, rng);
    }

    /// The child's program: with chance `chance` (the rule × the parent's `program_mutability`)
    /// one mutation, else an exact copy. One draw always; a mutation draws its kind and what it
    /// needs. `other` is the parent's other track, for a transfer. A mutation that cannot apply (a
    /// full program, a single block, nothing to nudge, no test to negate, nothing to copy) or lands
    /// where it was changes nothing and is not counted. No junk is born: a copy is of a live block
    /// that can stay live beside the original (not an unconditional setting, not a block that
    /// always fires), a new or copied deciding block goes where it is reached — no lower than the
    /// block that always fires — and a block added (a copy, a new one, a pair, a transfer) that
    /// would leave a block dead, itself or another, is not added, its draws spent.
    pub fn mutate_with(&mut self, chance: f64, other: Option<&Program>, rng: &mut Rng) {
        if rng.random() >= chance {
            return;
        }
        let before = *self;
        let len = usize::from(self.len);
        let ending = self.ending();
        let u = rng.random();
        let pick = |rng: &mut Rng, n: usize| rng.randint(0, n as i64 - 1) as usize;
        // where a block goes: a setting anywhere, a deciding one above the block that always fires
        // (or anywhere when none does)
        let place = |rng: &mut Rng, block: &Block| match ending {
            Some(end) if !block.action.is_setting() => pick(rng, end + 1),
            _ => pick(rng, len + 1),
        };
        // a live block of a program worth copying: its copy can live beside it
        let copy_of = |rng: &mut Rng, p: &Program| {
            let copyable = |i: usize| {
                let b = &p.blocks[i];
                p.live(i) && !(b.action.is_setting() && b.unconditional()) && !b.always_fires()
            };
            pick_where(rng, usize::from(p.len), copyable).map(|i| p.blocks[i])
        };
        // a live block of the other track: no original stands beside it here, so an unconditional
        // setting or an always-firing block may come too — the no-junk check below turns away a
        // copy that would be dead or kill one
        let transferable = |rng: &mut Rng, p: &Program| {
            pick_where(rng, usize::from(p.len), |i| p.live(i)).map(|i| p.blocks[i])
        };
        let mut acc = 0.0;
        let mut op = MUTATIONS.len() - 1;
        for (k, &(_, share)) in MUTATIONS.iter().enumerate() {
            acc += share;
            if u < acc {
                op = k;
                break;
            }
        }
        // where an adding mutation (a copy, a new block, a pair, a transfer) put its block
        let mut added = None;
        match MUTATIONS[op].0 {
            Mutation::Nudge => {
                // any number of the program: the tests' thresholds (slots 0‒2) and the action's (3‒)
                let blocks = &self.blocks;
                if let Some(k) = pick_where(rng, len * SLOTS, |k| blocks[k / SLOTS].spec(k % SLOTS).is_some())
                {
                    let (i, slot) = (k / SLOTS, k % SLOTS);
                    let spec = self.blocks[i].spec(slot).expect("a number");
                    let raw = self.blocks[i].number(slot);
                    *raw = spec.nudged(*raw, rng);
                }
            }
            Mutation::Condition => {
                let (i, s) = (pick(rng, len), pick(rng, TESTS));
                self.blocks[i].when[s] = random_test(rng);
            }
            Mutation::Negate => {
                // only a test with a condition: «always» is `Toggle`'s, so that a negation does
                // not knock a block out two times in three
                let i = pick(rng, len);
                let when = self.blocks[i].when;
                if let Some(s) = pick_where(rng, TESTS, |s| when[s].cond != Cond::Always) {
                    self.blocks[i].when[s].negate ^= true;
                }
            }
            Mutation::Action => {
                let i = pick(rng, len);
                let action = Action::ALL[pick(rng, Action::ALL.len())];
                let b = &mut self.blocks[i];
                b.args = action.args_from(b.action, &b.args);
                b.action = action;
            }
            Mutation::Swap => {
                if len >= 2 {
                    let i = pick(rng, len - 1);
                    self.blocks.swap(i, i + 1);
                }
            }
            Mutation::Duplicate => {
                if len < MAX_BLOCKS
                    && let Some(block) = copy_of(rng, self)
                {
                    let at = place(rng, &block);
                    self.insert(at, block);
                    added = Some(at);
                }
            }
            Mutation::Delete => {
                if len > 1 {
                    // a dead block first — switched off, never reached, or a setting hidden by one
                    // of its kind — so that the dead do not pile up; any block when none is dead
                    let i = pick_where(rng, len, |i| !self.live(i)).unwrap_or_else(|| pick(rng, len));
                    self.blocks.copy_within(i + 1..len, i);
                    self.blocks[len - 1] = FILLER;
                    self.len -= 1;
                }
            }
            Mutation::Insert => {
                if len < MAX_BLOCKS {
                    let test = random_test(rng);
                    let action = Action::ALL[pick(rng, Action::ALL.len())];
                    let block = Block::when(test, action);
                    let at = place(rng, &block);
                    self.insert(at, block);
                    added = Some(at);
                }
            }
            Mutation::Pair => {
                // a memory in one step: a setting that switches a mode on by a random test, and
                // that mode as a test on another live block with a free «always» — apart they
                // would be two mutations with nothing to select between them. Not the block that
                // always fires: a condition on it would bring the dead tail back to life. No such
                // reader: no memory either.
                if len < MAX_BLOCKS {
                    let mode = MODE.random(rng);
                    let setting = Block::when(random_test(rng), Action::Mode).with(0, mode);
                    let at = pick(rng, len + 1);
                    self.insert(at, setting);
                    let with = Summary::of(&self.blocks, len + 1);
                    let blocks = &self.blocks;
                    let reader = |i: usize| {
                        let b = &blocks[i];
                        i != at
                            && with.live & (1 << i) != 0
                            && !b.always_fires()
                            && b.when.iter().any(|t| t.always())
                    };
                    match pick_where(rng, len + 1, reader) {
                        Some(i) => {
                            let b = &mut self.blocks[i];
                            let slot = b.when.iter().position(|t| t.always()).expect("a free slot");
                            b.when[slot] = Test::at(Cond::Mode, mode);
                            added = Some(at);
                        }
                        None => *self = before,
                    }
                }
            }
            Mutation::Transfer => {
                // a live block of its other track — the adult's into the juvenile's or back —
                // copied to where it is reached: what one stage found the other may try
                if let Some(other) = other
                    && len < MAX_BLOCKS
                    && let Some(block) = transferable(rng, other)
                {
                    let at = place(rng, &block);
                    self.insert(at, block);
                    added = Some(at);
                }
            }
            Mutation::Toggle => {
                // off: every «never» back to «always»; on: the first «always» to «never» (a block
                // of three real tests has none and stays on)
                let i = pick(rng, len);
                let b = &mut self.blocks[i];
                if b.off() {
                    for t in &mut b.when {
                        if t.never() {
                            t.negate = false;
                        }
                    }
                } else if let Some(t) = b.when.iter_mut().find(|t| t.always()) {
                    t.negate = true;
                }
            }
        }
        if (self.blocks, self.len) == (before.blocks, before.len) {
            return;
        }
        self.summary = Summary::of(&self.blocks, usize::from(self.len));
        if added.is_some_and(|at| self.kills(&before, at)) {
            // the added block would be born dead, or would kill one: not added
            *self = before;
            return;
        }
        self.changes = self.changes.saturating_add(1);
    }

    /// With a block added at `at` to `before`: whether it was born dead or left dead a block that
    /// lived before. Block by block, not by their count: a pair that kills one block while its
    /// reader brings another back to life still kills one.
    pub(super) fn kills(&self, before: &Program, at: usize) -> bool {
        let was = |j: usize| match j.cmp(&at) {
            std::cmp::Ordering::Less => Some(j),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(j - 1),
        };
        !self.live(at)
            || (0..usize::from(self.len)).any(|j| was(j).is_some_and(|i| before.live(i) && !self.live(j)))
    }

    pub(super) fn insert(&mut self, at: usize, block: Block) {
        let len = usize::from(self.len);
        self.blocks.copy_within(at..len, at + 1);
        self.blocks[at] = block;
        self.len += 1;
    }
}

/// An index below `n` where `pred` holds, picked uniformly: counts them, draws one, walks again —
/// no allocation. None, and no draw, when none holds.
fn pick_where(rng: &mut Rng, n: usize, pred: impl Fn(usize) -> bool) -> Option<usize> {
    let count = (0..n).filter(|&i| pred(i)).count();
    if count == 0 {
        return None;
    }
    let k = rng.randint(0, count as i64 - 1) as usize;
    (0..n).filter(|&i| pred(i)).nth(k)
}

/// A random test: any condition, a threshold drawn in range for one that has it.
fn random_test(rng: &mut Rng) -> Test {
    let cond = Cond::ALL[rng.randint(0, Cond::ALL.len() as i64 - 1) as usize];
    let param = cond.param().map_or(0, |p| p.random(rng));
    Test { cond, negate: false, param }
}

/// The kinds of a program mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mutation {
    /// A number — a test's threshold or an action's parameter — moves (a flag flips).
    Nudge,
    /// A test becomes another random one.
    Condition,
    /// A test with a condition turns to its opposite («always» is left to `Toggle`).
    Negate,
    /// A block's action becomes a random one; a parameter of the same label and unit keeps its
    /// number, the others take their bases (`Action::args_from`).
    Action,
    /// A block trades places with the next one.
    Swap,
    /// A copy of a live block goes where it is reached.
    Duplicate,
    /// A block goes, a dead one first.
    Delete,
    /// A new block of one random test goes where it is reached.
    Insert,
    /// A block is switched off (its first «always» becomes «never») or on again.
    Toggle,
    /// A memory: a setting switching a mode on by a random test, and that mode as a test on
    /// another block.
    Pair,
    /// A live block of the other track copied in.
    Transfer,
}

/// The mutation kinds and their shares of the mutations; the shares sum to 1. Small changes
/// (a number, the order) are the most common, so a working program usually stays working. A
/// deletion is as likely as the kinds that add a block together (a copy, an insertion, a pair and
/// a transfer, which a child always has the other track for), so programs do not grow by
/// themselves; switching a block off is rare and its own kind.
pub(super) const MUTATIONS: [(Mutation, f64); 11] = [
    (Mutation::Nudge, 0.25),
    (Mutation::Condition, 0.12),
    (Mutation::Negate, 0.08),
    (Mutation::Action, 0.08),
    (Mutation::Swap, 0.12),
    (Mutation::Duplicate, 0.06),
    (Mutation::Delete, 0.16),
    (Mutation::Insert, 0.05),
    (Mutation::Toggle, 0.03),
    (Mutation::Pair, 0.03),
    (Mutation::Transfer, 0.02),
];
