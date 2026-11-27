//! What the machines over a basin share (`BasinOperatingBlockEntity`, `DeferralBehaviour`): a
//! deferred look at the basin two below, and the flag a basin emptied or removed raises. Both
//! are read at the operator's own turn in the level's block-entity order, so a press registered
//! before its basin finds out a tick after the basin tells it, and a mixer registered after the
//! basin the same tick.
//!
//! The state is the mixer's and the press's own (`basin::Mixer::operator`,
//! `press::Press::operator`): constructed with the block entity, placed or loaded, and gone with
//! it. Of it only `NeedsUpdate` is saved (`DeferralBehaviour.write`); a loaded operator is told
//! by its basin's fresh `contentsChanged` as a placed one is by its speed arriving
//! (`onSpeedChanged`).

/// `DeferralBehaviour`: an update asked for now (`scheduleUpdate`) that runs at the next
/// `tick` of its block entity, and is done once its callback says so.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Deferral {
    needs_update: bool,
}

impl Deferral {
    /// `DeferralBehaviour.read`: the saved `NeedsUpdate`.
    pub fn read(needs_update: bool) -> Deferral {
        Deferral { needs_update }
    }

    /// `scheduleUpdate`.
    pub fn schedule(&mut self) {
        self.needs_update = true;
    }

    /// `tick` for a callback that is always done (`updateBasin` returns true every time):
    /// whether an update was scheduled, now run.
    pub fn take(&mut self) -> bool {
        let mut due = false;
        self.tick(|| {
            due = true;
            true
        });
        due
    }

    /// Whether an update waits for the next tick.
    pub fn pending(&self) -> bool {
        self.needs_update
    }

    /// `tick`: runs `callback` when an update was scheduled; the update is done when it
    /// returns true, else it stays scheduled.
    pub fn tick(&mut self, callback: impl FnOnce() -> bool) {
        if self.needs_update && callback() {
            self.needs_update = false;
        }
    }
}

/// A press's `currentRecipe` that is one of the square crafting recipes (`canCompress`), which
/// the glue keeps by press: not an index into its compacting book.
pub const CRAFTED: usize = usize::MAX;

/// A basin operator's state (`BasinOperatingBlockEntity`): `basinChecker`, `basinRemoved` and
/// `currentRecipe`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Operator {
    pub checker: Deferral,
    /// `basinRemoved`: the basin below was emptied or removed (`BasinBlockEntity.onEmptied`).
    pub removed: bool,
    /// `currentRecipe`: the recipe its last look chose (`updateBasin`), by index into the book
    /// the glue matched it in.
    pub recipe: Option<usize>,
}

impl Operator {
    /// `onSpeedChanged`: it looks at its basin again. Java raises `basinRemoved` for a stopped
    /// operator and clears it on the next line, so a speed change always leaves it down.
    pub fn on_speed_changed(&mut self) {
        self.removed = false;
        self.checker.schedule();
    }

    /// `tick`'s `if (basinRemoved)`: whether the basin was removed, which it then forgets. An
    /// operator that finds it does `onBasinRemoved` and nothing else at that turn (its
    /// behaviours, `basinChecker` among them, do not tick).
    pub fn take_removed(&mut self) -> bool {
        std::mem::take(&mut self.removed)
    }

    /// Whether the operator has something to do at its turn besides what its own cycle asks:
    /// a basin removed or a look at the basin waiting.
    pub fn wants_turn(&self) -> bool {
        self.removed || self.checker.pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_scheduled_never_runs_the_callback() {
        let mut d = Deferral::default();
        d.tick(|| panic!("no update was asked for"));
        assert!(!d.pending());
    }

    #[test]
    fn a_scheduled_update_runs_at_the_next_tick_once() {
        let mut d = Deferral::default();
        d.schedule();
        assert!(d.pending());
        let mut calls = 0;
        d.tick(|| {
            calls += 1;
            true
        });
        assert_eq!(calls, 1);
        assert!(!d.pending());
        d.tick(|| {
            calls += 1;
            true
        });
        assert_eq!(calls, 1);
    }

    #[test]
    fn an_update_whose_callback_is_not_done_stays_scheduled() {
        let mut d = Deferral::default();
        d.schedule();
        d.tick(|| false);
        assert!(d.pending());
        d.tick(|| true);
        assert!(!d.pending());
    }

    #[test]
    fn take_reports_a_scheduled_update_once() {
        let mut d = Deferral::default();
        assert!(!d.take());
        d.schedule();
        assert!(d.take());
        assert!(!d.take());
        assert!(!d.pending());
    }

    #[test]
    fn scheduling_twice_runs_one_update() {
        let mut d = Deferral::default();
        d.schedule();
        d.schedule();
        let mut calls = 0;
        for _ in 0..3 {
            d.tick(|| {
                calls += 1;
                true
            });
        }
        assert_eq!(calls, 1);
    }

    /// `DeferralBehaviour.read`.
    #[test]
    fn read_restores_the_saved_needs_update() {
        assert!(Deferral::read(true).pending());
        assert!(!Deferral::read(false).pending());
        assert_eq!(Deferral::read(false), Deferral::default());
    }

    #[test]
    fn a_speed_change_schedules_a_look_and_clears_a_removed_basin() {
        let mut o = Operator {
            removed: true,
            ..Operator::default()
        };
        o.on_speed_changed();
        assert!(!o.removed);
        assert!(o.checker.pending());
        assert!(o.wants_turn());
    }

    #[test]
    fn a_removed_basin_is_found_once() {
        let mut o = Operator::default();
        assert!(!o.wants_turn());
        assert!(!o.take_removed());
        o.removed = true;
        assert!(o.wants_turn());
        assert!(o.take_removed());
        assert!(!o.take_removed());
        assert!(!o.wants_turn());
    }
}
