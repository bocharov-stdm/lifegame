//! Растения: неподвижная еда. Где они вырастают, решает профиль мира (`flora.rs`).

pub const PORTIONS: u8 = 5;

/// No slot: a plant put in by hand (tests, the app) or grown before the patch layout changed.
/// It holds no place and frees none when eaten.
pub const NO_SLOT: usize = (1 << 24) - 1;

#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub x: f64,
    pub y: f64,
    /// Одна порция за контактный тик; первоначально их пять. Съеденное (ноль порций)
    /// выметается раз за тик в конце хода существ.
    pub portions: u8,
    /// The place it holds (`flora.rs`), 24 bits: it fits into the padding, like `born`.
    slot: [u8; 3],
    /// Тик, на котором выросло (с насыщением на u32). Движку не нужен: по нему
    /// окно узнаёт новые растения и сопоставляет кадры. Лежит в выравнивании —
    /// растение от него не толстеет.
    pub born: u32,
}

impl Plant {
    /// A plant without a slot.
    pub fn at(x: f64, y: f64) -> Self {
        Plant::in_slot(x, y, NO_SLOT)
    }

    pub fn in_slot(x: f64, y: f64, slot: usize) -> Self {
        debug_assert!(slot <= NO_SLOT, "a slot takes 24 bits");
        let [a, b, c, ..] = (slot as u32).to_le_bytes();
        Plant { x, y, portions: PORTIONS, slot: [a, b, c], born: 0 }
    }

    pub fn alive(&self) -> bool {
        self.portions > 0
    }

    /// The place it holds, if any.
    pub fn slot(&self) -> Option<usize> {
        let [a, b, c] = self.slot;
        let slot = u32::from_le_bytes([a, b, c, 0]) as usize;
        (slot != NO_SLOT).then_some(slot)
    }

    /// Gives up its place: the patch layout it grew by is gone.
    pub fn lose_slot(&mut self) {
        self.slot = [0xFF; 3];
    }

    /// Возвращает `Some(true)`, если съедена последняя порция.
    pub fn bite(&mut self) -> Option<bool> {
        if self.portions == 0 {
            return None;
        }
        self.portions -= 1;
        Some(self.portions == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Растений в огромном мире миллионы: метка рождения и место не должны их толстить.
    #[test]
    fn растение_не_толстеет() {
        assert_eq!(std::mem::size_of::<Plant>(), 24);
    }

    /// The biggest world's slots fit into 24 bits.
    #[test]
    fn slots_of_the_biggest_world_fit() {
        use crate::{Shape, Space, config::PLANT_MAX, space::MAX_SCALE};
        let scattered = crate::Rules::default().with("plant_patches", 0.0).unwrap();
        for shape in Shape::ALL {
            let space = Space::new(MAX_SCALE, shape);
            for rules in [crate::Rules::default(), scattered.clone()] {
                let flora = crate::flora::Flora::new(&rules, &space, 1);
                assert!(flora.slots() >= space.per_area(PLANT_MAX) && flora.slots() < NO_SLOT, "{shape:?}");
            }
        }
        let p = Plant::in_slot(0.0, 0.0, NO_SLOT - 1);
        assert_eq!(p.slot(), Some(NO_SLOT - 1));
        assert_eq!(Plant::at(0.0, 0.0).slot(), None);
    }

    #[test]
    fn растение_исчезает_после_пяти_порций() {
        let mut plant = Plant::at(1.0, 2.0);
        for left in (1..5).rev() {
            assert_eq!(plant.bite(), Some(false));
            assert_eq!(plant.portions, left);
            assert!(plant.alive());
        }
        assert_eq!(plant.bite(), Some(true));
        assert_eq!(plant.bite(), None);
        assert!(!plant.alive());
    }
}
