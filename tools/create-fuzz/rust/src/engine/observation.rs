use super::*;

impl Run {
    pub fn snapshot(&self, sc: &Scenario) -> Snapshot {
        let mut blocks = BTreeMap::new();
        let mut groups: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        let mut sizes = BTreeMap::new();
        let mut unspecified = Vec::new();
        for p in sc.region() {
            let id = self.at(p);
            let state = self.name(id);
            let kinetic = self.kinetic(p, id);
            if kinetic.is_some()
                && let Some((network, size)) = self.w.dims[0].kinetics.network_membership(p, 0)
            {
                groups.entry(network).or_default().push(key(p));
                sizes.insert(
                    key(p),
                    i32::try_from(size).expect("network size fits Java int"),
                );
                if self.unspecified_seeds.contains(&network) {
                    unspecified.push(key(p));
                }
            }
            blocks.insert(
                key(p),
                BlockSnap {
                    fluid_storage: self.fluid_storage(p),
                    fluid_machine: self.fluid_machine(p),
                    fluid_filter: self.w.dims[0]
                        .plumbing
                        .filters
                        .get(&p)
                        .map(|f| f.item.clone()),
                    fluid_network: self.fluid_network(p),
                    world_fluid: self.world_fluid_observation(p),
                    payload: self.saved_payload(p),
                    saved: self
                        .persisted
                        .kinetics
                        .get(&(p, 0))
                        .and_then(crate::persistence::codec::saved_from_nbt)
                        .map(Into::into),
                    fluid_tank: self.fluid_tank(p),
                    transport: self.transport_observation(p),
                    conveyor_links: self.chains.conveyors.get(&p).map(|c| {
                        let mut v = c.connections.clone();
                        v.sort();
                        v
                    }),
                    belt: self.belt_observation(p),
                    tunnel: self.tunnel_observation(p),
                    funnel: self.funnel_observation(p),
                    chute: self.chute_observation(p),
                    // Version 1 kinetic cases use ejectors as stress loads only.
                    ejector: (sc.version >= 3)
                        .then(|| self.ejector_observation(p))
                        .flatten(),
                    processing: (sc.version >= 3)
                        .then(|| self.processing_observation(p))
                        .flatten(),
                    measured_trig: self.measured_trig.get(&p).map(|property| {
                        crate::allow::MeasuredTrigState {
                            tie: Some(crate::allow::NearestTie::Yaw45Pitch0),
                            java: state.clone(),
                            port: state.clone(),
                            properties: vec![property.clone()],
                        }
                    }),
                    bracket: self.brackets.get(&p).map(|b| {
                        format!(
                            "{}[axis_along_first={},facing={},type={}]",
                            b.block(),
                            b.along_first,
                            format!("{:?}", b.facing).to_lowercase(),
                            b.kind.name()
                        )
                    }),
                    state,
                    kinetic,
                    debug: None,
                },
            );
        }
        let mut partitions: Vec<_> = groups.into_values().collect();
        for group in &mut partitions {
            group.sort();
        }
        partitions.sort();
        unspecified.sort();
        Snapshot {
            origin: self.native_origin,
            save_phase: self.persisted.phase,
            item_outcomes: self.item_outcomes.clone(),
            drops: Some(self.drops.clone()),
            side: "rust".into(),
            unavailable: {
                let mut unavailable = Vec::new();
                if self.native_failed {
                    unavailable.push(crate::fluid_world::OUTSIDE.into());
                }
                unavailable
            },
            networks: Some(crate::observation::Networks {
                partitions,
                sizes,
                unspecified,
            }),
            blocks,
            notes: self.notes.clone(),
            checkpoints: BTreeMap::new(),
        }
    }

    pub(super) fn kinetic(&self, p: Pos, id: u32) -> Option<Kinetic> {
        let k = &self.w.dims[0].kinetics;
        let (block, info): (create_engine::Block, &StateInfo) = self.w.table.get(id)?;
        let motor_rpm = self
            .w
            .column_settings(0, p[0] >> 4, p[2] >> 4)
            .into_iter()
            .find(|(q, _)| *q == p)
            .map_or(16.0, |(_, rpm)| rpm as f32);
        let settings = Settings {
            belt: self.w.dims[0].belt_of.get(&p).copied(),
            motor_rpm,
            steam: self.w.shaft_steam(0, p),
            flow: self.wheel_scores.get(&p).copied().unwrap_or(0),
            ..Settings::default()
        };
        let part = parts(block, info, &settings)?;
        let (theoretical, stress, capacity, overstressed) = k.network(p, 0)?;
        let (network_stress, network_capacity) = k.network_totals(p, 0)?;
        let speed = k.speed(p, 0)?;
        let generated = k.generated(p, 0)?;
        // A split shaft is one block entity in Create: each face turns at the speed its source
        // gives it (`getRotationSpeedModifier`).
        let faces = matches!(
            part.rotation,
            create_engine::kinetics::ShaftRotation::Split(_)
        )
        .then(|| {
            let mut m = BTreeMap::new();
            for d in create_engine::kinetics::Dir::ALL {
                if part.shafts & d.bit() != 0 {
                    m.insert(
                        format!("{d:?}").to_lowercase(),
                        k.face_speed(p, d).unwrap_or(0.0),
                    );
                }
            }
            m
        });
        Some(Kinetic {
            speed,
            theoretical,
            overstressed,
            stress,
            capacity,
            network_stress,
            network_capacity,
            impact: part.impact,
            added_capacity: part.capacity,
            generated,
            has_source: k.driven_elsewhere(p, 0),
            comparator: self.w.dims[0]
                .gauges
                .get(&p)
                .map(|g| create_engine::gauge::analog_output(g.shown.dial)),
            sequence_angle: k.sequence_limit(p, 0, false),
            sequence_distance: k.sequence_limit(p, 0, true),
            valve: self.handles.get(&p).map(|h| {
                let mut observed = h.observe(info.facing.is_some_and(|f| f.positive()));
                if h.remaining == 0 && k.driven_elsewhere(p, 0) && speed != 0. {
                    observed.angle = None;
                    observed.angle_unavailable =
                        Some("externally driven angle requires client renderer".into());
                }
                observed
            }),
            sequencer: (block == create_engine::Block::SequencedGearshift)
                .then(|| self.w.sequencer_run(0, p))
                .flatten()
                .map(|r| crate::observation::SequenceRun {
                    current: r.current,
                    duration: r.duration,
                    progress: r.progress,
                    timer: r.timer,
                    powered_before: r.powered_before,
                }),
            chain_modifier: (block == create_engine::Block::AdjustableChainGearshift).then(|| {
                create_engine::chain::modifier_for_signal(
                    self.w.saved_chain_signal(0, p).unwrap_or(0),
                )
            }),
            speed_setting: self.w.motor_setting(0, p),
            faces,
        })
    }
}
