//! Create's `SmartBlockEntity`: the lazy tick every smart block entity shares.

/// `SmartBlockEntity`'s lazy tick: `lazyTick()` runs at `initialize`, a block entity's first
/// tick, and again whenever `lazyTickCounter-- <= 0` brings the counter back to `lazyTickRate`:
/// `rate` ticks after the first, then every `rate + 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmartBlockEntity {
    /// `lazyTickRate`.
    lazy_tick_rate: u8,
    /// `lazyTickCounter`.
    lazy_tick_counter: u8,
    initialized: bool,
}

impl SmartBlockEntity {
    /// `setLazyTickRate(10)`, the `SmartBlockEntity` constructor's.
    pub const DEFAULT_LAZY_TICK_RATE: u8 = 10;

    /// A block entity whose constructor called `setLazyTickRate(lazy_tick_rate)`.
    pub fn new(lazy_tick_rate: u8) -> Self {
        Self {
            lazy_tick_rate,
            lazy_tick_counter: lazy_tick_rate,
            initialized: false,
        }
    }

    /// `lazyTickRate`.
    pub fn lazy_tick_rate(&self) -> u8 {
        self.lazy_tick_rate
    }

    /// `setLazyTickRate`.
    pub fn set_lazy_tick_rate(&mut self, lazy_tick_rate: u8) {
        self.lazy_tick_rate = lazy_tick_rate;
        self.lazy_tick_counter = lazy_tick_rate;
    }

    /// The start of `SmartBlockEntity.tick`, at the block entity's turn: whether its `lazyTick`
    /// runs in this tick.
    pub fn tick(&mut self) -> bool {
        let first = !self.initialized;
        self.initialized = true;
        let due = self.count_down();
        first || due
    }

    /// `if (lazyTickCounter-- <= 0)`: whether the counter passed zero; it stands at the rate
    /// again when it did, one lower otherwise.
    fn count_down(&mut self) -> bool {
        if self.lazy_tick_counter == 0 {
            self.lazy_tick_counter = self.lazy_tick_rate;
            true
        } else {
            self.lazy_tick_counter -= 1;
            false
        }
    }
}

impl Default for SmartBlockEntity {
    fn default() -> Self {
        Self::new(Self::DEFAULT_LAZY_TICK_RATE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lazy_ticks(rate: u8, ticks: usize) -> Vec<usize> {
        let mut smart = SmartBlockEntity::new(rate);
        (0..ticks).filter(|_| smart.tick()).collect()
    }

    /// `processing-basin-spout-turn`: with a rate of 10, the first tick's `initialize` calls
    /// `lazyTick`, then the counter (10) brings it round 10 ticks later and every 11 after.
    #[test]
    fn the_lazy_tick_runs_at_initialize_then_every_rate_plus_one() {
        assert_eq!(lazy_ticks(10, 45), [0, 10, 21, 32, 43]);
        assert_eq!(lazy_ticks(3, 15), [0, 3, 7, 11]);
        assert_eq!(lazy_ticks(60, 122), [0, 60, 121]);
        assert_eq!(lazy_ticks(0, 3), [0, 1, 2]);
    }

    /// `setLazyTickRate` sets the counter too, and the block entity's own tick goes on
    /// counting it whether or not the lazy tick ran.
    #[test]
    fn a_new_rate_restarts_the_counter() {
        let mut smart = SmartBlockEntity::default();
        assert!(smart.tick());
        assert!(!smart.tick());
        smart.set_lazy_tick_rate(2);
        let ticks: Vec<bool> = (0..3).map(|_| smart.tick()).collect();
        assert_eq!(ticks, [false, false, true]);
        assert_eq!(smart.lazy_tick_rate(), 2);
    }
}
