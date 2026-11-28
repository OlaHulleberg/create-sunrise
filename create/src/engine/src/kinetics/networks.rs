//! Create's KineticNetwork and TorquePropagator, owned by one dimension.
use super::Node;
use std::collections::{HashMap, HashSet};

/// Generation identity of Create's KineticBlockEntity within this dimension. Shaft faces are ports,
/// not separate entities (`SplitShaftBlockEntity.getRotationSpeedModifier`).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Entity(u64);

/// One Java network member, with all engine ports that must receive its figures.
#[derive(Clone)]
struct Member {
    ports: HashSet<u32>,
    stress: f32,
}

/// Whether a port is still the world's block entity of `entity`: the test the Java maps prune by.
fn live(nodes: &[Node], entity: &Entity, id: u32) -> bool {
    let n = &nodes[id as usize];
    n.live && n.entity == entity.0
}

/// An entry's ports its map's pruning would keep.
fn live_ports<'a>(
    entity: &'a Entity,
    ports: &'a HashSet<u32>,
    nodes: &'a [Node],
) -> impl Iterator<Item = u32> + 'a {
    ports.iter().copied().filter(|&id| live(nodes, entity, id))
}

/// KineticNetwork stores base stress/capacity; actual contributions use current entity speeds.
#[derive(Default)]
pub(super) struct KineticNetwork {
    members: HashMap<Entity, Member>,
    sources: HashMap<Entity, Member>,
    stress: f32,
    capacity: f32,
    initialized: bool,
    unloaded_capacity: f32,
    unloaded_stress: f32,
    unloaded_members: u32,
}
impl KineticNetwork {
    #[cfg(test)]
    pub(super) fn unloaded_count(&self) -> u32 {
        self.unloaded_members
    }

    /// KineticNetwork.initFromTE accounts for members not yet loaded.
    pub(super) fn init_from_te(
        &mut self,
        capacity: f32,
        stress: f32,
        size: u32,
        nodes: &[Node],
    ) -> Vec<NetworkSync> {
        if self.initialized {
            return Vec::new();
        }
        self.initialized = true;
        self.unloaded_capacity = capacity;
        self.unloaded_stress = stress;
        self.unloaded_members = size;
        // initFromTE invokes updateStress then updateCapacity, including members
        // admitted before the first loaded entity initializes this network.
        let mut syncs = Vec::new();
        if let Some(sync) = self.update_stress(nodes) {
            syncs.push(sync);
        }
        if let Some(sync) = self.update_capacity(nodes) {
            syncs.push(sync);
        }
        syncs
    }
    /// KineticNetwork.addSilently subtracts the saved contribution before adding the live one.
    pub(super) fn add_silently(
        &mut self,
        id: u32,
        nodes: &mut [Node],
        last_capacity: f32,
        last_stress: f32,
    ) {
        let n = &nodes[id as usize];
        if self.members.contains_key(&Entity(n.entity)) {
            self.add(id, nodes);
            return;
        }
        if n.part.generated != 0.0 {
            self.unloaded_capacity =
                (self.unloaded_capacity - last_capacity * n.part.generated.abs()).max(0.0);
        }
        self.unloaded_stress =
            (self.unloaded_stress - last_stress * n.state.theoretical.abs()).max(0.0);
        self.unloaded_members = self.unloaded_members.saturating_sub(1);
        self.add(id, nodes);
    }
    /// KineticNetwork.add enrolls an entity; only a new member receives the initial
    /// updateFromNetwork and networkDirty transition (returned to the caller).
    pub(super) fn add(&mut self, id: u32, nodes: &mut [Node]) -> bool {
        self.initialized = true;
        let n = &mut nodes[id as usize];
        let enrolled = self.members.contains_key(&Entity(n.entity));
        self.members
            .entry(Entity(n.entity))
            .or_insert_with(|| Member {
                ports: HashSet::new(),
                stress: n.part.impact,
            })
            .ports
            .insert(id);
        // KineticNetwork.add returns before updating sources or syncing an existing entity.
        if enrolled {
            return false;
        }
        n.calculate_stress_applied();
        if n.part.generated != 0.0 {
            n.calculate_added_stress_capacity();
            self.sources.insert(
                Entity(n.entity),
                Member {
                    ports: HashSet::from([id]),
                    stress: n.part.capacity,
                },
            );
        }
        true
    }
    /// KineticNetwork.remove: detach a port; the entity leaves only when its last enrolled port
    /// leaves, and its capacity entry with it only while it is a source (`isSource`).
    pub(super) fn remove(&mut self, id: u32, nodes: &[Node]) -> bool {
        let mut removed = false;
        let source = nodes[id as usize].part.generated != 0.0;
        self.members.retain(|entity, member| {
            if entity.0 != nodes[id as usize].entity {
                return true;
            }
            removed |= member.ports.remove(&id);
            if member.ports.is_empty() {
                if source {
                    self.sources.remove(entity);
                }
                false
            } else {
                true
            }
        });
        removed
    }
    pub(super) fn update_capacity_for(&mut self, id: u32, capacity: f32, nodes: &[Node]) {
        self.sources.insert(
            Entity(nodes[id as usize].entity),
            Member {
                ports: HashSet::from([id]),
                stress: capacity,
            },
        );
    }
    pub(super) fn update_stress_for(&mut self, id: u32, stress: f32, nodes: &[Node]) {
        // KineticNetwork.updateStressFor: `members.put(be, stress)`, nothing else.
        let member = self
            .members
            .entry(Entity(nodes[id as usize].entity))
            .or_insert_with(|| Member {
                ports: HashSet::new(),
                stress,
            });
        member.ports.insert(id);
        member.stress = stress;
    }
    fn calculate_capacity(&mut self, nodes: &[Node]) -> f32 {
        // KineticNetwork.calculateCapacity prunes sources independently of members.
        self.sources.retain(|entity, source| {
            source.ports.retain(|&id| live(nodes, entity, id));
            !source.ports.is_empty()
        });
        self.capacity_now(nodes)
    }
    /// KineticNetwork.calculateCapacity, summed over the sources still in the level.
    fn capacity_now(&self, nodes: &[Node]) -> f32 {
        self.sources
            .iter()
            .map(|(entity, source)| {
                source.stress
                    * live_ports(entity, &source.ports, nodes)
                        .map(|id| nodes[id as usize].part.generated.abs())
                        .fold(0.0_f32, f32::max)
            })
            .sum::<f32>()
            + self.unloaded_capacity
    }
    fn calculate_stress(&mut self, nodes: &[Node]) -> f32 {
        // Java compares the world block entity with the retained object reference.
        self.members.retain(|entity, member| {
            member.ports.retain(|&id| live(nodes, entity, id));
            !member.ports.is_empty()
        });
        self.stress_now(nodes)
    }
    /// KineticNetwork.calculateStress, summed over the members still in the level.
    fn stress_now(&self, nodes: &[Node]) -> f32 {
        self.members
            .iter()
            .filter_map(|(entity, member)| {
                // A split shaft has one theoretical speed; face modifiers only affect
                // conveyed rotation. Prefer its externally driven port to its relay.
                let id = live_ports(entity, &member.ports, nodes).min_by_key(|&id| {
                    let n = &nodes[id as usize];
                    (
                        n.state.source.is_none_or(|source| source.pos == n.pos),
                        n.sub,
                    )
                })?;
                Some(member.stress * nodes[id as usize].state.theoretical.abs())
            })
            .sum::<f32>()
            + self.unloaded_stress
    }
    /// KineticNetwork.updateCapacity recalculates and syncs capacity changes immediately.
    pub(super) fn update_capacity(&mut self, nodes: &[Node]) -> Option<NetworkSync> {
        let capacity = self.calculate_capacity(nodes);
        if self.capacity == capacity {
            return None;
        }
        self.capacity = capacity;
        Some(self.sync(nodes))
    }
    /// KineticNetwork.updateStress recalculates and syncs stress changes immediately.
    pub(super) fn update_stress(&mut self, nodes: &[Node]) -> Option<NetworkSync> {
        let stress = self.calculate_stress(nodes);
        if self.stress == stress {
            return None;
        }
        self.stress = stress;
        Some(self.sync(nodes))
    }
    /// KineticNetwork.updateNetwork: calculate once and sync only when totals changed.
    pub(super) fn update_network(&mut self, nodes: &[Node]) -> Option<NetworkSync> {
        let stress = self.calculate_stress(nodes);
        let capacity = self.calculate_capacity(nodes);
        if stress == self.stress && capacity == self.capacity {
            return None;
        }
        self.stress = stress;
        self.capacity = capacity;
        Some(self.sync(nodes))
    }
    /// KineticNetwork.sync enumerates only this network's members.
    fn sync(&self, nodes: &[Node]) -> NetworkSync {
        NetworkSync {
            members: self
                .members
                .iter()
                .flat_map(|(entity, m)| {
                    m.ports.iter().copied().filter(|&id| {
                        nodes[id as usize].live && nodes[id as usize].entity == entity.0
                    })
                })
                .collect(),
            stress: self.stress,
            capacity: self.capacity,
            size: self.members.len() as u32 + self.unloaded_members,
        }
    }
}
/// Values sent to each member's KineticBlockEntity.updateFromNetwork.
pub(super) struct NetworkSync {
    pub members: Vec<u32>,
    pub stress: f32,
    pub capacity: f32,
    pub size: u32,
}
/// TorquePropagator's dimension registry; networkDirty belongs to each entity.
#[derive(Default)]
pub(super) struct TorquePropagator {
    networks: HashMap<i64, KineticNetwork>,
    /// KineticBlockEntity.networkDirty membership, keyed by retained entity identity.
    dirty: HashSet<(u32, Entity)>,
}
impl TorquePropagator {
    pub(super) fn get_or_create_network_for(&mut self, id: Option<i64>) -> &mut KineticNetwork {
        let id = id.expect("network identity");
        self.networks.entry(id).or_default()
    }
    /// KineticNetwork.add marks the entity after its initial updateFromNetwork.
    pub(super) fn mark_entity_dirty(&mut self, id: u32, nodes: &[Node]) {
        let n = &nodes[id as usize];
        if n.live {
            self.dirty.insert((id, Entity(n.entity)));
        }
    }

    /// KineticBlockEntity.updateFromNetwork and invalidation cancel pending membership.
    pub(super) fn clear_entity_dirty(&mut self, id: u32, nodes: &[Node]) {
        self.take_entity_dirty(id, nodes);
    }

    /// Only this entity's base tick consumes its flag; never drain an entire network.
    pub(super) fn take_entity_dirty(&mut self, id: u32, nodes: &[Node]) -> bool {
        self.dirty.remove(&(id, Entity(nodes[id as usize].entity)))
    }

    pub(super) fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    pub(super) fn mark_dirty(&mut self, net: Option<i64>, nodes: &mut [Node]) {
        if let Some((&entity, member)) = net
            .and_then(|n| self.networks.get(&n))
            .and_then(|n| n.members.iter().next())
        {
            for &id in &member.ports {
                let n = &nodes[id as usize];
                if n.live && n.entity == entity.0 {
                    self.dirty.insert((id, entity));
                }
            }
        }
    }

    pub(super) fn remove(&mut self, net: Option<i64>, id: u32, nodes: &mut [Node]) {
        let Some(key) = net else {
            return;
        };
        if let Some(n) = self.networks.get_mut(&key) {
            // KineticNetwork.remove is a no-op for a detached retained entity.
            if !n.remove(id, nodes) {
                return;
            }
            if n.members.is_empty() {
                self.networks.remove(&key);
            } else {
                self.mark_dirty(net, nodes);
            }
        }
    }
    pub(super) fn update_network(
        &mut self,
        id: Option<i64>,
        nodes: &[Node],
    ) -> Option<NetworkSync> {
        self.networks.get_mut(&id?)?.update_network(nodes)
    }
    pub(super) fn members<'a>(
        &'a self,
        id: Option<i64>,
        nodes: &'a [Node],
    ) -> impl Iterator<Item = u32> + 'a {
        id.and_then(|id| self.networks.get(&id))
            .into_iter()
            .flat_map(|n| {
                n.members.iter().flat_map(|(entity, m)| {
                    m.ports.iter().copied().filter(|&id| {
                        nodes[id as usize].live && nodes[id as usize].entity == entity.0
                    })
                })
            })
    }
    pub(super) fn figures(&self, id: Option<i64>) -> (f32, f32, u32) {
        id.and_then(|id| self.networks.get(&id))
            .map(|n| {
                (
                    n.stress,
                    n.capacity,
                    n.members.len() as u32 + n.unloaded_members,
                )
            })
            .unwrap_or_default()
    }
    /// A network's stress and capacity as its next recalculation sums them, read without pruning.
    pub(super) fn totals(&self, id: Option<i64>, nodes: &[Node]) -> (f32, f32) {
        id.and_then(|id| self.networks.get(&id))
            .map(|n| (n.stress_now(nodes), n.capacity_now(nodes)))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinetics::{Axis, Kinetics, Part};

    #[test]
    fn totals_sum_live_entries_without_pruning() {
        let mut kinetics = Kinetics::new();
        kinetics.set(
            [1, 8, 0],
            0,
            Part {
                generated: 16.0,
                capacity: 16384.0,
                impact: 8.0,
                ..Part::shaft(Axis::X)
            },
        );
        kinetics.set(
            [2, 8, 0],
            0,
            Part {
                impact: 4.0,
                ..Part::shaft(Axis::X)
            },
        );
        for n in &mut kinetics.nodes {
            n.state.theoretical = 32.0;
        }
        let mut propagator = TorquePropagator::default();
        let network = propagator.get_or_create_network_for(Some(7));
        network.update_capacity_for(0, 16384.0, &kinetics.nodes);
        network.update_stress_for(0, 8.0, &kinetics.nodes);
        network.update_stress_for(1, 4.0, &kinetics.nodes);
        assert_eq!(
            propagator.totals(Some(7), &kinetics.nodes),
            (384.0, 262144.0)
        );
        kinetics.nodes[1].live = false;
        assert_eq!(
            propagator.totals(Some(7), &kinetics.nodes),
            (256.0, 262144.0)
        );
        assert_eq!(propagator.networks[&7].members.len(), 2);
        let sync = propagator
            .get_or_create_network_for(Some(7))
            .update_network(&kinetics.nodes)
            .unwrap();
        assert_eq!((sync.stress, sync.capacity), (256.0, 262144.0));
        assert_eq!(propagator.networks[&7].members.len(), 1);
        assert_eq!(
            propagator.totals(Some(7), &kinetics.nodes),
            (256.0, 262144.0)
        );
        assert_eq!(propagator.totals(Some(8), &kinetics.nodes), (0.0, 0.0));
    }

    #[test]
    fn a_member_keeps_its_capacity_entry_unless_it_leaves_as_a_source() {
        let generator = Part {
            generated: 16.0,
            capacity: 256.0,
            ..Part::shaft(Axis::X)
        };
        let mut kinetics = Kinetics::new();
        kinetics.set([1, 8, 0], 0, generator);
        kinetics.set([2, 8, 0], 0, Part::shaft(Axis::X));
        let mut network = KineticNetwork::default();
        network.add(0, &mut kinetics.nodes);
        network.add(1, &mut kinetics.nodes);
        kinetics.nodes[0].part.generated = 0.0;
        assert!(network.remove(0, &kinetics.nodes));
        assert!(
            network
                .sources
                .contains_key(&Entity(kinetics.nodes[0].entity))
        );
        kinetics.nodes[0].part.generated = 16.0;
        network.add(0, &mut kinetics.nodes);
        assert!(network.remove(0, &kinetics.nodes));
        assert!(network.sources.is_empty());
    }

    #[test]
    fn init_from_te_includes_members_already_present() {
        // initFromTE calls updateStress and updateCapacity, rather than assigning
        // the saved totals over the contributions already in the network.
        let mut kinetics = Kinetics::new();
        kinetics.set(
            [1, 8, 0],
            0,
            Part {
                impact: 8.0,
                ..Part::shaft(Axis::X)
            },
        );
        kinetics.nodes[0].state.theoretical = 32.0;
        let mut network = KineticNetwork::default();
        network.update_stress_for(0, 8.0, &kinetics.nodes);
        let syncs = network.init_from_te(524288.0, 0.0, 1, &kinetics.nodes);
        assert_eq!(network.stress, 256.0);
        let sync = syncs.last().unwrap();
        assert_eq!(
            (sync.capacity, sync.stress, sync.size),
            (524288.0, 256.0, 2)
        );
    }
}
