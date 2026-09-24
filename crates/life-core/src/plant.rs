//! Растения: неподвижная еда. Где они вырастают, решает профиль мира (`flora.rs`).

pub const PORTIONS: u8 = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    pub x: f64,
    pub y: f64,
    /// Съеденное помечается, а выметается раз за тик в конце хода существ.
    pub alive: bool,
    /// Одна порция за контактный тик; первоначально их пять.
    pub portions: u8,
    /// Тик, на котором выросло (с насыщением на u32). Движку не нужен: по нему
    /// окно узнаёт новые растения и сопоставляет кадры. Лежит в выравнивании —
    /// растение от него не толстеет.
    pub born: u32,
}

impl Plant {
    pub fn at(x: f64, y: f64) -> Self {
        Plant { x, y, alive: true, portions: PORTIONS, born: 0 }
    }

    /// Возвращает `Some(true)`, если съедена последняя порция.
    pub fn bite(&mut self) -> Option<bool> {
        if !self.alive || self.portions == 0 {
            return None;
        }
        self.portions -= 1;
        if self.portions == 0 {
            self.alive = false;
        }
        Some(!self.alive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Растений в огромном мире миллионы: метка рождения не должна их толстить.
    #[test]
    fn растение_не_толстеет() {
        assert_eq!(std::mem::size_of::<Plant>(), 24);
    }

    #[test]
    fn растение_исчезает_после_пяти_порций() {
        let mut plant = Plant::at(1.0, 2.0);
        for left in (1..5).rev() {
            assert_eq!(plant.bite(), Some(false));
            assert_eq!(plant.portions, left);
            assert!(plant.alive);
        }
        assert_eq!(plant.bite(), Some(true));
        assert_eq!(plant.bite(), None);
        assert!(!plant.alive);
    }
}
