use super::*;

impl Run {
    /// One server tick: the level's, then the harness's disposal of item entities. What the
    /// driver ticks follows the level's entity pass.
    pub(super) fn tick(&mut self) {
        self.entity_pass_done = true;
        self.level_tick();
        self.flush_block_drops();
        self.dispose_item_entities();
    }

    /// One level tick: the game's block ticks and entities, then inputs, the solve and what it
    /// breaks.
    fn level_tick(&mut self) {
        self.now += 1;
        self.persisted.clock += 1;
        // The scheduled block ticks run before the entities, those before the block entities.
        self.vanilla_ticks();
        self.vanilla_entities();
        // Shape notifications from committed inputs precede the level tick.
        self.tick_pipe_shapes();
        self.tick_structures();
        self.tick_large_wheels();
        self.w.dims[0].fluid_transport.begin_tick();
        self.tick_world_fluids();
        if self.native_failed {
            return;
        }
        self.apply_wheel_flow();
        self.refresh_sequencer_signals();
        let mut events = Vec::new();
        // Clutches and gearshifts follow redstone (`ClutchBlock.neighborChanged`): the
        // callback runs with the block change, before any block entity of this tick.
        for p in self.w.take_redstone(0) {
            let state = self.at(p);
            let Some(was) = self.w.powered(state) else {
                continue;
            };
            let now = self.signal(p) > 0;
            if now != was {
                let next = with_prop(
                    &self.name(state),
                    "powered",
                    if now { "true" } else { "false" },
                );
                let id = self.id(&next);
                // The glue's `tick_inputs`: GearshiftBlock.neighborChanged detaches first.
                self.w.transmission_power_changing(0, p);
                // ChainGearshiftBlock.neighborChanged sets with UPDATE_KNOWN_SHAPE.
                if self.w.reads_signal(state) {
                    self.commit_known_shape(p, id);
                } else {
                    self.commit(p, id, false);
                }
            }
            if self.w.reads_signal(state) {
                let signal = i32::from(self.signal(p));
                self.w.set_chain_signal(0, p, signal);
            }
        }
        // The scheduled block ticks the detach owes (`GearshiftBlock.tick`): the level runs
        // them before block entities tick, so the transmission re-attaches before any base
        // tick of this tick.
        self.w.reattach_scheduled_transmissions(0, &mut events);
        self.publish_after_effects(events);
        self.w.flush_kinetic_callbacks(0);
        self.show_sequencer_writes();
        self.w.tick_cranks(0);
        for h in self.handles.values_mut() {
            h.remaining = h.remaining.saturating_sub(1);
        }
        self.apply_chain_shapes();
        for (p, running) in self.w.take_sequencer_checks(0) {
            let powered = self.signal(p) > 0;
            self.w.sequencer_redstone(0, p, powered, running);
        }
        self.show_sequencer_writes();
        let mut events = Vec::new();
        // `Processing::level_tick`: crafters, millstones, fans, crushers, burners, the
        // leftovers of gone basins (`spoutput::forget_gone`). The basins (lazy tick, spout, backup
        // check), the mixers and the presses over basins take their turns in the transport walk.
        self.tick_millstones();
        self.tick_fans();
        self.tick_crushers();
        self.tick_tank_layers();
        self.tick_fluids();
        self.tick_boilers();
        self.tick_burners();
        self.forget_gone_basins();
        // Ejector redstone before the walk ticks them (Java's `neighborChanged` runs at the
        // block change).
        self.ejector_signals();
        self.tick_transport();
        // `processing::Tools`: the drills and saws, then the depots the deployers hold
        // (`tick_presses`), then the deployers' strokes.
        self.tick_drills();
        self.tick_presses();
        self.tick_deployers();
        self.ejectors_ticked();
        self.save_fluid_entities();
        // CreateWorld::tick: the solve, then the states its sequencers set.
        self.refresh_sequencer_signals();
        let (_, writes) = self.w.tick(0, &mut events);
        self.persisted.dirty_columns.extend(self.w.take_unsaved(0));
        for (p, state) in writes {
            self.sequencer_state(p, state);
        }
        self.w.dims[0].kinetics.take_removed();
        self.mark_package_columns();
        self.publish_after_effects(events);
    }

    /// Glue applies neighbour shapes in Inputs and immediately after each kinetic break.
    pub(super) fn apply_chain_shapes(&mut self) {
        for (p, drive) in self.w.take_chain_shapes(0) {
            let next = with_prop(
                &self.name(self.at(p)),
                "axis_along_first",
                if drive.along_first { "true" } else { "false" },
            );
            self.set(p, self.id(&next));
            let next = with_prop(&self.name(self.at(p)), "part", drive.part.name());
            self.set(p, self.id(&next));
        }
    }

    /// The glue's `refresh_sequencer_signals`: each gearshift's `hasNeighborSignal`.
    pub(super) fn refresh_sequencer_signals(&mut self) {
        for p in self.w.sequencer_positions(0) {
            let signal = self.signal(p) > 0;
            self.w.set_sequencer_signal(0, p, signal);
        }
    }

    /// The glue's `apply_sequencer_writes`: show the states the engine's sequencers set.
    pub(super) fn show_sequencer_writes(&mut self) {
        for (p, state) in self.w.take_sequencer_writes(0) {
            self.sequencer_state(p, state);
        }
    }

    /// The glue's `show_sequencer_states`: a sequencer's state write, its kinetic effect done.
    pub(super) fn sequencer_state(&mut self, p: [i32; 3], state: u8) {
        self.save_sequence(p);
        let next = with_prop(&self.name(self.at(p)), "state", &state.to_string());
        self.set(p, self.id(&next));
    }
}
