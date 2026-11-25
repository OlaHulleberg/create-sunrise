//! `SmartBlockEntity`'s lazy tick: a block entity's `lazyTick` runs once when it first ticks
//! (`initialize`), then every `lazyTickRate + 1` ticks (its counter runs down to zero, then
//! resets).

/// The lazy tick state of one block entity (`initialized`, `lazyTickRate`, `lazyTickCounter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmartBlockEntity {
    initialized: bool,
    lazy_tick_rate: i32,
    lazy_tick_counter: i32,
}

impl SmartBlockEntity {
    /// `setLazyTickRate`: the counter starts at the rate.
    pub fn new(rate: i32) -> Self {
        Self {
            initialized: false,
            lazy_tick_rate: rate,
            lazy_tick_counter: rate,
        }
    }

    /// `SmartBlockEntity.tick`: whether `lazyTick` runs this tick.
    pub fn tick(&mut self) -> bool {
        let mut lazy = false;
        if !self.initialized {
            self.initialized = true;
            lazy = true;
        }
        let count = self.lazy_tick_counter;
        self.lazy_tick_counter -= 1;
        if count <= 0 {
            self.lazy_tick_counter = self.lazy_tick_rate;
            lazy = true;
        }
        lazy
    }
}

impl Default for SmartBlockEntity {
    /// The default lazy tick rate, 10.
    fn default() -> Self {
        Self::new(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn firing(mut be: SmartBlockEntity, ticks: i32) -> Vec<i32> {
        (1..=ticks).filter(|_| be.tick()).collect()
    }

    #[test]
    fn rate_ten_fires_at_initialize_then_every_eleven_ticks() {
        assert_eq!(firing(SmartBlockEntity::default(), 44), [1, 11, 22, 33, 44]);
    }

    #[test]
    fn rate_five_fires_every_six_ticks() {
        assert_eq!(firing(SmartBlockEntity::new(5), 18), [1, 6, 12, 18]);
    }

    #[test]
    fn rate_twenty_fires_every_twenty_one_ticks() {
        assert_eq!(firing(SmartBlockEntity::new(20), 42), [1, 21, 42]);
    }
}
