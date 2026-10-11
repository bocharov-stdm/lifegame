//! A block «if up to three tests → an action(params)» (`Block`) and its tests (`Test`).

use super::{Action, Cond, MAX_ARGS, ParamSpec, SLOTS, TESTS, Unit};

/// One check of a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Test {
    pub cond: Cond,
    pub negate: bool,
    /// The threshold of a test that has one (`Cond::param`), raw.
    pub param: u16,
}

impl Test {
    pub const ALWAYS: Test = Test::is(Cond::Always);

    /// The test at its base threshold, if it has one.
    pub const fn is(cond: Cond) -> Test {
        let param = match cond.param() {
            Some(p) => p.base,
            None => 0,
        };
        Test { cond, negate: false, param }
    }

    pub const fn at(cond: Cond, param: u16) -> Test {
        Test { cond, negate: false, param }
    }

    pub const fn not(self) -> Test {
        Test { negate: !self.negate, ..self }
    }

    /// Whether it always holds.
    pub const fn always(self) -> bool {
        matches!(self.cond, Cond::Always) && !self.negate
    }

    /// Whether it never holds: a negated «always» switches its block off.
    pub const fn never(self) -> bool {
        matches!(self.cond, Cond::Always) && self.negate
    }

    /// The threshold as the engine reads it (a share for percents, ticks for a time).
    pub fn value(self) -> f64 {
        self.cond.param().map_or(0.0, |p| p.value(self.param))
    }

    /// Game UI: «сытость < 40%», «угроза ближе 33% зрения», «его ударили за 1 тик».
    pub fn label(self) -> String {
        let (yes, no) = self.cond.labels();
        let text = if self.negate { no } else { yes };
        match self.cond.param() {
            Some(p) => format!("{text} {}", p.amount(self.param)),
            None => text.to_string(),
        }
    }
}

/// «If all its tests hold → the action with its parameters».
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Block {
    pub when: [Test; TESTS],
    pub action: Action,
    /// The action's parameters, raw, in the order of `Action::params`; the rest are zero.
    pub args: [u16; MAX_ARGS],
}

/// What fills a program past its end: kept the same, so programs of the same blocks are equal.
pub(super) const FILLER: Block = Block::does(Action::Wander);

impl Block {
    /// Unconditional, the parameters at their bases.
    pub const fn does(action: Action) -> Block {
        Block::when3(Test::ALWAYS, Test::ALWAYS, Test::ALWAYS, action)
    }

    pub const fn when(test: Test, action: Action) -> Block {
        Block::when3(test, Test::ALWAYS, Test::ALWAYS, action)
    }

    pub const fn when2(a: Test, b: Test, action: Action) -> Block {
        Block::when3(a, b, Test::ALWAYS, action)
    }

    pub const fn when3(a: Test, b: Test, c: Test, action: Action) -> Block {
        Block { when: [a, b, c], action, args: action.base_args() }
    }

    /// A setting's kind as a bit: of the settings of one kind whose tests hold, only the first
    /// applies. Each mode is a kind of its own, so one tick can switch several on.
    pub const fn setting_kind(&self) -> u64 {
        match self.action {
            Action::Mode => 1 << (48 + self.args[0]),
            a => 1 << a as u8,
        }
    }

    /// The same block with parameter `i` set, raw.
    pub const fn with(mut self, i: usize, raw: u16) -> Block {
        self.args[i] = raw;
        self
    }

    /// Parameter `i` as the engine reads it.
    pub fn arg(&self, i: usize) -> f64 {
        self.action.params()[i].value(self.args[i])
    }

    /// Whether a flag parameter is on.
    pub fn flag(&self, i: usize) -> bool {
        self.args[i] != 0
    }

    /// Switched off: a test never holds.
    pub const fn off(&self) -> bool {
        self.when[0].never() || self.when[1].never() || self.when[2].never()
    }

    /// Whether it decides every time it is reached.
    pub const fn always_fires(&self) -> bool {
        self.unconditional() && self.action.never_fails()
    }

    /// Its tests as one line: «сытость < 40% и его ударили за 1 тик»; «всегда» when there are none.
    pub fn condition_label(&self) -> String {
        let tests: Vec<String> = self.when.iter().filter(|t| !t.always()).map(|t| t.label()).collect();
        if tests.is_empty() { Test::ALWAYS.label() } else { tests.join(" и ") }
    }

    /// Its parameters as text: «добыча мельче в 1,5 раза, осторожность 100%». Game UI.
    pub fn args_label(&self) -> String {
        let params = self.action.params();
        params.iter().zip(self.args).map(|(p, raw)| p.show(raw)).collect::<Vec<_>>().join(", ")
    }

    /// Everything about the block packed into numbers: for digests and counting programs. A test
    /// takes 18 bits (condition 5, negation 1, threshold 12), the action 5, a parameter 12.
    pub fn code(&self) -> [u64; 3] {
        let test = |t: Test| u64::from(t.cond as u8) | u64::from(t.negate) << 5 | u64::from(t.param) << 6;
        let head = test(self.when[0])
            | test(self.when[1]) << 18
            | test(self.when[2]) << 36
            | u64::from(self.action as u8) << 54;
        let mut args = [0; 2];
        for (i, a) in self.args.iter().enumerate() {
            args[i / 5] |= u64::from(*a) << (12 * (i % 5));
        }
        [head, args[0], args[1]]
    }

    /// The block without its numbers — its tests' conditions and negations, its action, its flags
    /// and its indices (a mode's number): what stays while the numbers drift. The report groups
    /// programs by it.
    pub fn shape(&self) -> u64 {
        let test = |t: Test| {
            let index = if t.cond.param().is_some_and(|p| p.unit == Unit::Index) { t.param } else { 0 };
            u64::from(t.cond as u8) | u64::from(t.negate) << 5 | u64::from(index) << 6
        };
        let mut fixed = 0_u64;
        for (i, p) in self.action.params().iter().enumerate() {
            if !p.drifts() {
                fixed |= u64::from(self.args[i]) << (3 * i);
            }
        }
        test(self.when[0])
            | test(self.when[1]) << 9
            | test(self.when[2]) << 18
            | u64::from(self.action as u8) << 27
            | fixed << 32
    }

    /// Whether every test is «always»: it applies (a setting) or is tried (a deciding block) each
    /// time it is reached.
    pub const fn unconditional(&self) -> bool {
        self.when[0].always() && self.when[1].always() && self.when[2].always()
    }

    /// The spec of the number in `slot`: a test's threshold (slots 0‒2) or a parameter (3‒); None
    /// for a test without a threshold or past the action's parameters.
    pub(super) fn spec(&self, slot: usize) -> Option<ParamSpec> {
        if slot < TESTS {
            self.when[slot].cond.param()
        } else {
            self.action.params().get(slot - TESTS).copied()
        }
    }

    /// The numbers of the block the drift moves: each test's threshold and each parameter but the
    /// flags and indices, as (slot, its spec); slots 0‒2 are the tests, 3‒ the parameters.
    pub(super) fn numbers(&self) -> impl Iterator<Item = (usize, ParamSpec)> + '_ {
        (0..SLOTS).filter_map(|s| self.spec(s).filter(ParamSpec::drifts).map(|p| (s, p)))
    }

    /// The health share down to which a fight-back block fights: its highest «health ≥» test
    /// (0 without one). `Summary::of` reads the shape by it, `Menace` a creature's last move.
    pub const fn fight_health(&self) -> u16 {
        let mut health = 0;
        let mut t = 0;
        while t < TESTS {
            let test = self.when[t];
            if !test.negate && matches!(test.cond, Cond::Health) && test.param > health {
                health = test.param;
            }
            t += 1;
        }
        health
    }

    pub(super) fn number(&mut self, slot: usize) -> &mut u16 {
        if slot < TESTS { &mut self.when[slot].param } else { &mut self.args[slot - TESTS] }
    }

    pub(super) fn get(&self, slot: usize) -> u16 {
        if slot < TESTS { self.when[slot].param } else { self.args[slot - TESTS] }
    }
}
