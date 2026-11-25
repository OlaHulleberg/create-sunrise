# API gaps

What the port still needs from Sunrise's mod API: when this list is empty, the port runs fully on the mod API with no
mixins and can be published to the mod registry. Keep it current: add an item when something is left out for want of
API, and remove it once the API has it and the port uses it.

## Entities and movement

- **A raw position write for the local player** (`Entity.setPos` on the client): no raw lift past the top of an upward
  belt slope. sunrise-org/sunrise#5072
- **`entity_inside` for the local player's own block sweep**: the local belt carry can't see a predicted swept entry.
  sunrise-org/sunrise#5076
- **Moving a server player by a SELF move without a relative teleport**: the server's copy of a belt rider isn't moved
  in Create's order. sunrise-org/sunrise#5073
- **Editing a body's motion at each downward block contact before restitution** (`restituteMovementAfterCollisions`):
  seats bounce by vanilla's formula, and ejectors launch at the end of the tick, not on contact. sunrise-org/sunrise#5076
- **A velocity edit after the lava move and before lava drag** (`LivingEntity.travelInLava`): netherite diving boots
  don't speed walking or jumping in lava. sunrise-org/sunrise#5074
- **Placing each passenger at its own point on its vehicle** (`positionRider`): riders can't sit on a moving
  structure's seats. sunrise-org/sunrise#5276
- **Physics and damage for a mod-declared body**: a thrown package is an item, not Create's package
  entity. sunrise-org/sunrise#1340
- **A projectile-context block clip, and a general entity position update that isn't a teleport, with the shared impact
  path**: potato cannon shots fly by teleport and can miss an impact. sunrise-org/sunrise#5083,
  sunrise-org/sunrise#1340
- **Damage to a general entity**: potato cannon shots don't hit other shots. sunrise-org/sunrise#5084
- **Fire immunity of entities that aren't living** (`Entity.fireImmune` on boats and projectiles): an open lava pipe
  can't ignite boats or projectiles as Create does. sunrise-org/sunrise#5082

## Contraptions

- **A turning collider pushing the bodies it moves into**: bearings, windmills and other turning structures pass
  through entities in their way. sunrise-org/sunrise#4185
- **Contact effects of collider boxes** (friction, bounce, climbing): no slime bounce, ice slide or ladder climb on a
  moving structure. sunrise-org/sunrise#1909
- **A turning collider carrying the client's own player**: a player can't ride a turning structure. sunrise-org/sunrise#5584
- **A descending collider carrying the client's own player down**: a player is pushed off a descending structure.
  sunrise-org/sunrise#5585
- **Attachment codecs with Host access, run when the holder is saved**: a cart's contraption isn't encoded when its
  holder is saved, as Create does. sunrise-org/sunrise#5085

## Rendering

- **The camera's eye position and the entity's interpolated position in `EntityRenderer::submit`, and a reusable item
  draw**: a potato cannon shot can't face the camera in every view. sunrise-org/sunrise#5077
- **A block's face-skip rule against other blocks, for model faces only** (`skipRendering`): faces between different
  glass kinds are drawn, and copycats aren't culled by their material. sunrise-org/sunrise#5080
- **Connected-texture rules that read a neighbour's state per face, a neighbour's appearance (`getAppearance`), a rule
  that block-entity data turns off, and position-seeded random sheets**: casings join only their own block, a brass
  tunnel without distribution still connects, and the weathered window doesn't pick its sheet per position.
  sunrise-org/sunrise#5081, sunrise-org/sunrise#1106
- **Mining cracks on positions beyond 8 blocks of the broken block**: large multiblocks crack only near the broken part.
  sunrise-org/sunrise#5079
- **Editing a humanoid's final pose after vanilla's animation** (`HumanoidModel.setupAnim`): a skyhook rider can't hold
  its riding pose. sunrise-org/sunrise#5078
- **Reading an item model's `gui3d`**: placards, belts and depots can't size and lay items as Create does. Not filed.
- **A `BlockEntityDataView` built from a `Compound`**: a carried block entity's data can't be lent to a draw as a view.
  Not filed.
- **A client re-asking a block's light emission once the data it reads arrives**: a copycat shows a new material's
  light only after its chunk is sent again. Not filed.

## Blocks and world

- **A per-position answer to whether a fluid may flow toward a side** (`FlowingFluid.canPassThrough`): water still
  picks a water wheel's axle when finding a slope. sunrise-org/sunrise#5087
- **A rail block's velocity edit inside the minecart rail step, with the cart's speed cap**: a controller rail sets the
  cart's speed outside the rail step. sunrise-org/sunrise#5075

## Items and inventories

- **The backing slots each face of a container reaches** (`WorldlyContainer.getSlotsForFace`): packagers on different
  faces of a furnace, brewing stand or composter count as one inventory. sunrise-org/sunrise#5086
- **Hiding a mod menu screen's player inventory slots** (`Slot.isActive`): the train schedule screen can't hide the
  inventory while no part is edited. sunrise-org/sunrise#3968

## Client

- **Setting the clipboard when a server reply to the player's own command arrives**: `/create debuginfo` can't copy
  its report. sunrise-org/sunrise#5088

## Testing

- **Constructing a `TagKey` outside the game**: tests can't fake tags. Not filed.

More general mod-API gaps from a parity audit are tracked in sunrise-org/sunrise#3938.
