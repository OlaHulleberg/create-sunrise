//! The one fake server the trains' tests share. Its container, item and attachment journal is
//! the SDK fake's; this adapter supplies the level's block reads and writes, and, for the cart
//! assembly tests, the carts and the rider's lifecycle.

use crate::glue::{server::ServerLevel, states, world::with_world};
use create_engine::world::CreateWorld;
use mod_api::__private::game::{BackendMutRefOf, BackendRefOf};
use mod_api::prelude::*;
use mod_api::testing::FakeHost;
use mod_api::{
    AttachmentStore, BackendMutRef, BackendRef, BlockCapabilitySite, BlockEntityRef, BlockKey,
    BlockSide, BlockState, BlockStateError, BlockStateRange, BlockUpdateFlags, EntityRef,
    EntitySpawn, EntityTypeKey, EntityUuid, Hooks, Host, Identifier, ItemKey, ItemStack,
    ItemStackView, LevelHost, LevelReadError, LevelRef, LevelWriteError, MinecartRef, ModEntry,
    PlayerRef, Registrar, RegistrationContext, RegistryName, RemovalReason, ServerHost, SpawnBody,
    StackWriteError, StartRidingRefusal, TransactionContext, TransactionScopeError,
};
use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

pub(super) const MIN_Y: i32 = -64;
pub(super) const HEIGHT: i32 = 384;

pub(super) const AIR: &str = "minecraft:air";
pub(super) const BARREL: &str = "minecraft:barrel";
pub(super) const STONE: &str = "minecraft:stone";
pub(super) const ASSEMBLER: &str = "create:cart_assembler";
const ANCHOR: &str = "create:minecart_anchor[axis=x]";
pub(super) const BELT: &str = "create:belt[facing=south,part=start,slope=horizontal]";
pub(super) const MINECART: &str = "minecraft:minecart";
const DIAMOND: &str = "minecraft:diamond";
const CART_STATES: [&str; 6] = [AIR, BARREL, STONE, ASSEMBLER, ANCHOR, BELT];

/// The mod's registration for the cart tests: Create's block states, the cart module's own
/// attachments (`attachments`) and the plain blocks the cart tests place.
struct CartRegistration {
    attachments: fn(&mut dyn Registrar),
}

#[mod_api::__subscriptions]
impl Hooks for CartRegistration {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        states::register(r);
        (self.attachments)(r);
        for name in [AIR, BARREL, STONE] {
            r.block_key(name);
        }
    }
}

/// A minecart the fake level holds: what the cart tests' own cart records are made from.
#[derive(Clone)]
pub(super) struct FakeCart {
    pub(super) entity: EntityRef,
    pub(super) uuid: u128,
    pub(super) pos: [f64; 3],
}

pub(super) struct TestServer {
    pub(super) fake: FakeHost,
    states: Vec<(&'static str, BlockState)>,
    /// What a position nothing was placed at reads as; `None`: no block there to read.
    fill: Option<BlockState>,
    pub(super) blocks: HashMap<[i32; 3], BlockState>,
    pub(super) unloaded: HashSet<[i32; 2]>,
    pub(super) refuse: HashSet<[i32; 3]>,
    pub(super) writes: Vec<([i32; 3], BlockState, BlockUpdateFlags)>,
    pub(super) glue_present_at_write: bool,
    pub(super) dropped: Vec<([f64; 3], u32)>,
    pub(super) carts: Vec<FakeCart>,
    pub(super) spawned: Vec<String>,
    pub(super) rider: Option<EntityRef>,
    pub(super) mounted: Option<(EntityRef, EntityRef)>,
    pub(super) removed: Vec<(EntityRef, RemovalReason)>,
    pub(super) teleports: Vec<EntityRef>,
}

/// Block/attachment registration in the mod is process-wide. Run one exact test per child so
/// those keys, the engine range, and the thread-local assemblies cannot leak into other tests.
pub(super) fn isolated_assembly(test: &str, run: impl FnOnce()) {
    const CHILD: &str = "CREATE_CART_ASSEMBLY_TEST_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(test) {
        run();
    } else {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test])
            .env(CHILD, test)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("running 1 test"),
            "isolated {test} failed ({}):\n{stdout}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

impl TestServer {
    fn blank(fake: FakeHost, states: Vec<(&'static str, BlockState)>) -> Self {
        Self {
            fake,
            states,
            fill: None,
            blocks: HashMap::new(),
            unloaded: HashSet::new(),
            refuse: HashSet::new(),
            writes: Vec::new(),
            glue_present_at_write: false,
            dropped: Vec::new(),
            carts: Vec::new(),
            spawned: Vec::new(),
            rider: None,
            mounted: None,
            removed: Vec::new(),
            teleports: Vec::new(),
        }
    }

    /// A server whose block states are `table`: a name and its ordinal each, all of one block.
    pub(super) fn with_states(table: &[(&'static str, u16)]) -> Self {
        let fake = FakeHost::new();
        let key = mod_api::LevelHost::block_state(&fake, [0, 0, 0])
            .unwrap()
            .block();
        let states = table
            .iter()
            .map(|&(n, ord)| (n, mod_api::__private::block_state_from_ordinal(key, ord)))
            .collect();
        Self::blank(fake, states)
    }

    /// A server with the mod registered (`attachments` registers the cart module's own), a
    /// minecart at each of `cart_positions`, and air wherever nothing is placed.
    pub(super) fn with_carts(
        cart_positions: &[[f64; 3]],
        attachments: fn(&mut dyn Registrar),
    ) -> (Self, CreateWorld) {
        mod_api::__private::part_tests::install_parts_fixture(&[ModEntry::new(
            "create",
            Box::new(CartRegistration { attachments }),
        )])
        .unwrap();
        let states = CART_STATES
            .into_iter()
            .map(|text| {
                let key = mod_api::__private::part_tests::block_key(states::split(text).0);
                (text, mod_api::__private::block_state_from_ordinal(key, 0))
            })
            .collect();
        let mut server = Self::blank(FakeHost::default(), states);
        server.fill = Some(server.state(AIR));
        let diamond = Identifier::parse(DIAMOND).unwrap();
        server.fake.registries.resolve_items([diamond]).unwrap();
        for (i, &pos) in cart_positions.iter().enumerate() {
            let id = (i + 1) as u32;
            server.fake.level.entity(id, MINECART);
            server.carts.push(FakeCart {
                entity: server.fake.entity(id).unwrap(),
                uuid: u128::from(id),
                pos,
            });
        }
        server.fake.resolve_entity_types().unwrap();
        let world = CreateWorld::new(states::open(&server).unwrap());
        (server, world)
    }

    pub(super) fn state(&self, name: &str) -> BlockState {
        self.block_state_id(name).unwrap()
    }

    pub(super) fn place(&mut self, pos: [i32; 3], name: &str) {
        self.blocks.insert(pos, self.state(name));
        self.fake.level.set_block(pos, states::split(name).0);
    }

    pub(super) fn barrel(&mut self, pos: [i32; 3], count: u32) -> ItemStack {
        self.place(pos, BARREL);
        let item = self.item_key(&Identifier::parse(DIAMOND).unwrap()).unwrap();
        let stack = ItemStack::new(item, count).unwrap();
        self.fake
            .level
            .block_entity(pos)
            .with_items(1)
            .items
            .as_mut()
            .unwrap()
            .slots[0] = Some(stack.clone());
        stack
    }

    pub(super) fn barrel_slot(&self, pos: [i32; 3]) -> Option<&ItemStack> {
        self.fake.level.block_entity_at(pos)?.items.as_ref()?.slots[0].as_ref()
    }

    pub(super) fn level(&mut self) -> ServerLevel<'_> {
        let level = self.fake.level.level();
        ServerLevel {
            host: self,
            level,
            dim: 0,
            dimension: "minecraft:overworld",
        }
    }
}

mod_api::__mod_api_impl_host_areas!(
    TestServer,
    except(block_entities, block_states, items, mod_state)
);
mod_api::__mod_api_impl_server_host_areas!(TestServer, except(minecarts));
mod_api::__mod_api_impl_backend_areas!(
    TestServer,
    except(
        block_states,
        entity_identity,
        entity_position,
        entity_riding,
        items,
        keys,
        level_blocks,
        level_chunks,
        level_dimension,
        level_entities
    )
);
mod_api::__mod_api_impl_backend_mut_areas!(
    TestServer,
    except(
        entity_identity,
        entity_lifetime,
        entity_position,
        entity_riding,
        level_blocks,
        level_entities,
        level_spawning
    )
);

impl Host for TestServer {
    fn backend(&self) -> Option<BackendRef<'_>> {
        Some(BackendRef::of(self))
    }
}

impl ServerHost for TestServer {
    fn backend_mut(&mut self) -> Option<BackendMutRef<'_>> {
        Some(BackendMutRef::of(self))
    }

    fn with_transaction_scope(
        &mut self,
        callback: &mut dyn for<'loan> FnMut(
            &'loan mut dyn ServerHost,
            &'loan TransactionContext,
        ) -> bool,
    ) -> Result<bool, TransactionScopeError> {
        // Assembly's transactions only move container slots or save rider attachments, both
        // owned by the SDK fake. Give the callback that host and its matching journal.
        self.fake.with_transaction_scope(callback)
    }
}

impl mod_api::Backend for TestServer {}
impl mod_api::BackendMut for TestServer {}

impl ModStateHost for TestServer {
    fn attachments(&mut self) -> &mut dyn AttachmentStore {
        self.fake.attachments()
    }
}

impl BlockStatesHost for TestServer {
    fn block_state_id(&self, name: &str) -> Option<BlockState> {
        self.states
            .iter()
            .find_map(|&(text, state)| (text == name).then_some(state))
    }

    fn block_state_string(&self, state: BlockState) -> Option<String> {
        self.states
            .iter()
            .find_map(|&(text, s)| (s == state).then(|| text.to_owned()))
    }

    fn block_state_default(&self, block: BlockKey) -> Option<BlockState> {
        self.states
            .iter()
            .find_map(|&(_, state)| (state.block() == block).then_some(state))
    }
}

impl BlockStatesBackend for TestServer {
    fn block_states_of(&self, block: BlockKey) -> Option<BlockStateRange> {
        BlockStateRange::new(self.block_state_default(block)?, 1).ok()
    }

    fn block_metadata_with(
        &self,
        state: BlockState,
        name: &str,
        value: &str,
    ) -> Result<BlockState, BlockStateError> {
        if self.block_state_id(ANCHOR) == Some(state) && (name, value) == ("axis", "x") {
            Ok(state)
        } else {
            Err(BlockStateError::UnknownState)
        }
    }
}

impl BlockEntitiesHost for TestServer {
    fn block_entity(&self, level: LevelRef, pos: [i32; 3]) -> Option<BlockEntityRef> {
        BlockEntitiesHost::block_entity(&self.fake, level, pos)
    }
}

impl ItemsHost for TestServer {
    fn item_key(&self, id: &Identifier) -> Option<ItemKey> {
        self.fake.item_key(id)
    }

    fn max_damage(&self, _: ItemKey) -> Option<u32> {
        None
    }

    fn built_creative_tabs(
        &self,
    ) -> Result<Option<std::sync::Arc<mod_api::CreativeModeTabs>>, mod_api::CreativeTabReadError>
    {
        Ok(None)
    }
}

impl KeysBackend for TestServer {
    fn item_key_id(&self, key: ItemKey) -> Option<&str> {
        self.fake.item_key_id(key)
    }

    fn entity_type_key_id(&self, key: EntityTypeKey) -> Option<RegistryName> {
        self.fake.entity_type_key_id(key)
    }
}

impl ItemsBackend for TestServer {
    fn container_size(&self, at: BlockSide) -> Option<usize> {
        self.fake.container_size(at)
    }

    fn lend_container_stack(
        &self,
        at: BlockSide,
        slot: usize,
        read: &mut dyn for<'a> FnMut(ItemStackView<'a>),
    ) {
        self.fake.lend_container_stack(at, slot, read);
    }

    fn creative_stack_key(&self, stack: &ItemStack) -> Option<ItemStack> {
        Some(stack.clone())
    }
}

impl LevelDimensionBackend for TestServer {
    fn level_height(&self, _: LevelRef) -> Option<(i32, i32)> {
        Some((MIN_Y, HEIGHT))
    }
}

impl LevelChunksBackend for TestServer {
    fn level_is_loaded(&self, _: LevelRef, pos: [i32; 3]) -> bool {
        (MIN_Y..MIN_Y + HEIGHT).contains(&pos[1])
            && !self.unloaded.contains(&[pos[0] >> 4, pos[2] >> 4])
    }
}

impl LevelBlocksBackend for TestServer {
    fn level_block_state(&self, _: LevelRef, pos: [i32; 3]) -> Option<BlockState> {
        self.blocks.get(&pos).copied().or(self.fill)
    }

    fn level_with_capability_site(
        &self,
        at: BlockSide,
        visit: &mut dyn FnMut(&dyn LevelHost, BlockCapabilitySite),
    ) -> Result<(), LevelReadError> {
        self.fake.level_with_capability_site(at, visit)
    }
}

impl LevelBlocksBackendMut for TestServer {
    fn level_set_block(
        &mut self,
        _: LevelRef,
        pos: [i32; 3],
        state: BlockState,
        flags: BlockUpdateFlags,
    ) -> Result<bool, LevelWriteError> {
        self.writes.push((pos, state, flags));
        self.glue_present_at_write |= with_world(|w| !w.dims[0].glue.is_empty()).unwrap_or(false);
        if self.refuse.contains(&pos) {
            return Ok(false);
        }
        let old = self.blocks.insert(pos, state);
        if let Some(name) = self.block_state_string(state) {
            self.fake.level.set_block(pos, states::split(&name).0);
        }
        self.fake.level.remove_block_entity(pos);
        with_world(|w| {
            w.apply_block_change(
                0,
                pos,
                old.map_or(0, states::id_or_none),
                states::id_or_none(state),
                flags.contains(BlockUpdateFlags::KNOWN_SHAPE),
            );
        });
        Ok(true)
    }
}

impl LevelEntitiesBackend for TestServer {
    fn level_visit_online_players(
        &self,
        _: LevelRef,
        _: &mut dyn FnMut(PlayerRef) -> ControlFlow<()>,
    ) -> Result<ControlFlow<()>, LevelReadError> {
        Ok(ControlFlow::Continue(()))
    }

    fn level_entity_by_uuid(&self, _: LevelRef, uuid: EntityUuid) -> Option<EntityRef> {
        self.carts
            .iter()
            .find(|cart| cart.uuid == uuid.0)
            .map(|cart| cart.entity)
    }

    fn level_visit_entities_in(
        &self,
        _: LevelRef,
        min: [f64; 3],
        max: [f64; 3],
        visit: &mut dyn FnMut(EntityRef) -> ControlFlow<()>,
    ) -> Result<ControlFlow<()>, LevelReadError> {
        for cart in &self.carts {
            if (0..3).all(|i| (min[i]..max[i]).contains(&cart.pos[i]))
                && visit(cart.entity).is_break()
            {
                return Ok(ControlFlow::Break(()));
            }
        }
        Ok(ControlFlow::Continue(()))
    }
}

impl LevelEntitiesBackendMut for TestServer {
    fn level_spawn_item_with_motion(
        &mut self,
        _: LevelRef,
        x: f64,
        y: f64,
        z: f64,
        stack: ItemStack,
        _: [f64; 3],
        _: i32,
    ) -> Result<Option<EntityRef>, StackWriteError> {
        self.dropped.push(([x, y, z], stack.count()));
        Ok(None)
    }
}

impl LevelSpawningBackendMut for TestServer {
    fn level_spawn_entity(
        &mut self,
        level: LevelRef,
        kind: &str,
        _: [f64; 3],
        spawn: &EntitySpawn,
    ) -> Option<EntityRef> {
        assert_eq!(spawn.body, SpawnBody::General);
        self.spawned.push(kind.to_owned());
        let rider = EntityRef::new(level.epoch(), 10);
        assert_eq!(self.rider.replace(rider), None);
        Some(rider)
    }
}

impl MinecartsServerHost for TestServer {
    fn minecart(&self, entity: EntityRef) -> Option<MinecartRef> {
        self.carts
            .iter()
            .any(|cart| cart.entity == entity)
            .then(|| MinecartRef::new(entity))
    }
}

impl EntityIdentityBackend for TestServer {
    fn entity_kind(&self, entity: EntityRef) -> Option<EntityTypeKey> {
        self.fake.entity_kind(entity)
    }
}

impl EntityIdentityBackendMut for TestServer {
    fn entity_uuid(&mut self, entity: EntityRef) -> Option<u128> {
        self.carts
            .iter()
            .find(|cart| cart.entity == entity)
            .map(|cart| cart.uuid)
    }
}

impl EntityPositionBackend for TestServer {
    fn entity_position(&self, entity: EntityRef) -> Option<[f64; 3]> {
        self.carts
            .iter()
            .find(|cart| cart.entity == entity)
            .map(|cart| cart.pos)
    }
}

impl EntityPositionBackendMut for TestServer {
    fn entity_teleport_to(
        &mut self,
        entity: EntityRef,
        pos: [f64; 3],
        _: Option<[f32; 2]>,
    ) -> bool {
        self.teleports.push(entity);
        self.carts
            .iter_mut()
            .find(|cart| cart.entity == entity)
            .unwrap()
            .pos = pos;
        true
    }
}

impl EntityRidingBackend for TestServer {
    fn entity_is_vehicle(&self, entity: EntityRef) -> Option<bool> {
        Some(self.mounted.is_some_and(|(_, cart)| cart == entity))
    }
}

impl EntityRidingBackendMut for TestServer {
    fn entity_start_riding(
        &mut self,
        entity: EntityRef,
        vehicle: EntityRef,
        force: bool,
    ) -> Result<(), StartRidingRefusal> {
        assert_eq!(self.rider, Some(entity));
        assert!(!force);
        assert_eq!(self.mounted.replace((entity, vehicle)), None);
        Ok(())
    }
}

impl EntityLifetimeBackendMut for TestServer {
    fn entity_remove(&mut self, entity: EntityRef, reason: RemovalReason) -> bool {
        assert_eq!(self.rider, Some(entity));
        assert_eq!(self.mounted.take().map(|(rider, _)| rider), Some(entity));
        self.removed.push((entity, reason));
        true
    }
}
