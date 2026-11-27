//! Create's block breakers (`BlockBreakingKineticBlockEntity`): the mechanical drill and the saw
//! facing sideways turn, and break the block in front in steps that come faster at speed and
//! slower in hard blocks.

/// `BlockBreakingKineticBlockEntity`'s fields: `ticksUntilNextProgress`, which starts at 0 so
/// that a breaker looks on its first tick, `destroyProgress` and `breakingPos`; on its block
/// entity's `SmartBlockEntity` lazy tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Breaker {
    pub ticks_until: i32,
    pub progress: i32,
    pub breaking_pos: Option<[i32; 3]>,
    /// `SmartBlockEntity`'s lazy tick (the default rate, 10).
    pub lazy: crate::block_entity::SmartBlockEntity,
}

/// What a look at the block did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Wait,
    /// The cracks reached this stage (0..10).
    Progress(i32),
    /// The block breaks now.
    Broken,
    /// Nothing breakable there: the cracks go.
    Reset,
}

impl Breaker {
    /// `destroyNextTick`.
    pub fn destroy_next_tick(&mut self) {
        self.ticks_until = 1;
    }

    /// `SawBlockEntity.tick` before `super.tick()`: a saw facing sideways that rests looks next
    /// tick.
    pub fn rearm(&mut self) {
        if self.ticks_until < 0 {
            self.destroy_next_tick();
        }
    }

    /// `tick` up to the block: `SmartBlockEntity.tick`, whose `lazyTick` arms a resting breaker,
    /// then `getSpeed() == 0` and `ticksUntilNextProgress`. Whether the breaker looks at the block
    /// at `breaking_pos` now.
    pub fn tick(&mut self, speed: f32, breaking_pos: [i32; 3]) -> bool {
        if self.lazy.tick() && self.ticks_until == -1 {
            self.destroy_next_tick();
        }
        if speed == 0.0 {
            return false;
        }
        self.breaking_pos = Some(breaking_pos);
        if self.ticks_until < 0 {
            return false;
        }
        let looks = self.ticks_until == 0;
        self.ticks_until -= 1;
        looks
    }

    /// The rest of `tick`: the block at `breakingPos`, its `getDestroySpeed` or None for what
    /// `canBreak` refuses. `Broken` leaves the breaker to [`rest`](Self::rest) once its block is
    /// gone.
    pub fn look(&mut self, speed: f32, hardness: Option<f32>) -> Step {
        let Some(hardness) = hardness else {
            return if self.progress != 0 {
                self.progress = 0;
                Step::Reset
            } else {
                Step::Wait
            };
        };
        let break_speed = (speed / 100.0).abs();
        self.progress += ((break_speed / hardness) as i32).clamp(1, 10 - self.progress);
        if self.progress >= 10 {
            return Step::Broken;
        }
        self.ticks_until = (hardness / break_speed) as i32;
        Step::Progress(self.progress)
    }

    /// After `onBlockBroken`: `destroyProgress = 0; ticksUntilNextProgress = -1`.
    pub fn rest(&mut self) {
        self.progress = 0;
        self.ticks_until = -1;
    }

    /// `invalidate`'s: `breakingPos` while the breaker has progress.
    pub fn cracks_at(&self) -> Option<[i32; 3]> {
        (self.progress != 0).then_some(self.breaking_pos).flatten()
    }

    /// `read`: `Progress`, `NextTick` and `Breaking` as they were saved.
    pub fn read(&mut self, progress: i32, next_tick: i32, breaking_pos: Option<[i32; 3]>) {
        self.progress = progress;
        self.ticks_until = next_tick;
        self.breaking_pos = breaking_pos;
    }
}

/// `isBreakable`'s hardness test: unbreakable (-1) blocks stay.
pub fn breakable(hardness: f32) -> Option<f32> {
    (hardness >= 0.0).then_some(hardness)
}

/// `DrillBlock.getDamage`: what a turning drill or saw deals an entity inside it, 1 to 10 by
/// speed.
pub fn damage(speed: f32) -> f32 {
    let s = speed.abs();
    let sum = (s / 16.0).min(2.0) + (s / 32.0).min(4.0) + (s / 64.0).min(4.0);
    sum.clamp(1.0, 10.0)
}

/// `entityInside`'s test: the entity's box `[min, max]` meets the block at `pos` shrunk by 0.1
/// on every side (`new AABB(pos).deflate(.1f)`).
pub fn inside(pos: [i32; 3], min: [f64; 3], max: [f64; 3]) -> bool {
    (0..3).all(|a| {
        let lo = f64::from(pos[a]) + 0.1;
        let hi = f64::from(pos[a]) + 0.9;
        min[a] < hi && max[a] > lo
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STONE: Option<f32> = Some(1.5);
    const AT: [i32; 3] = [2, 8, 0];

    #[test]
    fn blades_hurt_by_speed_within_their_block() {
        assert_eq!(damage(0.0), 1.0);
        assert_eq!(damage(16.0), 1.75);
        assert_eq!(damage(-64.0), 2.0 + 2.0 + 1.0);
        assert_eq!(damage(256.0), 10.0);
        // A mob standing on an upright saw (12 px tall) is inside; one on a full block above is not.
        assert!(inside([0, 0, 0], [0.2, 0.75, 0.2], [0.8, 2.5, 0.8]));
        assert!(!inside([0, 0, 0], [0.2, 0.95, 0.2], [0.8, 2.5, 0.8]));
    }

    #[test]
    fn the_lazy_tick_runs_at_initialize_then_every_eleven_ticks() {
        let mut b = Breaker::default();
        let runs: Vec<i32> = (1..=33).filter(|_| b.lazy.tick()).collect();
        assert_eq!(runs, [1, 11, 22, 33]);
    }

    #[test]
    fn stone_breaks_in_steps_at_speed() {
        // 64 rpm: break speed 0.64; stone (hardness 1.5): one step every 2 ticks, 10 steps.
        let mut b = Breaker::default();
        let mut looks = Vec::new();
        let mut step = Step::Wait;
        for tick in 1..=28 {
            if b.tick(64.0, AT) {
                looks.push(tick);
                step = b.look(64.0, STONE);
            }
        }
        assert_eq!(looks, [1, 4, 7, 10, 13, 16, 19, 22, 25, 28]);
        assert_eq!(step, Step::Broken);
        assert_eq!((b.progress, b.breaking_pos), (10, Some(AT)));
        b.rest();
        assert_eq!((b.progress, b.ticks_until), (0, -1));
        // Dirt (0.5) at 256 rpm: 2.56 / 0.5 = 5 steps at once.
        let mut d = Breaker::default();
        assert!(d.tick(256.0, AT));
        assert_eq!(d.look(256.0, Some(0.5)), Step::Progress(5));
        assert_eq!(breakable(-1.0), None);
    }

    #[test]
    fn a_breaker_with_nothing_to_break_looks_again_two_ticks_after_each_lazy_tick() {
        let mut b = Breaker::default();
        let looks: Vec<i32> = (1..=35)
            .filter(|_| b.tick(64.0, AT) && b.look(64.0, None) == Step::Wait)
            .collect();
        assert_eq!(looks, [1, 12, 23, 34]);
        assert_eq!(b.breaking_pos, Some(AT));
    }

    #[test]
    fn a_stopped_breaker_counts_nothing() {
        let mut b = Breaker::default();
        assert!(!b.tick(0.0, AT));
        assert_eq!((b.ticks_until, b.breaking_pos), (0, None));
    }

    #[test]
    fn a_drill_told_to_look_does_so_two_ticks_later() {
        let mut b = Breaker::default();
        b.destroy_next_tick();
        assert!(!b.tick(64.0, AT));
        assert_eq!(b.ticks_until, 0);
        assert!(b.tick(64.0, AT));
        assert_eq!(b.look(64.0, STONE), Step::Progress(1));
    }

    #[test]
    fn an_unbreakable_block_takes_the_cracks() {
        let mut b = Breaker::default();
        assert!(b.tick(64.0, AT));
        assert_eq!(b.look(64.0, STONE), Step::Progress(1));
        while b.ticks_until > 0 {
            assert!(!b.tick(64.0, AT));
        }
        assert!(b.tick(64.0, AT));
        assert_eq!(b.look(64.0, None), Step::Reset);
        assert_eq!(b.progress, 0);
        assert_eq!(b.look(64.0, None), Step::Wait);
    }

    #[test]
    fn a_reloaded_breaker_goes_on_from_what_it_read() {
        let mut b = Breaker::default();
        assert!(b.tick(64.0, AT));
        assert_eq!(b.look(64.0, STONE), Step::Progress(1));
        assert_eq!(
            (b.progress, b.ticks_until, b.breaking_pos),
            (1, 2, Some(AT))
        );
        let mut reloaded = Breaker::default();
        reloaded.read(1, 607, Some(AT));
        assert!(!reloaded.tick(64.0, AT));
        assert_eq!(reloaded.ticks_until, 606);
        let mut resting = Breaker::default();
        resting.read(0, -1, Some(AT));
        assert!(!resting.tick(64.0, AT));
        assert_eq!(resting.ticks_until, 0);
    }

    #[test]
    fn a_saw_that_rests_looks_again_two_ticks_later() {
        let mut b = Breaker::default();
        let looks: Vec<i32> = (1..=7)
            .filter(|_| {
                b.rearm();
                b.tick(64.0, AT) && b.look(64.0, None) == Step::Wait
            })
            .collect();
        assert_eq!(looks, [1, 3, 5, 7]);
        let mut b = Breaker::default();
        b.destroy_next_tick();
        b.rearm();
        assert_eq!(b.ticks_until, 1);
    }
}
