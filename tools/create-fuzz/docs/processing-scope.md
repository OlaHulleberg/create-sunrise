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
| `breaking/drill` | 93 | A drill on a creative motor against the block in front of it |
| `breaking/saw` | 192 | A sideways saw on a motor against what stands in front of it |

They took residues from `geometry/states` (205, 221, 93 and 109, 125 mod 128, the first two copies
of 77 and 93) and `geometry/stress_boundary` (128 and 192), so those two families keep reaching
every state and load step over more seeds. The two breaking families are below.

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
(`:371`). `level.random` is the server's own and nothing reseeds it per case, so where its
stream stands when a case starts is the batch's: two Java runs of one case roll differently. The
port rolls the same odds from its own seed. A case whose input has a rolled recipe therefore
compares what chance decides by structure (`processing::Rolls`): an item a rolled recipe makes
for sure compares by presence, one it makes only by chance not at all, in machine slots, depots,
thrown items and items given to a player. A machine's slots are then read by stack on both sides
whenever either holds a rolled stack: results go to the first slot that fits
(`MillstoneBlockEntity.java:165`), so a chance result being there or not moves the fixed results
after it to other slots. Seed 7885's bone left white dye in one Java run and not in another, and
the cobblestone milled after it sat in output slot 1 or 2. Depots' saved payloads are then not
compared, nor the comparator of one holding such an item. Inputs, times and fixed recipes'
results compare exactly.

Where the rolls would change what happens next, the family does not go on:

- A depot below a controller takes one stack of the results, and how many more stay in the
  controller (so whether it takes the next insert) is chance's: over a depot nothing is put in
  after a rolled input (seeds 5341 and 6109, where two Java runs disagreed on a later insert).
- After a rolled input has reached a millstone's input slot, none of its output slots is drawn
  on: which stack sits there is then chance's.
- All a case's rolled inputs make less than a full slot of any one result, since a full one
  stops the millstone (`MillstoneBlockEntity.tick`, :92) when chance says.

### Block breaking

The drill and the sideways saw break the block in front of them through
`BlockBreakingKineticBlockEntity` (`base/BlockBreakingKineticBlockEntity.java`), as
`rust/src/breaking.rs` drives and `engine/breaking.rs` mirrors from the port's glue
(`glue/processing/saw.rs`) and engine (`engine/src/drill.rs`, `tree.rs`):

- `tick` (:86). With speed 0 nothing happens (:93). `ticksUntilNextProgress` counts down (:98);
  when it is 0 the block along `facing` is looked at (`getBreakingPos`,
  `DrillBlockEntity.java:36`, `SawBlockEntity.java:476`). `isBreakable` (:135) refuses a liquid,
  air, hardness −1 and `create:non_breakable` (no block of Create's own is in it); a refused block
  resets the progress (:106). Otherwise `destroyProgress += clamp((int)(breakSpeed / hardness), 1,
  10 − destroyProgress)` with `breakSpeed = |speed| / 100` (:114, :157): 10 breaks the block and
  rests the breaker at −1 (:119); short of 10 it waits `(int)(hardness / breakSpeed)` ticks
  (:127). Stone (1.5) takes one step every 19 ticks at 8 rpm and one every 3 at 64; TNT (0) goes
  on the first look.
- Looking again. `ticksUntilNextProgress` starts at 0, so a breaker looks on its first tick. A
  resting one (−1) is set to 1 by `lazyTick` (:43; every 11th tick of its block entity,
  `SmartBlockEntity.java:74`), by `DrillBlock.neighborChanged` (`drill/DrillBlock.java:80`) when
  any of its six neighbours changes, and by `SawBlockEntity.tick` (`saw/SawBlockEntity.java:170`)
  itself every tick. The saw has no `neighborChanged`; an upward saw breaks nothing (`shouldRun`,
  :468).
- Drops. `onBlockBroken` (:139) gives the block's loot (`BlockHelper.destroyBlock`, :213, no tool)
  at the block's middle within an eighth, standing still, and nothing with `doTileDrops` off. An
  ore's experience is popped too (`foundation/utility/BlockHelper.java:237`). The saw then fells
  what stood on the cut block (`SawBlockEntity.onBlockBroken`, :481): `TreeCutter.findTree`
  (`saw/TreeCutter.java:71`) takes the logs it stands on, the leaves and attached blocks over
  them within two blocks, and `Tree.destroyBlocks` (:337) drops attachments, logs, then leaves,
  each at its block's middle flying on along the saw's facing by `distance / 20`
  (`dropItemFromCutTree`, :495). `canBreak` (:505) also asks `isSawable` (:510): not saplings;
  logs, leaves, roots, bamboo, the solid crops, cactus, sugar cane, kelp and chorus.
- `write` and `read` (:59) save `Progress` and `NextTick`; `invalidate` (:78) clears the cracks of
  a breaker that had progress.

The comparison reads a breaker's `Progress` and `NextTick` (`breaking`) and the cracks it last
asked the level to show at the block (`cracks`: 1.21.1 keeps no crack map on the server).

Around the breakers the driver keeps the game's own rules, each as the server ran it on a fixture
(`engine/vanilla.rs`): a leaf's `distance` (1 beside a log, else the least around it one further,
else 7; nothing decays, the server runs no random ticks), a falling block (a scheduled tick two
later, then 0.04 faster a tick, dragged by 0.98, landing in the cell it rests in; a second one
resting where the first just landed turns into its item at the cell's floor), and a huge
mushroom block's faces turning to their skin towards a block of their own kind that comes. Water
and the rest is the native fluid host's ([fluid world](fluid-world.md)). `rust/src/breaking.json`
is the vanilla contract (hardness, property domains, tags, fixed and rolled drops, `stands_on`,
what falls); `BreakingAudit` checks it against the loaded game at startup.

`breaking/drill` puts a drill on a motor against a block, in twelve histories on six facings at
six speeds: the target from the start, swapped, taken away and put back, the drill or its motor
removed while it cracks, the world saved and reloaded, a block arriving in front of an idle drill,
a sand column refilling what it breaks, `doTileDrops` off, two drills on one block, nine target
kinds (bedrock, TNT, glass, coal ore, gravel, cobblestone, clay, a log, water), and the drill
wrenched while it cracks. `breaking/saw` puts a sideways saw against fifteen: a log, trees it
fells (centre out or rim in), a cut a second trunk holds up, a branch, mangrove roots, a mushroom
stem among caps and a shroomlight, bamboo, a pumpkin or melon, what a saw cannot cut, a crimson
stem among wart blocks, a tree with bee nests and moss carpet on it, leaves alone, the saw or its
motor removed, and `doTileDrops` off. Neither family reloads by seed bit.

Not compared: experience (the harness discards the orbs, as it does the drops it never observes);
the items chance decides (a leaf's sticks, saplings and apples, gravel's flint, a melon's seeds, a
mushroom block's mushrooms), which both sides leave out wherever the block that rolls them shows;
sounds and particles. A version 3 event `{"kind":"game_rule","do_tile_drops":false}` turns the
rule off from its tick to the case's end; the server puts its own rule back after the case.

Cases in `cases/`: `breaking-saw-upward`, `breaking-drill-unbreakable`, `breaking-drill-water`,
`breaking-mushroom-skins`, `breaking-leaf-distances`, `breaking-sand-falls`,
`breaking-sand-column-pull`. Every history where a breaker breaks a block differs from Java by the
first two rows below, so those histories are in the two families and, shrunk, with the report of
the differences; the cases that pin them go to `cases/` once the port is right.

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
| `processing-millstone-unmillable` | The millstone's handler refuses 19 dirt (no milling recipe) and takes 2 cobblestone | Took the dirt and held it | `MillstoneBlockEntity.java:236`, `canProcess` (:211) |
| `processing-crusher-output-down`, `-north`, `-south` | Results leave 0.55 past the controller's middle along its facing (y − 0.55 facing down), tagged `BypassCrushingWheel` so its collision lets them through | Started clear of the controller: y − 0.8 facing down, 0.68 sideways | `CrushingWheelControllerBlockEntity.java:130`, `:193`; `CrushingWheelControllerBlock.java:169` |

The driver spilled a broken millstone's slots as `Block.popResource` does; the game spills a
mod container as `Containers.dropContents` does, as Create's `destroy` (:144,
`ItemHelper.dropContents`) does, so seeds 973 and 1485 differed whenever Java's items landed
outside the narrower range.

Open, found with the breaking families (a seed reproduces each with `fuzz gen <seed>` and the
lane; `breaking/drill` unless said):

| Difference | Java | Port | Deciding Java | Port code |
| --- | --- | --- | --- | --- |
| First look late (93: every seed that breaks) | A breaker looks on its first tick: with nothing to break `NextTick` goes to −1 at tick 2, and stone is cracked from tick 4 | Starts at −1 and is set to 1 by its first lazy tick: tick 2 shows 0 and the cracks come at tick 3, one tick early; two ticks late into a network that did not change in the tick the breaker was placed | `BlockBreakingKineticBlockEntity.java:26`, `:98-101` | `engine/src/drill.rs:16`, `:36-41`; `glue/processing/saw.rs:73-76` |
| No neighbour arming (1629) | A drill beside any block change looks two ticks later: a stone arriving at 42 is cracked from 44 | Waits for its next lazy tick (46) | `DrillBlock.java:80-84` | `engine/src/world.rs:3081`; `glue/processing/saw.rs:41` |
| Saw re-arm (7872 of `breaking/saw`) | An idle saw sets itself to 1 every tick, so it looks again two ticks after a break or an arrival: at the case's last tick, 73, its `NextTick` is 0 | Every lazy tick: `NextTick` stays −1 | `SawBlockEntity.java:170-173` | `glue/processing/saw.rs:73-76` |
| Speed 0 on the first tick (3165; of the 16 rpm seeds whose target never changes later, the only difference) | A breaker placed into a turning network has its speed on its first tick: `NextTick` −1 at tick 2 | `tick_drills` runs before the kinetic solve and reads speed 0: its lazy tick arms it to 1 instead. A generator that changes in the same tick re-places the network and hides this, which is why only the motor's default speed shows it | `KineticBlockEntity.java:99-100`, `:102` | `glue/processing/saw.rs:70`; `engine/src/world.rs:2839` |
| Cracks left (861) | A drill removed at 21 while it cracks takes its cracks along | They stay to the end of the case | `BlockBreakingKineticBlockEntity.java:78-83` | `engine/src/world.rs:2060`; `glue/processing/saw.rs:46` |
| Reload forgets progress (1373) | A reload at 21 keeps `Progress` and `NextTick` | Starts over (tick 22: 0 and 0 where Java holds 1 and 607) | `BlockBreakingKineticBlockEntity.java:59-76` | `engine/src/world.rs:470`, `:2377` |
| Lazy tick stopped | A stopped breaker's lazy ticks stay on its block entity's clock (66, 77, 88 after its restart) | Its counter counts turning ticks only (70, 81, 92); no lane history runs long enough to show it | `SmartBlockEntity.java:74-86` | `glue/processing/saw.rs:41`, `:73`; `engine/src/world.rs:2690-2696` |

With Java's arming, reload and crack clearing laid over the port's engine, a throwaway driver
matched Java on 360 of the first 432 drill seeds and 284 of the first 360 saw seeds; the rest
differed by the speed-0 row alone.

## Not driven yet

| Machine | Java | Why not yet |
| --- | --- | --- |
| Mixer over a basin | `kinetics/mixer/MechanicalMixerBlockEntity` (`tick`, :126: 40-tick cycle, `processingTicks` :157 from speed and the recipe's time), `processing/basin/*` (`BasinBlockEntity.tick` :344, `BasinRecipe.match` :39 and `apply` :61, heat from the block below :618, spout output :376) | The Java snapshot reports basins (inputs, outputs, tanks, filter) and mixers (`running`, `runningTicks`, `processingTicks`). The driver reports the port's mixer cycle (idle: it cannot place a basin, the only thing that starts one) and does not mirror `glue/processing/basin.rs` yet |
| Press over a basin | `MechanicalPressBlockEntity.tryProcessInBasin` (:83), `PressingBehaviour.tick` (`CYCLE` 240) | As the mixer: the basin |
| A drill in a cobble generator | `kinetics/drill/DrillBlockEntity.onBlockBroken` (:41), `CobbleGenOptimisation` | Lava and water sources around a drill with a belt, hopper or chute under it; lava is not among the fluids the harness places |
| Dynamic Trees | `saw/TreeCutter.canDynamicTreeCutFrom` (:51), `findDynamicTree` | That mod is not installed |
| Deployer | `kinetics/deployer/DeployerBlockEntity.tick` (:173), `DeployerHandler` | A fake player's item use |
| Mechanical arm | `kinetics/mechanicalArm/ArmBlockEntity.tick` (:128), interaction points | Placement selection packets and points |
| Mechanical crafter | `kinetics/crafter/MechanicalCrafterBlockEntity.tick` (:255) | Grids, connected inputs and crafting recipes |
| Fans on entities | `kinetics/fan/AirCurrent`, `fan/processing/FanProcessing` | Item entities in flight, which the harness discards |
