//! `SequencedGearshiftBlockEntity`: the sequence its kinetic entity runs, following the speed
//! propagation gives it (`onSpeedChanged`) and taking its own kinetics apart as it goes (`run`).
use super::*;
use crate::sequencer::{Kind, Sequencer};

impl Kinetics {
    /// The sequenced gearshift's block entity at `pos` is created (placed or loaded).
    pub fn add_sequencer(&mut self, pos: [i32; 3], state: u8) {
        self.sequencers.entry(pos).or_insert_with(|| Sequencer {
            state,
            ..Sequencer::default()
        });
    }

    /// Its block entity is gone: so are the states it set that the host has not shown.
    pub fn remove_sequencer(&mut self, pos: [i32; 3]) {
        self.sequencers.remove(&pos);
        self.sequencer_writes.retain(|(p, _)| *p != pos);
    }

    /// The sequencers whose column unloaded go with it.
    pub fn retain_sequencers(&mut self, keep: impl Fn(&[i32; 3]) -> bool) {
        self.sequencers.retain(|p, _| keep(p));
        self.sequencer_writes.retain(|(p, _)| keep(p));
    }

    pub fn sequencer(&self, pos: [i32; 3]) -> Option<&Sequencer> {
        self.sequencers.get(&pos)
    }

    pub fn sequencer_mut(&mut self, pos: [i32; 3]) -> Option<&mut Sequencer> {
        self.sequencers.get_mut(&pos)
    }

    pub fn sequencer_positions(&self) -> Vec<[i32; 3]> {
        self.sequencers.keys().copied().collect()
    }

    /// Whether any sequencer has an instruction to count down this tick.
    pub fn sequencers_tick(&self) -> bool {
        self.sequencers.values().any(Sequencer::ticks)
    }

    /// `level.hasNeighborSignal(pos)` of the sequenced gearshift at `pos`.
    pub fn set_sequencer_signal(&mut self, pos: [i32; 3], signal: bool) {
        if let Some(s) = self.sequencers.get_mut(&pos) {
            s.signal = signal;
        }
    }

    /// The block states `run` and `onRedstoneUpdate` set, in order, for the host to show. The
    /// kinetic effect of each is already done.
    pub fn take_sequencer_writes(&mut self) -> Vec<([i32; 3], u8)> {
        std::mem::take(&mut self.sequencer_writes)
    }

    /// The block's state was changed by something else (placed, loaded, replaced).
    pub fn sequencer_state_changed(&mut self, pos: [i32; 3], state: u8) {
        if let Some(s) = self.sequencers.get_mut(&pos) {
            s.state = state;
        }
    }

    /// `SequencedGearshiftBlockEntity.read`: the fields alone, and the output face's modifier.
    pub fn restore_sequencer(&mut self, pos: [i32; 3], run: crate::sequencer::Run) {
        let Some(s) = self.sequencers.get_mut(&pos) else {
            return;
        };
        s.restore(run);
        if let Some(id) = self.at.get(&pos).map(|ids| ids[0]).filter(|&id| id != NONE) {
            self.nodes[id as usize].part = self.entity_part(pos, 0, self.nodes[id as usize].part);
        }
    }

    fn sequencer_node(&self, pos: [i32; 3]) -> Option<u32> {
        self.at
            .get(&pos)
            .map(|ids| ids[0])
            .filter(|&id| id != NONE && self.nodes[id as usize].live)
    }

    /// `getRotationSpeedModifier` reads the running instruction: the part of a sequenced
    /// gearshift turns its output face at the sequencer's modifier, whatever its block state.
    pub(super) fn entity_part(&self, pos: [i32; 3], sub: u8, part: Part) -> Part {
        match (self.sequencers.get(&pos), part.rotation) {
            (Some(s), ShaftRotation::Split(_)) if sub == 0 => Part {
                rotation: ShaftRotation::Split(s.modifier() as f32),
                ..part
            },
            _ => part,
        }
    }

    fn refresh_sequencer_part(&mut self, pos: [i32; 3], id: u32) {
        let part = self.entity_part(pos, 0, self.nodes[id as usize].part);
        if self.nodes[id as usize].part != part {
            self.nodes[id as usize].part = part;
            self.nodes[id as usize].fresh = true;
        }
    }

    /// `SequencedGearshiftBlockEntity.onSpeedChanged`, after the base callback.
    pub(super) fn sequencer_speed_changed(&mut self, id: u32, previous: f32) {
        let pos = self.nodes[id as usize].pos;
        let Some(s) = self.sequencers.get(&pos) else {
            return;
        };
        if s.is_idle() {
            return;
        }
        if previous.abs() == self.nodes[id as usize].state.theoretical.abs() {
            return;
        }
        let Some(instruction) = s.instruction(s.current) else {
            return;
        };
        if self.nodes[id as usize].state.speed == 0.0 {
            self.run_sequencer(pos, -1);
        }
        let theoretical = self.nodes[id as usize].state.theoretical;
        let s = self.sequencers.get_mut(&pos).unwrap();
        s.duration = instruction.duration(s.progress, theoretical);
        s.timer = 0;
    }

    /// `SequencedGearshiftBlockEntity.tick`, after its `super.tick()`.
    pub fn tick_sequencer(&mut self, pos: [i32; 3]) {
        let Some(id) = self.sequencer_node(pos) else {
            return;
        };
        let speed = self.nodes[id as usize].state.theoretical;
        let Some(s) = self.sequencers.get_mut(&pos) else {
            return;
        };
        if s.is_idle() || s.duration < 0 {
            return;
        }
        if s.timer < s.duration {
            s.timer += 1;
            if let Some(i) = s.instruction(s.current) {
                s.progress += i.tick_progress(speed);
            }
            return;
        }
        let next = s.current + 1;
        self.run_sequencer(pos, next);
    }

    /// `SequencedGearshiftBlockEntity.onRedstoneUpdate`, from the block's scheduled tick
    /// (`SequencedGearshiftBlock.tick`): `powered` is `hasNeighborSignal`, `running` whether the
    /// block's state was not 0.
    pub fn sequencer_redstone(&mut self, pos: [i32; 3], powered: bool, running: bool) {
        let Some(s) = self.sequencers.get_mut(&pos) else {
            return;
        };
        s.signal = powered;
        if !s.powered_before && powered {
            self.rising_flank(pos);
        }
        let s = self.sequencers.get_mut(&pos).unwrap();
        s.powered_before = powered;
        if !s.is_idle() || powered == running {
            return;
        }
        if !s.signal {
            self.write_sequencer_state(pos, 0);
            return;
        }
        if self
            .sequencer_node(pos)
            .is_none_or(|id| self.nodes[id as usize].state.speed == 0.0)
        {
            return;
        }
        self.run_sequencer(pos, 0);
    }

    /// `SequencedGearshiftBlockEntity.risingFlank`.
    fn rising_flank(&mut self, pos: [i32; 3]) {
        let s = self.sequencers.get_mut(&pos).unwrap();
        let Some(instruction) = s.instruction(s.current) else {
            return;
        };
        if s.powered_before {
            return;
        }
        s.powered_before = true;
        if instruction.kind == Kind::Await {
            let next = s.current + 1;
            self.run_sequencer(pos, next);
        }
    }

    /// `SequencedGearshiftBlockEntity.run`: start instruction `index`, or end the sequence (an
    /// `END`, or past the list).
    pub fn run_sequencer(&mut self, pos: [i32; 3], index: i32) {
        let Some(id) = self.sequencer_node(pos) else {
            return;
        };
        let Some(s) = self.sequencers.get(&pos) else {
            return;
        };
        let Some(instruction) = s.instruction(index).filter(|i| i.kind != Kind::End) else {
            if s.modifier() != 0 {
                self.detach_kinetics(pos);
            }
            let s = self.sequencers.get_mut(&pos).unwrap();
            s.current = -1;
            s.duration = -1;
            s.progress = 0.0;
            s.timer = 0;
            let signal = s.signal;
            self.refresh_sequencer_part(pos, id);
            self.set_sequence_context(pos, 0, None);
            if !signal {
                self.write_sequencer_state(pos, 0);
            } else {
                self.mark_changed(id);
            }
            return;
        };
        self.detach_kinetics(pos);
        let theoretical = self.nodes[id as usize].state.theoretical;
        let s = self.sequencers.get_mut(&pos).unwrap();
        s.duration = instruction.duration(0.0, theoretical);
        s.runs = s.runs.wrapping_add(1);
        s.current = index;
        s.progress = 0.0;
        // `SequenceContext.fromGearshift(instruction, getTheoreticalSpeed() * getModifier(),
        // value)`.
        let context = s.context().map(|c| {
            let speed = theoretical * s.modifier() as f32;
            (c, if speed == 0.0 { 0.0 } else { c.value / speed })
        });
        s.timer = 0;
        self.refresh_sequencer_part(pos, id);
        self.set_sequence_context(pos, 0, context);
        self.write_sequencer_state(pos, (index + 1).clamp(0, 5) as u8);
    }

    /// `level.setBlock(pos, state.setValue(STATE, value), UPDATE_ALL)`, for the host to make:
    /// an unchanged state is no change at all.
    fn write_sequencer_state(&mut self, pos: [i32; 3], value: u8) {
        let Some(s) = self.sequencers.get_mut(&pos) else {
            return;
        };
        if s.state == value {
            return;
        }
        s.state = value;
        self.sequencer_writes.push((pos, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::{Instruction, Speed};

    const MOTOR: [i32; 3] = [-1, 0, 0];
    const GEARSHIFT: [i32; 3] = [0, 0, 0];
    const OUTPUT: [i32; 3] = [1, 0, 0];
    const FASTER: f32 = 32.0;

    fn gearshift() -> Part {
        Part {
            speed_changed: SpeedChanged::Sequencer,
            context: SequenceContextSource::Owned,
            ..Part::split_shaft(Axis::X, 0.0)
        }
    }

    fn motor(rpm: f32) -> Part {
        Part {
            generated: rpm,
            capacity: 1024.0,
            ..Part::half(Dir::East)
        }
    }

    fn line(rpm: f32, instructions: Vec<Instruction>) -> Kinetics {
        let mut k = Kinetics::new();
        k.set(MOTOR, 0, motor(rpm));
        k.add_sequencer(GEARSHIFT, 0);
        k.sequencer_mut(GEARSHIFT).unwrap().instructions = instructions;
        k.set(GEARSHIFT, 0, gearshift());
        k.set(OUTPUT, 0, Part::shaft(Axis::X));
        k.solve(&mut Vec::new());
        k
    }

    /// The host shows the states written: the same block in another state, which is not
    /// kinetically equivalent.
    fn show(k: &mut Kinetics) -> Vec<([i32; 3], u8)> {
        let writes = k.take_sequencer_writes();
        for _ in &writes {
            k.change_block_state(GEARSHIFT, 0, gearshift(), false);
        }
        writes
    }

    fn tick(k: &mut Kinetics) {
        k.solve_with(&mut Vec::new(), true, |k, pos, _| k.tick_sequencer(pos));
    }

    fn speed(k: &Kinetics, pos: [i32; 3]) -> f32 {
        k.theoretical(pos, 0).unwrap().0
    }

    #[test]
    fn run_writes_its_state_and_the_entity_attaches_on_its_next_tick() {
        let mut k = line(16.0, vec![Instruction::new(Kind::TurnAngle, 90)]);
        assert_eq!(speed(&k, OUTPUT), 0.0, "idle: the output face is still");
        k.sequencer_redstone(GEARSHIFT, true, false);
        assert_eq!(show(&mut k), [(GEARSHIFT, 1)]);
        assert_eq!(k.sequencer(GEARSHIFT).unwrap().duration, 21);
        tick(&mut k);
        assert_eq!(speed(&k, GEARSHIFT), 16.0);
        assert_eq!(speed(&k, OUTPUT), 16.0);
        for _ in 0..20 {
            tick(&mut k);
        }
        assert!((k.sequencer(GEARSHIFT).unwrap().progress - 100.8).abs() < 1e-3);
        assert!(show(&mut k).is_empty());
        // Past the list while unpowered: state 0, its output face stops.
        k.set_sequencer_signal(GEARSHIFT, false);
        tick(&mut k);
        assert_eq!(show(&mut k), [(GEARSHIFT, 0)]);
        assert!(k.sequencer(GEARSHIFT).unwrap().is_idle());
        tick(&mut k);
        assert_eq!(speed(&k, GEARSHIFT), 16.0);
        assert_eq!(speed(&k, OUTPUT), 0.0);
    }

    #[test]
    fn a_powered_end_keeps_its_state_and_kinetics() {
        let mut k = line(32.0, vec![Instruction::new(Kind::TurnAngle, 45)]);
        k.sequencer_redstone(GEARSHIFT, true, false);
        for _ in 0..8 {
            tick(&mut k);
        }
        show(&mut k);
        tick(&mut k);
        assert!(k.sequencer(GEARSHIFT).unwrap().is_idle());
        assert!(show(&mut k).is_empty());
        // `detachKinetics` alone: its own speed and source stay, and it owes no attach.
        assert_eq!(speed(&k, GEARSHIFT), 32.0);
        assert_eq!(speed(&k, OUTPUT), 0.0);
        assert!(!k.saved_state(GEARSHIFT, 0).unwrap().needs_speed_update);
        // Unpowered while idle and running: state 0.
        k.sequencer_redstone(GEARSHIFT, false, true);
        assert_eq!(show(&mut k), [(GEARSHIFT, 0)]);
    }

    #[test]
    fn a_speed_change_inside_propagation_retimes_the_running_instruction() {
        let mut k = line(16.0, crate::sequencer::default_instructions());
        k.sequencer_redstone(GEARSHIFT, true, false);
        for _ in 0..10 {
            tick(&mut k);
        }
        // 48° done. A faster motor turns the output shaft: the propagation hands the gearshift
        // 32 RPM, and at that speed the 42° left take ceil(4.375) + 2.
        k.set(
            [2, 0, 0],
            0,
            Part {
                generated: FASTER,
                capacity: 1024.0,
                ..Part::half(Dir::West)
            },
        );
        let mut events = Vec::new();
        k.solve(&mut events);
        assert!(
            events.iter().all(|e| !matches!(e, Event::Break { .. })),
            "{events:?}"
        );
        assert_eq!(speed(&k, GEARSHIFT).abs(), 32.0);
        let s = k.sequencer(GEARSHIFT).unwrap();
        assert_eq!((s.current, s.duration, s.timer), (0, 7, 0));
    }

    #[test]
    fn a_speed_lost_inside_propagation_ends_the_sequence_at_once() {
        let mut k = line(16.0, crate::sequencer::default_instructions());
        k.sequencer_redstone(GEARSHIFT, true, false);
        show(&mut k);
        for _ in 0..10 {
            tick(&mut k);
        }
        // `applyNewSpeed` detaches first: the gearshift loses its source, `run(-1)` ends the
        // turn (unpowered: state 0), and the old turn is timed again against 0 RPM.
        k.set_sequencer_signal(GEARSHIFT, false);
        k.set(MOTOR, 0, motor(32.0));
        let s = k.sequencer(GEARSHIFT).unwrap();
        assert!(s.is_idle());
        assert_eq!(s.duration, i32::MIN + 1);
        assert_eq!(speed(&k, GEARSHIFT), 32.0);
        assert_eq!(speed(&k, OUTPUT), 0.0);
        assert_eq!(show(&mut k), [(GEARSHIFT, 0)]);
    }

    #[test]
    fn a_pulse_moves_past_an_await() {
        let mut k = line(
            32.0,
            vec![
                Instruction::new(Kind::Delay, 2),
                Instruction::new(Kind::Await, 1),
                Instruction {
                    speed: Speed::Back,
                    ..Instruction::new(Kind::TurnAngle, 45)
                },
            ],
        );
        k.sequencer_redstone(GEARSHIFT, true, false);
        for _ in 0..4 {
            tick(&mut k);
        }
        assert_eq!(k.sequencer(GEARSHIFT).unwrap().current, 1);
        assert!(!k.sequencers_tick(), "waiting costs nothing per tick");
        k.sequencer_redstone(GEARSHIFT, true, true);
        k.sequencer_redstone(GEARSHIFT, false, true);
        assert_eq!(k.sequencer(GEARSHIFT).unwrap().current, 1);
        k.sequencer_redstone(GEARSHIFT, true, true);
        let s = k.sequencer(GEARSHIFT).unwrap();
        assert_eq!((s.current, s.modifier(), s.duration), (2, -1, 7));
        assert_eq!(
            show(&mut k),
            [(GEARSHIFT, 1), (GEARSHIFT, 2), (GEARSHIFT, 3)]
        );
    }
}
