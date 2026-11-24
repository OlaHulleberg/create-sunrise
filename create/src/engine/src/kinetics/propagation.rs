//! Create RotationPropagator and GeneratingKineticBlockEntity lifecycle.
use super::*;
impl Kinetics {
    /// RotationPropagator.getConveyedSpeed; controller connections depend on source.
    pub(super) fn get_conveyed_speed(&self, from: u32, to: u32) -> f32 {
        let a = &self.nodes[from as usize];
        let b = &self.nodes[to as usize];
        let controller = if controls(&a.part, &b.part) && b.pos == add(a.pos, [0, 1, 0]) {
            Some((a, b, false))
        } else if controls(&b.part, &a.part) && a.pos == add(b.pos, [0, 1, 0]) {
            Some((b, a, true))
        } else {
            None
        };
        if let Some((c, w, targeting)) = controller {
            let target = c.part.controller.expect("a speed controller");
            let desired = if target == 0.0 || (targeting && w.state.theoretical == 0.0) {
                0.0
            } else if c.state.source.is_none() {
                if targeting { target } else { 0.0 }
            } else if c.state.source == Some(w.pos) {
                if targeting {
                    target
                } else {
                    w.state.theoretical
                }
            } else if targeting {
                c.state.theoretical
            } else {
                target
            };
            let compare = if targeting {
                c.state.theoretical
            } else {
                w.state.theoretical
            };
            return if desired >= 0.0 && compare >= 0.0 {
                desired.max(compare)
            } else if desired < 0.0 && compare < 0.0 {
                desired.min(compare)
            } else {
                desired
            };
        }
        // RotationPropagator.getRotationSpeedModifier's axis connection: the far entity's
        // modifier inverted (0 stays 0), times the near one's.
        if let Some(d) = Dir::from_offset(sub(b.pos, a.pos))
            && a.part.shafts & d.bit() != 0
            && b.part.shafts & d.opposite().bit() != 0
        {
            let output = a.part.rotation.modifier(self.source_facing(a.pos), d);
            let input = b
                .part
                .rotation
                .modifier(self.source_facing(b.pos), d.opposite());
            let input = if input != 0.0 { 1.0 / input } else { input };
            return a.state.theoretical * (output * input);
        }
        // RotationPropagator.getConveyedSpeed evaluates the directed modifier
        // from the live entities; a reverse edge is not its rounded reciprocal.
        let modifier = if self.far.get(&a.pos).is_some_and(|ps| ps.contains(&b.pos)) {
            1.0
        } else {
            super::ratio(&a.part, &b.part, sub(b.pos, a.pos))
        };
        a.state.theoretical * modifier
    }

    /// KineticBlockEntity.onSpeedChanged and setSpeed.
    pub(super) fn change_speed(&mut self, id: u32, speed: f32) {
        let n = &mut self.nodes[id as usize];
        let shown = if n.state.overstressed { 0.0 } else { speed };
        n.state.flicker = n.state.flicker(self.now) + flicker_gain(n.state.speed, shown);
        n.state.flicker_at = self.now;
        let changed = n.state.theoretical != speed || n.state.speed != shown;
        n.fresh |= changed;
        n.state.theoretical = speed;
        n.state.speed = shown;
        if changed {
            self.speed_notifications.push((n.pos, shown));
        }
        // KineticBlockEntity.setSpeed/onSpeedChanged do not enroll members.
        // switchToBlockState detaches while the old network id is still present.
        // Membership belongs to setNetwork/add and generator stress updates.
        if changed {
            self.mark_changed(id);
        }
        self.on_speed_changed(id);
    }

    /// StressGaugeBlockEntity.onSpeedChanged calls updateFromNetwork with its cached
    /// totals and the live network size. This also clears KineticBlockEntity.networkDirty.
    pub(super) fn on_speed_changed(&mut self, id: u32) {
        let n = &self.nodes[id as usize];
        if n.part.speed_changed == SpeedChanged::StressGauge
            && n.state.speed != 0.0
            && n.state.net.is_some()
        {
            let (_, _, size) = self.networks.figures(n.state.net);
            self.update_from_network(id, n.state.capacity, n.state.stress, size);
        }
    }

    /// KineticBlockEntity.removeSource; GeneratingKineticBlockEntity.removeSource.
    pub(super) fn remove_source(&mut self, id: u32) {
        let n = &mut self.nodes[id as usize];
        if n.state.source.is_some() && n.part.generated != 0.0 {
            n.state.reactivate_source = true;
            self.scheduler.reactivate_later(self.now, id, n.stamp);
        }
        let changed = n.state.seq.is_some() || n.state.overstressed;
        n.state.source = None;
        n.state.overstressed = false;
        self.change_speed(id, 0.0);
        self.nodes[id as usize].state.seq = None;
        self.set_network(id, None);
        if changed {
            self.mark_changed(id);
        }
    }

    /// KineticBlockEntity.setSource copies the network and sequence context.
    pub(super) fn set_source(&mut self, id: u32, source: u32) {
        if !self.nodes[source as usize].live {
            self.remove_source(id);
            return;
        }
        let pos = self.nodes[source as usize].pos;
        let net = self.nodes[source as usize].state.net;
        let seq = self.nodes[source as usize].state.seq;
        let speed = self.nodes[source as usize].state.speed;
        let n = &mut self.nodes[id as usize];
        if n.state.source != Some(pos) {
            self.saved_dirty.insert(n.pos);
            n.fresh = true;
        }
        n.state.source = Some(pos);
        let old_seq = n.state.seq;
        if matches!(n.part.context, super::SequenceContextSource::Inherited) {
            n.state.seq = seq;
        }
        if speed.abs() >= n.part.generated.abs() {
            n.state.reactivate_source = false;
        }
        if old_seq != n.state.seq {
            self.mark_changed(id);
        }
        self.set_network(id, net);
    }

    /// RotationPropagator.propagateMissingSource, breadth first, then first surviving source.
    pub(super) fn propagate_missing_source(&mut self, start: u32, events: &mut Vec<Event>) {
        let missing = self.nodes[start as usize].state.source;
        let mut frontier = std::collections::VecDeque::from([start]);
        let mut candidates = Vec::new();
        while let Some(id) = frontier.pop_front() {
            if !self.nodes[id as usize].live {
                continue;
            }
            self.remove_source(id);
            let pos = self.nodes[id as usize].pos;
            for m in self.get_connected_neighbours(id) {
                let n = &self.nodes[m as usize];
                if Some(n.pos) == missing || n.state.source.is_none() {
                    continue;
                }
                if n.state.source != Some(pos) {
                    candidates.push(m);
                    continue;
                }
                if n.part.generated != 0.0 {
                    candidates.push(m);
                }
                frontier.push_back(m);
            }
        }
        for id in candidates {
            let n = &self.nodes[id as usize];
            if n.state.source.is_some() || n.part.generated != 0.0 {
                self.propagate_new_source(id, events);
                break;
            }
        }
    }

    /// RotationPropagator.handleRemoved checks source references even across disconnected faces.
    pub(super) fn handle_removed(&mut self, id: u32, events: &mut Vec<Event>) {
        if self.nodes[id as usize].state.theoretical == 0.0 {
            return;
        }
        let n = &self.nodes[id as usize];
        let pos = n.pos;
        let mut potential = Vec::new();
        for o in neighbour_offsets(&n.part) {
            if let Some(&m) = self.at.get(&add(pos, o)) {
                potential.push(m);
            }
        }
        for p in self.far.get(&pos).into_iter().flatten() {
            if let Some(&m) = self.at.get(p) {
                potential.push(m);
            }
        }
        let dependents = potential
            .into_iter()
            .filter(|&m| {
                self.nodes[m as usize].live && self.nodes[m as usize].state.source == Some(pos)
            })
            .collect::<Vec<_>>();
        for m in dependents {
            self.propagate_missing_source(m, events);
        }
    }

    /// RotationPropagator.propagateNewSource. Explicit frames preserve Java's recursive order.
    pub(super) fn propagate_new_source(&mut self, start: u32, events: &mut Vec<Event>) {
        let mut stack = vec![(start, self.get_connected_neighbours(start), 0usize)];
        while let Some((id, neighbours, next)) = stack.last_mut() {
            if *next == neighbours.len() {
                stack.pop();
                continue;
            }
            let cur = *id;
            let m = neighbours[*next];
            *next += 1;
            let current = self.nodes[cur as usize].state.theoretical;
            let other = self.nodes[m as usize].state.theoretical;
            let new = self.get_conveyed_speed(cur, m);
            let opposite = self.get_conveyed_speed(m, cur);
            if new == 0.0 && opposite == 0.0 {
                continue;
            }
            if new.abs() > MAX_SPEED
                || opposite.abs() > MAX_SPEED
                || self.nodes[cur as usize].state.flicker(self.now) > MAX_FLICKER_SCORE
                || (new != 0.0 && other != 0.0 && new.signum() != other.signum())
            {
                let pos = self.nodes[cur as usize].pos;
                if self.nodes[cur as usize].live {
                    events.push(Event::Break { pos });
                    self.remove(pos);
                }
                stack.pop();
                continue;
            }
            if opposite.abs() > current.abs() {
                self.set_source(cur, m);
                let conveyed = self.get_conveyed_speed(m, cur);
                self.change_speed(cur, conveyed);
                *stack.last_mut().unwrap() = (cur, self.get_connected_neighbours(cur), 0);
                continue;
            }
            if new.abs() >= other.abs() {
                if self.nodes[cur as usize].state.net.is_none()
                    || self.nodes[cur as usize].state.net == self.nodes[m as usize].state.net
                {
                    if new.abs() > other.abs() + other.abs() / 256.0 / 256.0 {
                        let pos = self.nodes[cur as usize].pos;
                        if self.nodes[cur as usize].live {
                            events.push(Event::Break { pos });
                            self.remove(pos);
                        }
                    }
                    continue;
                }
                if self.nodes[cur as usize].state.source == Some(self.nodes[m as usize].pos) {
                    self.remove_source(cur);
                }
            } else if (other - new).abs() <= 1e-4 {
                continue;
            }
            if new.abs() >= other.abs() {
                self.set_source(m, cur);
                let conveyed = self.get_conveyed_speed(cur, m);
                self.change_speed(m, conveyed);
            } else {
                self.change_speed(m, new);
                self.set_source(m, cur);
            }
            stack.push((m, self.get_connected_neighbours(m), 0));
        }
    }

    /// GeneratingKineticBlockEntity.updateGeneratedRotation / applyNewSpeed.
    pub(super) fn update_generated_rotation(&mut self, id: u32, events: &mut Vec<Event>) {
        let n = &self.nodes[id as usize];
        let (prev, generated, source) = (n.state.theoretical, n.part.generated, n.state.source);
        if prev != generated {
            self.apply_new_speed(id, prev, generated, source, events);
        }
        let n = &self.nodes[id as usize];
        // destroyBlock does not unwind updateGeneratedRotation: the retained
        // generator still updates its network after applyNewSpeed destroys it.
        if n.state.net.is_some() {
            let net = n.state.net;
            let capacity = if generated == 0.0 {
                0.0
            } else {
                n.part.capacity
            };
            let impact = n.part.impact;
            // GeneratingKineticBlockEntity.updateGeneratedRotation calculates these
            // contributions before passing them to the network.
            if generated != 0.0 {
                self.nodes[id as usize].calculate_added_stress_capacity();
            }
            self.nodes[id as usize].calculate_stress_applied();
            let network = self.networks.get_or_create_network_for(net);
            network.update_capacity_for(id, capacity, &self.nodes);
            let sync = network.update_capacity(&self.nodes);
            self.sync_network(sync);
            let network = self.networks.get_or_create_network_for(net);
            network.update_stress_for(id, impact, &self.nodes);
            let sync = network.update_stress(&self.nodes);
            self.sync_network(sync);
            // updateGeneratedRotation updates the figures synchronously. It does
            // not set networkDirty: later addSilently calls must not trigger a sync.
        }
    }

    /// GeneratingKineticBlockEntity.applyNewSpeed: stop, start, overpower, or reapply.
    fn apply_new_speed(
        &mut self,
        id: u32,
        prev: f32,
        generated: f32,
        source: Option<[i32; 3]>,
        events: &mut Vec<Event>,
    ) {
        if generated == 0.0 {
            if source.is_some() {
                return;
            }
            self.handle_removed(id, events);
            self.change_speed(id, 0.0);
            self.set_network(id, None);
        } else if prev == 0.0 {
            self.change_speed(id, generated);
            self.set_network(id, Some(network_id(self.nodes[id as usize].pos)));
            self.propagate_new_source(id, events);
        } else if source.is_some() && prev.abs() >= generated.abs() {
            if prev.signum() != generated.signum() {
                let pos = self.nodes[id as usize].pos;
                events.push(Event::Break { pos });
                self.remove(pos);
            }
        } else {
            self.handle_removed(id, events);
            self.change_speed(id, generated);
            // applyNewSpeed only creates an identity when overpowering a source.
            // Reapplying an independent generator retains its current network.
            if source.is_some() {
                self.nodes[id as usize].state.source = None;
                self.set_network(id, Some(network_id(self.nodes[id as usize].pos)));
            }
            self.propagate_new_source(id, events);
        }
    }
}
