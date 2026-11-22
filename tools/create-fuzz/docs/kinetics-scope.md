# Kinetics subsystem scope

What the kinetics comparison covers, researched in Create 6.0.11 (`b62e837`), and what it leaves
to other subsystems. Paths are under `src/main/java/com/simibubi/create/content/kinetics/`.
`coverage/subsystems.json` counts all of `content/kinetics` (and `api/stress`) as kinetics;
`coverage/reasons.tsv` marks the parts listed under "Left to other subsystems" with their own
categories, so the kinetics harness gaps are the work items of this subsystem alone.

## In scope

### Network propagation, stress and speed

- `RotationPropagator`: `handleAdded`/`propagateNewSource` (speed conveyed to each connected
  neighbour, overpowering the slower tree, `destroyBlock` on opposite directions, on speeds past
  `maxRotationSpeed` and on a flicker score past 128), `handleRemoved`/`propagateMissingSource`
  (the dependent tree cleared and a replacement source searched), `getRotationSpeedModifier`
  (shaft axes, gearbox and split-shaft halves through `getAxisModifier`, chain drives, large and
  small cog meshes) and the controller's `isLargeCogToSpeedController`.
- `TorquePropagator` and `KineticNetwork`: network ids, member admission and removal, capacity
  and stress sums with `getStressMultiplierForSpeed`, and `updateFromNetwork` on every member.
- `base/KineticBlockEntity`: `tick` (validation every `kineticValidationFrequency` ticks,
  `validateKinetics`, flicker tally, dirty networks), `onSpeedChanged`, `setSource`/`removeSource`,
  overstress (`updateFromNetwork`), `switchToBlockState` for kinetically equivalent states, and
  save/load (`write`/`read`).
- `base/GeneratingKineticBlockEntity`: `updateGeneratedRotation`, `applyNewSpeed` and source
  reactivation.
- `base/IRotate`: `SpeedLevel` and `StressImpact` thresholds as gauges and goggles read them.
- `api/stress/BlockStressValues` and the configured impacts and capacities.

### Sources

- `motor/CreativeMotorBlock(Entity)` and `KineticScrollValueBehaviour` (the speed board).
- `crank/HandCrankBlock(Entity)` and `ValveHandleBlock(Entity)`: turning, its timer, the valve
  angle board and dyes.
- `waterwheel/*`: flow sampling, wheel material, the large wheel's structure blocks.
- `steamEngine/*`: boiler heat and water, engine count, powered shafts.
- `flywheel/FlywheelBlock(Entity)`.

### Relays and transmissions

- `simpleRelays/*`: shafts (brackets, casings, girders, `ShaftBlock.PlacementHelper`), cogs
  (`CogWheelBlock` placement and `isValidCogwheelPosition`, `CogwheelBlockItem` and its four
  placement helpers), encased shafts and cogs (`EncasedCogwheelBlock` shaft caps and
  `swapShaftsForRotation`).
- `gearbox/*`, `transmission/*` (clutch, gearshift, the sequenced gearshift and its instructions),
  `chainDrive/*` (chain drives and the adjustable chain gearshift), `speedController/*`.
- `gauge/*`: speed and stress gauges, their dials and comparator output.

### Block states, placement and the wrench

- The base block classes `base/KineticBlock`, `DirectionalKineticBlock`,
  `DirectionalAxisKineticBlock`, `HorizontalKineticBlock`, `HorizontalAxisKineticBlock`,
  `RotatedPillarKineticBlock`, `AbstractEncasedShaftBlock`: `getStateForPlacement` with the
  preferred-neighbour searches (`getPreferredFacing`, `getPreferredHorizontalAxis`,
  `prefersConnectionTo`, `getPreferredAxis`), `hasShaftTowards` and `getRotationAxis` on every
  state, `onPlace`, `setPlacedBy` and `updateIndirectNeighbourShapes`.
- Player inputs that change kinetic state: block placement through `ServerPlayerGameMode.useItemOn`
  (clicked block, face, hit, look and sneaking), the wrench (`IWrenchable.onWrenched` with
  `getRotatedBlockState` and each block's override, `onSneakWrenched`), casings, brackets,
  girders, the speed, valve and sequencer boards, redstone power and block removal.
- The kinetic connection geometry of every block in `rust/src/kinetics.json`, machines included:
  the static catalogue (`geometry/*` families) injects each state and checks its shafts and cogs.

## Left to other subsystems

| Left to | Java | Category in `reasons.tsv` |
| --- | --- | --- |
| Belts and items (its own families and report) | `belt/*`, `chainConveyor/*` | `harness-gap` (belts work items) |
| Processing: machine work | `mixer/*`, `millstone/*`, `crusher/*`, `saw/*`, `drill/*`, `deployer/*`, `mechanicalArm/*`, `crafter/*`, `turntable/*`, `clock/CuckooClockBlockEntity`, `base/BlockBreakingKineticBlockEntity` | `out-of-scope:processing` |
| Processing already driven through belts and depots | `press/*` (`press_processing`), `fan/*` (`depot_outputs`, `chute_columns`) | `harness-gap` |
| Contraptions and schematics | `gantry/*` carriages, `BlockBreakingMovementBehaviour`, every `transform`, and `rotate`/`mirror` (below) | `out-of-scope:contraptions`, `out-of-scope:schematics-and-contraptions` |
| Client | renderers, visuals, models, particles, screens, `KineticDebugger` | `client` |

Machine work is an item, fluid or entity process run by the machine's own tick: the mixer's basin
recipes (`MechanicalMixerBlockEntity.tick`, MechanicalMixerBlockEntity.java:126), milling
(`MillstoneBlockEntity.tick`, :87), crushing (`CrushingWheelControllerBlockEntity.tick`, :98),
sawing (`SawBlockEntity.tick`, :170), a deployer's fake player (`DeployerBlockEntity.tick`, :173),
an arm's interaction points (`ArmBlockEntity.tick`, :128), crafting grids
(`MechanicalCrafterBlockEntity.tick`, :255), a turntable turning entities
(`TurntableBlock.entityInside`, :45), the cuckoo clock's animation and explosion
(`CuckooClockBlockEntity.tick`, :70) and the stationary breaker's progress
(`BlockBreakingKineticBlockEntity`, :43-86). Their speed, stress impact and shaft faces stay in
kinetics through the catalogue; what they do with that speed does not.

`Block.rotate` and `Block.mirror` on kinetic blocks are structure transforms: only
`StructureTransform` (contraptions/StructureTransform.java:172-182, contraptions and schematics),
vanilla structure templates and the symmetry wand (`SymmetryMirror`, SymmetryMirror.java:134-167)
call them. The wrench never does: `IWrenchable.onWrenched` turns a state with
`getRotatedBlockState` (IWrenchable.java:63-105).

## Families

The kinetics families and what each drives are in the README's seed table and the doc comments
in `rust/src/network_inputs.rs`, `transmission.rs`, `geometry.rs`, `kinetic_faces.rs`,
`waterwheel.rs` and `steam_engine.rs`. The two face families:

- `network/face_placement` (256k + 220): every kinetic block item `contested_placement` places, and
  the flywheel, used on each of the six faces of a shaft on x and y, a small cog, a large cog, a
  gearbox and an encased shaft, sneaking and not, with four yaws and three pitches. The clicked
  block's `useItemOn` runs first (the shaft and cog placement helpers), then the item's placement
  with that clicked face (`GaugeBlock.getStateForPlacement` on a shaft end, `getAxisForPlacement`
  and the preferred-neighbour searches with a kinetic block behind). 192 indices meet every
  combination.
- `network/face_wrench` (256k + 252, always reloads): each of those items placed on stone, then
  wrenched from one face, the opposite face and a face across them, with a creative motor beside
  it in half the indices, and the last one picked up with a sneaking wrench. 32 indices.
