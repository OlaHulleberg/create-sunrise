//! Create's KineticNetwork and TorquePropagator, owned by one dimension.
use super::Node;
use std::collections::{HashMap, HashSet};

/// Generation identity of Create's KineticBlockEntity within this dimension: a split shaft's
/// faces belong to the one entity.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Entity(u64);

/// One entry of `KineticNetwork.members` or `sources`: the node that is the block entity now,
/// and its base stress or capacity.
#[derive(Clone, Copy)]
struct Member {
    node: u32,
    value: f32,
}

/// Whether a member is still the block entity its position holds: Create prunes a retained
/// object the world has replaced (`be.getLevel().getBlockEntity(be.getBlockPos()) != be`).
fn present(member: &Member, entity: &Entity, nodes: &[Node]) -> bool {
    let n = &nodes[member.node as usize];
    n.live && n.entity == entity.0
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

    fn is_member(&self, id: u32, nodes: &[Node]) -> bool {
        self.members
            .contains_key(&Entity(nodes[id as usize].entity))
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
        if self.is_member(id, nodes) {
            self.add(id, nodes);
            return;
        }
        let n = &nodes[id as usize];
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
        if self.is_member(id, nodes) {
            return false;
        }
        let n = &mut nodes[id as usize];
        // KineticNetwork.add returns before updating sources or syncing an existing entity.
        n.calculate_stress_applied();
        if n.part.generated != 0.0 {
            let capacity = n.calculate_added_stress_capacity();
            self.sources.insert(
                Entity(n.entity),
                Member {
                    node: id,
                    value: capacity,
                },
            );
        }
        let value = n.part.impact;
        self.members
            .insert(Entity(n.entity), Member { node: id, value });
        true
    }
    /// KineticNetwork.remove; false for an entity the network does not hold.
    pub(super) fn remove(&mut self, id: u32, nodes: &[Node]) -> bool {
        let n = &nodes[id as usize];
        let entity = Entity(n.entity);
        if self.members.remove(&entity).is_none() {
            return false;
        }
        self.sources.remove(&entity);
        true
    }
    pub(super) fn update_capacity_for(&mut self, id: u32, capacity: f32, nodes: &[Node]) {
        self.sources.insert(
            Entity(nodes[id as usize].entity),
            Member {
                node: id,
                value: capacity,
            },
        );
    }
    pub(super) fn update_stress_for(&mut self, id: u32, stress: f32, nodes: &[Node]) {
        // KineticNetwork.updateStressFor: `members.put(be, stress)`, nothing else.
        self.members
            .entry(Entity(nodes[id as usize].entity))
            .or_insert(Member {
                node: id,
                value: stress,
            })
            .value = stress;
    }
    fn calculate_capacity(&mut self, nodes: &[Node]) -> f32 {
        // KineticNetwork.calculateCapacity prunes sources independently of members.
        self.sources
            .retain(|entity, source| present(source, entity, nodes));
        self.sources
            .values()
            .map(|source| source.value * nodes[source.node as usize].part.generated.abs())
            .sum::<f32>()
            + self.unloaded_capacity
    }
    fn calculate_stress(&mut self, nodes: &[Node]) -> f32 {
        self.members
            .retain(|entity, member| present(member, entity, nodes));
        self.members
            .values()
            .map(|member| member.value * nodes[member.node as usize].state.theoretical.abs())
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
                .filter(|(entity, member)| present(member, entity, nodes))
                .map(|(_, member)| member.node)
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
            && present(member, &entity, nodes)
        {
            self.dirty.insert((member.node, entity));
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
                n.members
                    .iter()
                    .filter(|(entity, member)| present(member, entity, nodes))
                    .map(|(_, member)| member.node)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinetics::{Axis, Kinetics, Part};

    #[test]
    fn init_from_te_includes_members_already_present() {
        // initFromTE calls updateStress and updateCapacity, rather than assigning
        // the saved totals over the contributions already in the network.
        let mut kinetics = Kinetics::new();
        kinetics.set(
            [1, 8, 0],
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
