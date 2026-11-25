# API gaps

What the port still needs from Sunrise's mod API: when this list is empty, the port runs fully on the mod API with no
mixins and can be published to the mod registry. Keep it current: add an item when something is left out for want of
API, and remove it once the API has it and the port uses it.

## Entities and movement

- **A raw position write for the local player** (`Entity.setPos` on the client): no raw lift past the top of an upward
  belt slope. Create uses: calls vanilla `Entity.setPos` ([`BeltMovementHandler.java:196`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/kinetics/belt/transport/BeltMovementHandler.java#L196)). sunrise-org/sunrise#5072
- **`entity_inside` for the local player's own block sweep**: the local belt carry can't see a predicted swept entry.
  Create uses: overrides vanilla `Block.entityInside` ([`BeltBlock.java:184`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/kinetics/belt/BeltBlock.java#L184)). sunrise-org/sunrise#5076
- **Moving a server player by a SELF move without a relative teleport**: the server's copy of a belt rider isn't moved
  in Create's order. Create uses: calls vanilla `Entity.move(MoverType.SELF, …)` ([`BeltMovementHandler.java:176`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/kinetics/belt/transport/BeltMovementHandler.java#L176)). sunrise-org/sunrise#5073
- **Editing a body's motion at each downward block contact before restitution** (`restituteMovementAfterCollisions`):
  seats bounce by vanilla's formula, and ejectors launch at the end of the tick, not on contact. Create uses: overrides
  vanilla `Block.updateEntityAfterFallOn` ([`SeatBlock.java:89`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/contraptions/actors/seat/SeatBlock.java#L89), [`EjectorBlock.java:101`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/logistics/depot/EjectorBlock.java#L101)), with Create mixin `EntityMixin` on `Entity.move` at
  `getOnPosLegacy` ([`EntityMixin.java:33`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/foundation/mixin/EntityMixin.java#L33)). sunrise-org/sunrise#5076
- **A velocity edit after the lava move and before lava drag** (the lava branch of `LivingEntity.travel`): netherite diving boots
  don't speed walking or jumping in lava. Create uses: Create mixin `LavaSwimmingMixin` on `LivingEntity.travel`, after
  its first `move` in the `isInLava` slice ([`LavaSwimmingMixin.java:26`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/foundation/mixin/LavaSwimmingMixin.java#L26)). sunrise-org/sunrise#5074
- **Placing each passenger at its own point on its vehicle** (`positionRider`): riders can't sit on a moving
  structure's seats. Create uses: overrides vanilla `Entity.positionRider` ([`AbstractContraptionEntity.java:208`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/contraptions/AbstractContraptionEntity.java#L208)). sunrise-org/sunrise#5276
- **Physics and damage for a mod-declared body**: a thrown package is an item, not Create's package
  entity. Create uses: extends vanilla `LivingEntity` ([`PackageEntity.java:58`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/logistics/box/PackageEntity.java#L58)). sunrise-org/sunrise#1340
- **A projectile-context block clip, and a general entity position update that isn't a teleport, with the shared impact
  path**: potato cannon shots fly by teleport and can miss an impact. Create uses: extends vanilla
  `AbstractHurtingProjectile` and runs its `tick` ([`PotatoProjectileEntity.java:153`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/equipment/potatoCannon/PotatoProjectileEntity.java#L153)). sunrise-org/sunrise#5083, sunrise-org/sunrise#1340
- **Damage to a general entity**: potato cannon shots don't hit other shots. Create uses: overrides vanilla
  `Entity.hurt` on the shot ([`PotatoProjectileEntity.java:302`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/equipment/potatoCannon/PotatoProjectileEntity.java#L302)). sunrise-org/sunrise#5084
- **Fire immunity of entities that aren't living** (`Entity.fireImmune` on boats and projectiles): an open lava pipe
  can't ignite boats or projectiles as Create does. Create uses: calls vanilla `Entity.fireImmune` and
  `Entity.igniteForSeconds` ([`LavaEffectHandler.java:19`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/impl/effect/LavaEffectHandler.java#L19)). sunrise-org/sunrise#5082

## Contraptions

- **A turning collider pushing the bodies it moves into**: bearings, windmills and other turning structures pass
  through entities in their way. Create uses: Create's own `ContraptionCollider.collideEntities` ([`ContraptionCollider.java:283`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/contraptions/ContraptionCollider.java#L283)), run from
  NeoForge `LevelTickEvent.Post` ([`CommonEvents.java:133`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/foundation/events/CommonEvents.java#L133)). sunrise-org/sunrise#4185
- **Contact effects of collider boxes** (friction, bounce, climbing): no slime bounce, ice slide or ladder climb on a
  moving structure. Create uses: Create's own code in `collideEntities` ([`ContraptionCollider.java:207`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/contraptions/ContraptionCollider.java#L207), [`ContraptionCollider.java:223`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/contraptions/ContraptionCollider.java#L223)). sunrise-org/sunrise#1909
- **A turning collider carrying the client's own player**: a player can't ride a turning structure. Create uses:
  Create's own `collideEntities` on the client ([`ContraptionCollider.java:123`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/contraptions/ContraptionCollider.java#L123)), run from NeoForge `ClientTickEvent.Post` ([`ClientEvents.java:129`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/foundation/events/ClientEvents.java#L129)).
  sunrise-org/sunrise#5584
- **A descending collider carrying the client's own player down**: a player is pushed off a descending structure.
  Create uses: Create's own `saveClientPlayerFromClipping` ([`ContraptionCollider.java:336`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/contraptions/ContraptionCollider.java#L336)). sunrise-org/sunrise#5585
- **Attachment codecs with Host access, run when the holder is saved**: a cart's contraption isn't encoded when its
  holder is saved, as Create does. Create uses: overrides vanilla `Entity.addAdditionalSaveData` ([`AbstractContraptionEntity.java:612`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/contraptions/AbstractContraptionEntity.java#L612)).
  sunrise-org/sunrise#5085

## Rendering

- **The camera's eye position and the entity's interpolated position in `EntityRenderer::submit`, and a reusable item
  draw**: a potato cannon shot can't face the camera in every view. Create uses: calls vanilla
  `Minecraft.getCameraEntity().getEyePosition(pt)` ([`AllPotatoProjectileRenderModes.java:48`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/equipment/potatoCannon/AllPotatoProjectileRenderModes.java#L48)) and `ItemRenderer.renderStatic` ([`PotatoProjectileRenderer.java:34`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/equipment/potatoCannon/PotatoProjectileRenderer.java#L34)).
  sunrise-org/sunrise#5077
- **A block's face-skip rule against other blocks, for model faces only** (`skipRendering`): faces between different
  glass kinds are drawn, and copycats aren't culled by their material. Create uses: overrides vanilla
  `Block.skipRendering` ([`ConnectedGlassBlock.java:20`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/decoration/palettes/ConnectedGlassBlock.java#L20)) and `hidesNeighborFace` ([`CopycatPanelBlock.java:176`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/decoration/copycat/CopycatPanelBlock.java#L176)). sunrise-org/sunrise#5080
- **Connected-texture rules that read a neighbour's state per face, a neighbour's appearance (`getAppearance`), a rule
  that block-entity data turns off, and position-seeded random sheets**: casings join only their own block, a brass
  tunnel without distribution still connects, and the weathered iron window keeps its plain texture, as no rule
  picks its sheet per position.
  Create uses: Create's own `ConnectedTextureBehaviour`, calling `BlockState.getAppearance` ([`ConnectedTextureBehaviour.java:79`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/foundation/block/connected/ConnectedTextureBehaviour.java#L79)).
  sunrise-org/sunrise#5081, sunrise-org/sunrise#1106
- **Mining cracks on positions beyond 8 blocks of the broken block**: large multiblocks crack only near the broken part.
  Create uses: Create mixin `LevelRendererMixin` on `LevelRenderer.destroyBlockProgress`, reading NeoForge
  `IClientBlockExtensions` ([`LevelRendererMixin.java:37`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/foundation/mixin/client/LevelRendererMixin.java#L37)). sunrise-org/sunrise#5079
- **Editing a humanoid's final pose after vanilla's animation** (`HumanoidModel.setupAnim`): a skyhook rider can't hold
  its riding pose. Create uses: Create mixin `HumanoidModelMixin` at the return of `HumanoidModel.setupAnim`
  ([`HumanoidModelMixin.java:23`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/foundation/mixin/client/HumanoidModelMixin.java#L23)). sunrise-org/sunrise#5078
- **Reading an item model's `gui3d`**: placards, belts and depots can't size and lay items as Create does. Create uses:
  calls vanilla `BakedModel.isGui3d` ([`PlacardRenderer.java:38`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/decoration/placard/PlacardRenderer.java#L38)). Not filed.
- **A `BlockEntityDataView` built from a `Compound`**: a carried block entity's data can't be lent to a draw as a view.
  Create uses: builds a block entity with vanilla `EntityBlock.newBlockEntity` and fills it with
  `BlockEntity.handleUpdateTag` ([`ClientContraption.java:151`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/contraptions/render/ClientContraption.java#L151)). Not filed.
- **A client re-asking a block's light emission once the data it reads arrives**: a copycat shows a new material's
  light only after its chunk is sent again. Create uses: NeoForge `AuxiliaryLightManager.setLightAt` ([`CopycatBlockEntity.java:124`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/decoration/copycat/CopycatBlockEntity.java#L124)). Not
  filed.

## Blocks and world

- **A rail block's velocity edit inside the minecart rail step, with the cart's speed cap**: a controller rail sets the
  cart's speed outside the rail step. Create uses: overrides `onMinecartPass`, reading
  `AbstractMinecart.getMaxSpeedWithRail` ([`ControllerRailBlock.java:144`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/redstone/rail/ControllerRailBlock.java#L144)). sunrise-org/sunrise#5075

## Items and inventories

- **The backing slots each face of a container reaches** (`WorldlyContainer.getSlotsForFace`): packagers on different
  faces of a furnace, brewing stand or composter count as one inventory. Create uses: calls vanilla
  `WorldlyContainer.getSlotsForFace` ([`AllInventoryIdentifiers.java:85`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/logistics/packager/AllInventoryIdentifiers.java#L85)). sunrise-org/sunrise#5086
- **Hiding a mod menu screen's player inventory slots** (`Slot.isActive`): the train schedule screen can't hide the
  inventory while no part is edited. Create uses: overrides vanilla `Slot.isActive` ([`ScheduleMenu.java:63`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/content/trains/schedule/ScheduleMenu.java#L63)). sunrise-org/sunrise#3968

## Client

- **Setting the clipboard when a server reply to the player's own command arrives**: `/create debuginfo` can't copy
  its report. Create uses: a Catnip clientbound payload whose handler calls vanilla `KeyboardHandler.setClipboard`
  ([`ServerDebugInfoPacket.java:41`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/infrastructure/debugInfo/ServerDebugInfoPacket.java#L41)). sunrise-org/sunrise#5088

## Testing

- **Constructing a `TagKey` outside the game**: tests can't fake tags. Create uses: vanilla `TagKey.create`
  ([`AllTags.java:44`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/AllTags.java#L44)); its tests are in-game GameTests on vanilla `GameTestHelper` ([`CreateGameTestHelper.java:64`](https://github.com/Creators-of-Create/Create/blob/b62e837acab5e2ceeb7bfd0c9fd551c64b52dd18/src/main/java/com/simibubi/create/infrastructure/gametest/CreateGameTestHelper.java#L64)). Not filed.

More general mod-API gaps from a parity audit are tracked in sunrise-org/sunrise#3938.
