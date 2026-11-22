# Processing scope

What the processing machines are in Create 6.0.11 (`b62e837`), as its Java says, and how much of
them the fuzzer drives. Paths are under `src/main/java/com/simibubi/create/content/`. The
[kinetics scope](kinetics-scope.md) left this machine work to its own subsystem; the machines'
shafts and cogs stay in the kinetic catalogue.

## Driven

Three families, one residue each modulo 256 (`rust/src/processing.rs`). None reloads.

| Family | Residue | Machine |
| --- | --- | --- |
| `processing/millstone` | 205 | A millstone on a creative motor |
| `processing/crushing` | 221 | Two crushing wheels on two motors and the controller between them |
| `processing/saw` | 128 | An upward saw on a motor, a depot beside it or nothing |

They took residues from `geometry/states` (205 and 221, copies of 77 and 93) and
`geometry/stress_boundary` (128 and 192, which it keeps), so those two families keep reaching every
state and load step over more seeds.

The press over a depot or belt is `belt/press_processing`, and a fan's current over a depot or a
chute is `belt/depot_outputs` and `belt/chute_columns` ([item transport](item-transport.md)). The
press now also reports its head (`running`, `runningTicks`, `mode`) in version 3 cases.

### Millstone

`kinetics/millstone/MillstoneBlockEntity`, block `create:millstone` (no properties; shaft below).

- State: `inputInv` (1 slot), `outputInv` (9), `timer`, `lastRecipe`.
- `tick` (:87): nothing without speed, or while any output slot holds its slot limit (64,
  `getSlotLimit`, not the item's stack size). A running timer goes down by `getProcessingSpeed`
  (:207, `clamp(|speed| / 16, 1, 512)`) and processes at 0 or below. With the timer out and an
  input, it finds the milling recipe: none waits 100 ticks, one sets the recipe's
  `processing_time`.
- `process` (:150): one input item, the rolled results stacked into the outputs
  (`ItemHandlerHelper.insertItemStacked`), then the input's crafting remainder.
- Inputs: an item landing on it (`MillstoneBlock.updateEntityAfterFallOn`, :87, inserting into
  slot 0 through the handler) and the item handler (`MillstoneInventoryHandler`, :222): slot 0
  takes only what it can mill (`canProcess`), outputs refuse, the input gives nothing. An empty
  hand takes the outputs, or the input when there are none (`MillstoneBlock.useItemOn`, :52).
- Breaking it drops both inventories (`destroy`, :144).
- Observed: `timer`, the input slot and the nine output slots.

### Crushing wheels

`kinetics/crusher/CrushingWheelBlock`, `CrushingWheelBlockEntity`, `CrushingWheelControllerBlock`,
`CrushingWheelControllerBlockEntity`. Blocks `create:crushing_wheel[axis]` and
`create:crushing_wheel_controller[facing,valid]`.

- Wheels two apart on the same axis keep a controller between them
  (`CrushingWheelBlock.updateControllers`, :60, on placement, removal and speed changes). Turning
  against each other it is valid and faces the way they throw (down between horizontal wheels side
  by side, up when turned the other way); turning the same way it is invalid. Its
  `crushingspeed` is a wheel's |speed| / 50 (`CrushingWheelControllerBlock.updateSpeed`, :156).
- State: a 32-slot `ProcessingInventory` (`remainingTime`, `recipeDuration`, `appliedRecipe`) and
  the entity it draws in.
- `tick` (:98): without an entity, time goes down by `clamp(speed × 4 / log2(count), 0.25, 20)`
  until applied, then by `speed × 4` (:136). Under 20 it applies the recipe (`applyRecipe`, :302):
  crushing, else milling (`findRecipe`, :328), each input item rolling the results. `applyRecipe`
  clears the inventory first (`ProcessingInventory.clear`), which zeroes the time, so the results
  leave on the next tick. Facing anything but up, a belt or depot below (and on, past a sideways
  facing) takes them (`DirectBeltInputBehaviour`); else they are thrown out 0.55 past the middle
  with `outSpeed` (:124) and the inventory is cleared, `appliedRecipe` with it.
- Inputs: the item handler on every side (`registerCapabilities`, :76): `isItemValid`
  (`ProcessingInventory`, :83) takes slot 0 of an empty inventory only, and the controller only
  while it holds no entity; `itemInserted` (:365) sets the recipe's time, else 100. A belt's end
  facing in (`supportsDirectBeltInput`, :89). Entities falling in (`entityInside`), which the
  harness never leaves alive long enough (it discards item entities each tick).
- Observed: `crushingspeed`, whether it holds an entity, the slots, the time, `recipeDuration` and
  `appliedRecipe`; the depot below and the thrown items (`drops`).

### Upward saw

`kinetics/saw/SawBlock`, `SawBlockEntity`. Block
`create:mechanical_saw[axis_along_first,facing,flipped]`; facing up its shaft runs along x with
`axis_along_first`, else z (`DirectionalAxisKineticBlock.getRotationAxis`).

- State: a 32-slot `ProcessingInventory` with a slot limit of 1 (`bulkCutting` off, :92),
  `recipeIndex`, a recipe filter.
- Inserting into slot 0 calls `start` (:426) at once (the inventory's callback): with recipes,
  the next in turn (`recipeIndex`), its time per 5 items; without, 10 ticks.
- `tick` (:170): only facing up (`canProcess`, :462) and turning. Time goes down by
  `clamp(|speed| / 24, 1, 128)`; under 5 it applies the recipe and waits 20 more. At 0 a pulling
  belt funnel above takes the results first, else a belt or depot the way it turns
  (`getItemMovementVec`, :334), else they are thrown out and the inventory cleared (its time to
  -1). Handing them to a belt or depot leaves the empty inventory applied with no time.
- Inputs: an item landing on a turning saw (`SawBlock.updateEntityAfterFallOn`, :167, then
  `insertItem`, :407) and the item handler on every side but the bottom (`registerCapabilities`,
  :98), both one item at a time into an empty saw.
- Observed: the slots, the time, `recipeDuration`, `appliedRecipe`, `recipeIndex` and the filter;
  the depot it turns toward and the thrown items.

### Recipes

`rust/src/processing.json` lists the recipes the families' inputs reach: Create's milling
(wheat, cobblestone, gravel, andesite, bone, sugar cane), crushing (obsidian, gravel, coal ore)
and the two cutting recipes `RuntimeDataGenerator.cuttingRecipes` makes for oak logs; dirt has
none. `ProcessingRecipeAudit` checks every entry against the loaded recipe and that no other
milling, crushing, cutting or stonecutting recipe takes a family's input for its machine
(stonecutting is on for saws by default, `allowStonecuttingOnSaw`), so the driver's recipe order
cannot differ from Create's.

### Rolled results

`ProcessingRecipe.rollResults` (`processing/recipe/ProcessingRecipe.java:148`) rolls each output
below chance 1 once per count with `level.random` (`ProcessingOutput.rollOutput`, :87): the
millstone (`MillstoneBlockEntity.java:164`), the crushing controller (`:312`) and the saw
(`:371`). The port rolls the same odds from its own seed. Two Java runs of the same case differ
too, so a case whose input has a rolled recipe compares what chance decides by structure
(`processing::Rolls`): an item a rolled recipe makes for sure compares by presence, one it makes
only by chance not at all, in machine slots, depots, thrown items and items given to a player.
Depots' saved payloads are then not compared, nor the comparator of one holding such an item. Inputs, times and fixed recipes'
results compare exactly. Where the rolls change what happens next (a depot taking one result
stack but not two, so the next insert is refused) Java disagrees with itself and the case is
reported unstable.

## Port differences found

Fixed, each with an engine test carrying Create's numbers and a case in `cases/`:

| Case | Java | Port before | Deciding Java |
| --- | --- | --- | --- |
| `processing-crusher-handler` | A controller takes 4 cobblestone into slot 0 when empty, refuses more, refuses other slots | No item handler | `CrushingWheelControllerBlockEntity.java:76`, `ProcessingInventory.java:83` |
| `processing-crusher-clear` | 2 obsidian at 128 rpm applied on the 47th tick, thrown out on the 48th; the cleared controller has no time and is not applied | Thrown out a tick or more later (the time ran on from under 20); kept its time and `appliedRecipe` | `CrushingWheelControllerBlockEntity.java:309`, `:196`, `ProcessingInventory.java:35` |
| `processing-saw-handler` | The saw's handler takes one log into slot 0 | No item handler | `SawBlockEntity.java:98` |
| `processing-saw-start` | A log dropped on a saw at 48 rpm has 50 left at once, out on the 33rd tick | Started a tick later | `SawBlockEntity.java:92`, `:426` |
| `processing-saw-broken` | A broken saw cutting a log drops it, scattered in its block | Discarded it with the block entity (and would have spawned it at the middle, still) | `SawBlockEntity.java:284`, `ItemHelper.dropContents` |
| `processing-crusher-removed` | A controller holding 8 cobblestone drops them when a wheel goes | Dropped nothing | `CrushingWheelControllerBlock.java:197` |
| `processing-saw-depot-turn` | A depot placed after the saw moves the result 0.125 in the tick it arrives | Moved it a tick later (saws ticked after the transport walk) | `SawBlockEntity.java:170` in the level's block-entity order |

Remaining (repro: `./run.sh fuzz gen <seed>` then `check`):

- Seed 2765 (`processing/millstone`): the millstone's handler takes 19 dirt into slot 0. Create's
  `MillstoneInventoryHandler.insertItem` (:236) refuses what it can't mill (`canProcess`, :211).
  The port's container has no per-item rule; the SDK's `can_place_item` hook sees the entity's
  slots but not its block, so a millstone cannot be told from a basin there.
- Seed 2269 (`processing/crushing`): results thrown out of a controller facing down appear at
  y − 0.8, Create's at y − 0.55 (`outPos`, :130). Create tags them `BypassCrushingWheel` (:193) so
  the controller's collision for items lets them through; the port has no such tag and starts
  them clear of it (`crusher::output`).

## Not driven yet

| Machine | Java | Why not yet |
| --- | --- | --- |
| Mixer over a basin | `kinetics/mixer/MechanicalMixerBlockEntity` (`tick`, :126: 40-tick cycle, `processingTicks` :157 from speed and the recipe's time), `processing/basin/*` (`BasinBlockEntity.tick` :344, `BasinRecipe.match` :39 and `apply` :61, heat from the block below :618, spout output :376) | The Java snapshot reports basins (inputs, outputs, tanks, filter) and mixers (`running`, `runningTicks`, `processingTicks`); the driver does not mirror `glue/processing/basin.rs` yet |
| Press over a basin | `MechanicalPressBlockEntity.tryProcessInBasin` (:83), `PressingBehaviour.tick` (`CYCLE` 240) | As the mixer: the basin |
| Drill and sideways saw | `kinetics/base/BlockBreakingKineticBlockEntity` (`tick` :86, `lazyTick` :43), `saw/TreeCutter` | Block breaking with hardness, drops and tree felling |
| Deployer | `kinetics/deployer/DeployerBlockEntity.tick` (:173), `DeployerHandler` | A fake player's item use |
| Mechanical arm | `kinetics/mechanicalArm/ArmBlockEntity.tick` (:128), interaction points | Placement selection packets and points |
| Mechanical crafter | `kinetics/crafter/MechanicalCrafterBlockEntity.tick` (:255) | Grids, connected inputs and crafting recipes |
| Fans on entities | `kinetics/fan/AirCurrent`, `fan/processing/FanProcessing` | Item entities in flight, which the harness discards |
