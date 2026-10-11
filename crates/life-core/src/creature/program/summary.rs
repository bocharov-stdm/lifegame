//! What a program's blocks say at a glance (`Summary`), cached in each `Program` and recomputed
//! on every change.

use super::params::WANDER_REACH_PCT;
use super::{Action, Block, Cond, MAX_BLOCKS, TESTS};

/// What the rest of the world reads of a program every tick, worked out when the program is made
/// or changed. Raw numbers, as the blocks keep them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Summary {
    /// Blocks up to the first deciding one that always fires.
    pub(super) reachable: u8,
    /// The blocks that may act, a bit a block (`Program::live`).
    pub(super) live: u32,
    /// The farthest a threat test looks, % of sight.
    pub(super) threat: u16,
    /// Its most permissive hunt's ratio; 0: it never hunts. Its own `Cond::PreySeen` reads it;
    /// the others read what its blocks did on its last move (`Menace`), the shape only before it.
    pub(super) hunt: u16,
    /// Its first fight-back block's ratio (0: it never fights back) and health threshold.
    pub(super) fight: u16,
    pub(super) fight_health: u16,
    /// How far its first wander looks for a target, % of sight.
    pub(super) wander: u16,
    /// Its home layer, % of the depth: its first live unconditional layer setting (a founder is
    /// placed there); the whole depth without one.
    pub(super) layer: (u16, u16),
    /// It has a live shooting setting.
    pub(super) shoots: bool,
    /// It has a live block defending its children.
    pub(super) defends: bool,
}

impl Summary {
    pub(super) const fn of(blocks: &[Block; MAX_BLOCKS], len: usize) -> Summary {
        let mut reachable = len;
        let mut i = 0;
        while i < len {
            if blocks[i].always_fires() {
                reachable = i + 1;
                break;
            }
            i += 1;
        }
        // live: not switched off, and a deciding block reached or a setting that no earlier
        // unconditional one of its kind hides (only the first of a kind whose tests hold applies)
        let (mut live, mut hidden) = (0_u32, 0_u64);
        let mut i = 0;
        while i < len {
            let b = &blocks[i];
            if b.off() {
                // switched off
            } else if b.action.is_setting() {
                let kind = b.setting_kind();
                if hidden & kind == 0 {
                    live |= 1 << i;
                    if b.unconditional() {
                        hidden |= kind;
                    }
                }
            } else if i < reachable {
                live |= 1 << i;
            }
            i += 1;
        }
        let mut s = Summary {
            reachable: reachable as u8,
            live,
            threat: 0,
            hunt: 0,
            fight: 0,
            fight_health: 0,
            wander: 0,
            layer: (0, 100),
            shoots: false,
            defends: false,
        };
        let (mut fights, mut wanders, mut layered) = (false, false, false);
        i = 0;
        while i < len {
            let b = &blocks[i];
            if live & (1 << i) != 0 {
                let mut t = 0;
                while t < TESTS {
                    let test = b.when[t];
                    if !test.negate
                        && matches!(test.cond, Cond::ThreatNear | Cond::HunterNear)
                        && test.param > s.threat
                    {
                        s.threat = test.param;
                    }
                    t += 1;
                }
                match b.action {
                    Action::Hunt if s.hunt == 0 || b.args[0] < s.hunt => s.hunt = b.args[0],
                    Action::FightBack if !fights => {
                        fights = true;
                        s.fight = b.args[0];
                        s.fight_health = b.fight_health();
                    }
                    Action::Wander if !wanders => {
                        wanders = true;
                        s.wander = b.args[1];
                    }
                    Action::Layer if !layered && b.unconditional() => {
                        layered = true;
                        let (a, z) = (b.args[0], b.args[1]);
                        s.layer = if a <= z { (a, z) } else { (z, a) };
                    }
                    Action::Shoot => s.shoots = true,
                    Action::DefendChild => s.defends = true,
                    _ => {}
                }
            }
            i += 1;
        }
        if !wanders {
            s.wander = WANDER_REACH_PCT;
        }
        s
    }
}
