# Processing scope

What the processing machines are in Create 6.0.11 (`b62e837`), as its Java says, and how much of
them the fuzzer drives. Paths are under `src/main/java/com/simibubi/create/content/`. The
[kinetics scope](kinetics-scope.md) left this machine work to its own subsystem; the machines'
shafts and cogs stay in the kinetic catalogue.

## Driven

Seven families, one residue each modulo 256 (`rust/src/processing.rs`, `rust/src/breaking.rs`,
the two basin ones in `rust/src/basin.rs`). None reloads.

| Family | Residue | Machine |
| --- | --- | --- |
| `processing/millstone` | 205 | A millstone on a creative motor |
| `processing/crushing` | 221 | Two crushing wheels on two motors and the controller between them |
| `processing/saw` | 128 | An upward saw on a motor, a depot beside it or nothing |
| `processing/mixing` | 77 | A basin under a mechanical mixer on a motor, heat below the basin or nothing, a depot beside the heat or nothing |
| `processing/compacting` | 93 | A basin under a mechanical press on a motor, a depot beside it or nothing |
| `breaking/drill` | 64 | A drill on a creative motor against the block in front of it |
| `breaking/saw` | 192 | A sideways saw on a motor against what stands in front of it |

They took residues from `geometry/states` (205, 221, 77 and 93, which leaves it 109 and 125) and
`geometry/stress_boundary` (128, 64 and 192), so those two families keep reaching every state and
load step over more seeds. The two breaking families are below.

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

### Basins under a mixer or a press

`processing/basin/BasinBlock`, `BasinBlockEntity`, `BasinInventory`, `BasinRecipe` and
`BasinOperatingBlockEntity`; `kinetics/mixer/MechanicalMixerBlockEntity`; `kinetics/press/
MechanicalPressBlockEntity` two blocks above a basin. Blocks `create:basin[facing]` (down, north,
south, west, east), `create:mechanical_mixer`, `create:mechanical_press`; the heat below the basin
is `create:blaze_burner[blaze]`, a magma block or a lit campfire (`BasinBlockEntity.getHeatLevelOf`,
:618).

- State: nine input slots and nine output slots, whose item handler on every side is both
  (`CombinedInvWrapper`, :116); two input tanks and two output tanks of 1000, the fluid handler
  being the outputs then the inputs (:154-162); a recipe filter; the spout's waiting items and
  fluids (`spoutputBuffer`, `spoutputFluidBuffer`) and the sides the wrench chose and shut.
- `tick` (:344), in the basin's block entity turn: what waits at the spout goes out, then its
  operator is told that the contents changed (`basinChecker.scheduleUpdate`, :361).
  `SmartBlockEntity.initialize` calls a basin's `lazyTick` at its first tick, and its counter (10)
  every 11 ticks after; `lazyTick` (:253) turns the spout (`updateSpoutput`) and, on its 21st call
  and every 21st after (219 ticks in, then 231 apart), has a non-empty basin looked at again
  (`recipeBackupCheck`, :258).
- `BasinRecipe.match` (:39): the filter on the result, the heat and the ingredients, items and
  fluids, which `apply` (:61) takes one by one from the first slot that has one. Results go to the
  output slots and tanks, or, the basin facing a side, to the spout's buffer (`acceptOutputs`,
:523).
- Inputs: an item landing on it (`BasinBlock.updateEntityAfterFallOn`, :133), the item handler
  (`BasinInventory.insertItem`, :20: one stack of an item in a basin, and only into the first empty
  slot; `extractItem`, :45: any slot), and `useItemOn` (:88): a container's fluid goes into the
  input tanks when all of it fits, a container is filled from the outputs first, an empty hand
  takes everything and stops the operator (`onEmptied`, :237).
- `onWrenched` (:280): the spout's side is chosen, or shut when it is the one faced. Facing a
  side, results go to what takes items below it; facing down, into the output slots.
- The mixer (`tick`, :126): 20 ticks down, `processingTicks` (:157: from the speed and the
  recipe's time, left as it is by `startProcessingBasin`, :272) at the bottom, 20 ticks up. It goes
  on with the same recipe while the spout was free and the recipe still fits
  (`BasinOperatingBlockEntity.applyBasinRecipe`, :110). The press compacts at the lowest point of
  its cycle (`tryProcessInBasin`, :83) with the compacting recipe, or the square crafting recipe of
  one item (`canCompress`, :189) that has the most ingredients.
- The families: three batches of items two ticks apart, dropped on the basin or put through its
  item handler into any slot or the first; buckets of water and milk emptied into it before them;
  heat from nothing, a smouldering burner, magma, a lit or an unlit campfire, a burner fed a blaze
  cake or a creative one before the items or after they wait, or one placed seething; between the
  batches outputs taken by handler, the basin emptied by hand, the motor stopped or slowed, the
  basin removed, the wrench on the spout, a bucket filled; the basin and its operator placed in
  either order; and a depot the basin spouts into in 30% of the seeds.
- Observed: the basin's slots, tanks and filter; the mixer's `running`, `runningTicks`,
  `processingTicks`; the press's `running`, `runningTicks`, `mode`; the depot's stack and what is
  dropped.
- Not driven: the basin's filter (a basin's filter has no count board, so `filter_setting` does
  not reach it); potions (`PotionMixingRecipes`, :229: a potion is a component stack, outside the
  contract); items from belts and funnels (the driver mirrors `processing::belt_input` for a basin,
  no family feeds one); a spout pouring into a basin; results to a belt, a funnel or a drain; an
  operator whose kinetic neighbour's first lazy tick (:10 ticks after its placement) meets an
  input.

### Recipes

`rust/src/processing.json` lists the recipes the families' inputs reach: Create's milling
(wheat, cobblestone, gravel, andesite, bone, sugar cane), crushing (obsidian, gravel, coal ore)
and the two cutting recipes `RuntimeDataGenerator.cuttingRecipes` makes for oak logs; dirt has
none. `ProcessingRecipeAudit` checks every entry against the loaded recipe and that no other
milling, crushing, cutting or stonecutting recipe takes a family's input for its machine
(stonecutting is on for saws by default, `allowStonecuttingOnSaw`), so the driver's recipe order
cannot differ from Create's.

`rust/src/basin.json` lists what the basin families' basins can hold and run: Create's mixing and
compacting recipes, the shapeless crafting ones a mixer runs (`allowShapelessInMixer`) and the
square crafting ones a press compacts (`allowShapedSquareInPress`), each ingredient as the items
of its family it takes. An operator takes the matching recipes from a recipe trie in its
traversal's order, those with the most item ingredients first (`BasinOperatingBlockEntity`,
:150-165). The driver asks the port's trie (`create_engine::basin::matching`), which walks its
nodes in ascending id order where Java's walk follows its hash tables: recipes with as many item
ingredients as each other could come out in another order. `BasinRecipeAudit` checks every entry
against the loaded recipe and that
no recipe of these kinds is missing whose ingredients a family's basin can come to hold: its
inputs, its recipes' results and the containers these and what they use up leave.

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
- `write` and `read` (:59) save `Progress`, `NextTick` and `Breaking`; `invalidate` (:78) clears
  the cracks of
  a breaker that had progress.

The comparison reads a breaker's `Progress` and `NextTick` (`breaking`) and the cracks it last
asked the level to show at the block (`cracks`: 1.21.1 keeps no crack map on the server); after a
reload, from the fields its `read` restored, as the Java side does.

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
wrenched while it cracks. `breaking/saw` puts a sideways saw against sixteen: a log, trees it
fells (centre out or rim in), a cut a second trunk holds up, a branch, mangrove roots, a mushroom
stem among caps and a shroomlight, bamboo, a pumpkin or melon, what a saw cannot cut, a crimson
stem among wart blocks, a tree with bee nests and moss carpet on it, leaves alone, the saw or its
motor removed, `doTileDrops` off, and the saw wrenched while it cuts. Neither family reloads by
seed bit.

Not compared: experience (the harness discards the orbs, as it does the drops it never observes);
the items chance decides (a leaf's sticks, saplings and apples, gravel's flint, a melon's seeds, a
mushroom block's mushrooms), which both sides leave out wherever the block that rolls them shows;
sounds and particles. A version 3 event `{"kind":"game_rule","do_tile_drops":false}` turns the
rule off from its tick to the case's end; the server puts its own rule back after the case.

Cases in `cases/`: `breaking-saw-upward`, `breaking-drill-unbreakable`, `breaking-drill-water`,
`breaking-mushroom-skins`, `breaking-leaf-distances`, `breaking-sand-falls`,
`breaking-sand-column-pull`, `breaking-first-look`, `breaking-neighbour-arm`,
`breaking-saw-rearm`, `breaking-placed-16`, `breaking-placed-64`, `breaking-cracks-after-removal`,
`breaking-reload-mid`, `breaking-lazy-while-stopped`. Every history where a breaker breaks a block
differed from Java by a row of the table below, so those histories are in the two families, and
shrunk in the cases that pin each fix.

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

Fixed on `main` (#290, #296, #291, #297, #289), each kept by its case here:

| Case | Java | Port before | Deciding Java |
| --- | --- | --- | --- |
| `processing-basin-mixer-timing` | A mixer's first time at the bottom takes one tick more (`processingTicks` starts at 0), and one resumed after a pause keeps its count | The count was reset at every start | `MechanicalMixerBlockEntity.java:55`, `:149`, `:172`, `:272` |
| `processing-basin-handler` | The handler takes a stack only into the first empty slot and not beside a stack of the same item; it gives from the input slots | Took into any empty input slot; gave nothing from the inputs | `BasinInventory.java:20`, `:45`, `BasinBlockEntity.java:116` |
| `processing-basin-bucket` | A bucket emptied into the tanks, and one filled from them, is back in the hand | The bucket went to the inventory | `FluidHelper.java:141`, `:185` |
| `processing-basin-spout-turn` | The basin turns its spout 11 ticks after it was placed, then every 11 (`SmartBlockEntity` calls `lazyTick` at `initialize`, then every `rate + 1` ticks) | Every 10 game ticks: the tick before | `SmartBlockEntity.java:64`, `:80`, `BasinBlockEntity.java:257` |
| `processing-basin-spout-pop` | A broken basin drops the items waiting at its spout | They were lost with the block | `BasinBlockEntity.java:228` |

Fixed on `main` after that (#315, #318), each kept by its case here:

| Case | Java | Port before | Deciding Java |
| --- | --- | --- | --- |
| `processing-basin-press-start` | A press placed before its basin starts the tick after the items arrive; a mixer rests a tick between two cycles | Started at once, and restarted at once; placed after its basin, a mixer started a tick late | `BasinBlockEntity.java:361` in the basin's turn, `DeferralBehaviour.java:40`, `BasinOperatingBlockEntity.java:58`, `MechanicalMixerBlockEntity.java:129` |
| `processing-basin-heater-lit` | A burner lit under a basin has its mixer look at the basin at once | Only at the next backup check: every 200 game ticks | `BlazeBurnerBlock.java:88`, `BasinBlockEntity.java:258` |
| `processing-basin-press-removed` | A basin taken away stops its press at once | The cycle ran on | `BasinBlockEntity.java:232-239`, `BasinOperatingBlockEntity.java:59-61`, `MechanicalPressBlockEntity.java:232` |
| `processing-basin-press-restart` | A press starts its next cycle in the tick one ends while its recipe still fits | One tick later | `MechanicalPressBlockEntity.java:171` |
| `processing-basin-continue` | A mixer goes on with its recipe only when nothing waited at its spout before it applied | Went on whatever waited there | `BasinOperatingBlockEntity.java:118`, `:125` |
| `processing-basin-recipe-order` (with the bucket row: its water comes from a bucket) | With dirt, water, mud and wheat in the basin the mixer packs mud first: `getMatchingRecipes` sorts by item ingredients alone, and `minecraft:packed_mud` has two to `create:mixing/mud_by_mixing`'s one | Sorted by item and fluid ingredients together, a tie the mixing recipe won | `BasinOperatingBlockEntity.java:153`, `:165` |

Open, with a case in `cases/` that differs until the port matches (the driver mirrors the port, so
`./run.sh fuzz check cases/processing-basin-*.json` shows it):

| Case | Java | Port | Deciding Java |
| --- | --- | --- | --- |
| `processing-basin-emptied` | A mixer whose basin was emptied while it ran rests in that same turn: `onBasinRemoved` sends its head to 40, and its own `tick`, which goes on after `BasinOperatingBlockEntity.tick` returned, rests it at once | `runningTicks` stays at 40 until the mixer's next turn | `BasinOperatingBlockEntity.java:58-67`, `MechanicalMixerBlockEntity.java:126-134`, `:287-292` |

Three more differences show in the sweeps alone, none with a case here (against `main` at
`a2c1ae1d`: `fuzz-lane 0 327680 77 256`, 1170 matched and 110 differed; `fuzz-lane 0 327680 93 256`,
1141 and 139; `fuzz 0 4000`, 3990 and 10):

- A spout's results reached what takes them a tick later than in Java whenever the basin's turn
  followed its operator's (fixed, #346): the basin's own `tick` puts them out
  (`BasinBlockEntity.java:355`), and the port now does it at the basin's turn in the walk
  (`TransportTick::Basin`, `spoutput::turn`).
- The mixer's rest of `processing-basin-emptied` after its basin was taken away rather than
  emptied (`BasinBlockEntity.java:232`): 39 seeds of the first sweep, beside the 40 that empty it.
- The backup check, in 4 seeds of the second sweep where an empty hand leaves a press's basin
  with fluid it could compact: the press starts again at the basin's 21st lazy tick in Java and at
  the port's next 200th game tick (`glue/processing/basin.rs`, `backup_check`), 20 ticks apart in
  the smallest one (seed 217949 shrunk: 219 against 199).

Two differences of the port cannot be seen here. A belt handing a stack to a basin is told all nine
inputs are room (`glue/processing/mod.rs`, `belt_input_room`) where Java's `insertItemStacked` asks
`BasinInventory.insertItem` (`DirectBeltInputBehaviour.java:76`); the real insertion refuses the
same, so the stack waits at the belt's end on both, and a full stack, a stack of 60 and a full one
with another item beside it all match. A broken basin's waiting items appear at the block's middle
(`glue/processing/mod.rs`, `spill_removed`) where `Block.popResource` scatters them
(`BasinBlockEntity.java:228`), and drops compare within the range their origin spreads them over.

A depot merges nothing into its stack (`DepotBehaviour.allowMerge`, set only by
`enableMerging`), so a basin spouting into one holds its second result at the spout until the
first leaves it.

Fixed after the breaking families found them (#384, #385, #386, #387), each with an engine test
carrying Create's numbers and a case in `cases/`:

| Case | Java | Port before | Deciding Java |
| --- | --- | --- | --- |
| `breaking-first-look`, `breaking-placed-16`, `breaking-placed-64` | A breaker looks on its first tick, with the speed it attached with: with nothing to break `NextTick` is −1 at tick 2, and stone is cracked from tick 4 | Started at −1 and was armed by its first lazy tick: a tick early in the families, two late into a network that did not change in the tick the breaker was placed; and it read speed 0 on its first tick, as the breakers ticked before the solve | `BlockBreakingKineticBlockEntity.java:26`, `:86-101`, `KineticBlockEntity.java:98-102` |
| `breaking-neighbour-arm` | A drill beside any block change looks two ticks later: a stone arriving at tick 42 is cracked from 44 | Waited for its next lazy tick (46) | `DrillBlock.java:80-84` |
| `breaking-saw-rearm` | An idle saw sets itself to 1 every tick, so it looks again two ticks after a break or an arrival | Looked again at its lazy ticks only | `SawBlockEntity.java:170-173` |
| `breaking-cracks-after-removal` | A drill removed at 21 while it cracks takes its cracks along | They stayed to the end of the case | `BlockBreakingKineticBlockEntity.java:78-83` |
| `breaking-reload-mid` | A reload at 21 keeps `Progress`, `NextTick` and `Breaking`, and the block drops at tick 31 | Started over (tick 22: 0 and 0 where Java holds 1 and 607), and dropped at 39 | `BlockBreakingKineticBlockEntity.java:59-76` |
| `breaking-lazy-while-stopped` | A stopped breaker's lazy ticks stay on its block entity's clock (66, 77, 88 after its restart) | Its counter counted turning ticks only (70, 81, 92) | `SmartBlockEntity.java:74-86` |

After the fixes, each lane's first 1280 seeds (`fuzz-lane 0 327680 <residue> 256`):
`breaking/drill` 1280 matched and `breaking/saw` 1267 matched, 13 differed; `fuzz 0 4000` matched.
The 13 are the saw's leaves-alone history at −48 and 256 rpm, and differ in the `distance`
of vanilla leaves alone: on the server the leaves around a cut one recompute their distances in
another order than the driver's model of them does (`engine/vanilla.rs`, `leaf_tick`), as they did
before the fixes. The smallest is one leaf (seed 23488 shrunk: at tick 12 `3,14,-1` is 6 on Java and
7 on the driver). A wrench on a drill does not arm it: `IWrenchable.onWrenched` runs the drill's own
`updateShape` and its neighbours' `neighborChanged`, never the drill's own.

## Deployers

`kinetics/deployer/`: `DeployerBlock`, `DeployerBlockEntity`, its `DeployerFakePlayer`,
`DeployerHandler` (what the fake player does to the block it reaches), `BeltDeployerCallbacks`
(a deployer facing down over a belt or a depot), `DeployerItemHandler`, and the item that places
it (`processing/AssemblyOperatorBlockItem`), with the two mixins that bend block item placement
and block drops to the fake player (`foundation/mixin/BlockItemMixin`, `BlockMixin`).

### Deployers: driven

One family, `deployers` (`rust/src/deployers.rs`), on residues 64, 125 and 253 mod 256. A seed
≡ 64 never reloads (`seed & 32` is clear), the others always do. Nine histories, picked by
`index / 3`; the table gives the first seed of each:

| History | Seed | What happens |
| --- | --- | --- |
| depot | 64 | Facing down over a depot. A click on its front gives the hand (a `(stack, tool)` pair of the contract, or dirt and planks, which no recipe takes), stacks are dropped under it, the wrench turns it to punch and back, a signal beside it locks it, the hand is taken back and another given mid-cycle, and a handler or an empty hand takes what the depot holds |
| belt | 320 | The same over a belt of 4 to 6 segments at ±32 rpm, built with the connector; the stacks land on its first segment |
| placing | 576 | A block item in its hand (planks, cobblestone, dirt, an andesite casing, a honeycomb block) on every facing: its block appears two ahead, stands there from the start, or a stone is in the way two or one ahead; it goes, and may come back |
| punching | 832 | Punch mode on stone, cobblestone, dirt, planks, a depot or bedrock two ahead, with nothing, a stick or a pickaxe in hand; the block changes to air, dirt, stone or itself |
| tools | 1088 | An axe on a log, a shovel on grass, a hoe on dirt or grass, an andesite alloy, a brass or a copper ingot on a stripped log, an inert pickaxe or ingot; the block is set back and another tool comes |
| fist bump | 1344 | Two punchers two to four blocks apart, the second wrenched to punch or left using, its motor at the same speed or at 1 rpm (a board set to 0) |
| filter | 1600 | The depot history with a filter on its slot, naming its tool (it works) or iron ingots (the tool is put away); the filter is cleared again |
| swapping | 1856 | The hand taken back and another given, stacks under it, the hand swapped by clicks |
| removed | 2112 | The deployer taken while empty and put back, its motor slowed to 1 rpm and set again, the other way or not, its depot taken under a stack it holds and put back |

The contract (`rust/src/deployers.json`) names the 21 full states a deployer may reach, the 33
items it may hold (a block item names the block it places; an axe, a shovel or a hoe the blocks
it turns), the hardness and outline top of 10 blocks, what one punch does for each of 45 (held,
block) pairs (the progress, whether it harvests, the drops), and the 11 recipes the pairs meet (7
deploying, 3 item application, 1 sandpaper polishing). `DeployerAudit` checks every entry at
startup with the game's own fake player, by experiment in a probe area of slot 127, and fails the
start on a mismatch.

Version 3 snapshots report each deployer (`DeployerObservations`, `deployers::Observation`):
`mode`, `state`, `timer`, `reach`, the hand (`held`: item, count, damage), the fake player's other
slots (`inventory`), `overflow`, `locked`, whether it has an `owner` (a player placed it), the
filter item, `fist_bump`, and the block it is breaking with its progress. The Rust driver
(`rust/src/engine/deployer.rs`) re-implements the port's glue (`create/src/glue/processing/deployer.rs`,
`fist.rs`, `filtering.rs`, `operator.rs`) on the port's engine (`create/src/engine/src/deployer.rs`),
in the port's order, as the other families' drivers do; it takes no rule from Create's Java.

What the Java does, as measured:

- `tick` (`DeployerBlockEntity.java:172`): still, nothing. `timer > 0`: down by
  `clamp(|speed| × 2, 8, 512)`. At rest, holding overflow, or holding what its filter refuses
  (which goes to the overflow), or using over a block it should not activate on: `timer =
  timerSpeed × 10` and return, so it looks again 11 ticks later at any speed. A puncher looks 2 to
  4 blocks ahead for another facing it and punching: both expand at once, `reach = (distance − 2)
  × 0.5`, and retract together when the first runs out (`startFistBump`, `triggerFistBump`).
  Locked by a signal, it starts nothing. Else `start()`, and the stroke counts from the next tick.
- `start` (:258): a ray from 1.5 to 2.5 blocks ahead gives `reach = 0.5 + min(hit, 0.75)`: 1.25 in
  the air, 0.6875 over a depot's or a belt's top, 0.5625 over farmland's, and 0.501 against a full
  block whose face the ray starts on (the game's `clip` puts that hit a thousandth in: stone, dirt
  or a depot seen from the side). `reach` is synced to clients but not saved (:389, :422): 0 after
  a reload.
- `shouldActivate` (`DeployerHandler.java:120`): a block item over its own block, a bucket over
  its own fluid, or anything held over a depot or belt starts no stroke by itself. An empty hand,
  or a held stack over anything else, strokes on; over a depot or a belt `activate` then returns
  at once (:351).
- Over a depot or a belt (`BeltDeployerCallbacks.java`): a stack arriving under a deployer at rest
  starts its stroke when a recipe takes the stack with the hand, and passes otherwise; one
  arriving mid-stroke is held whatever it is (:48). A held stack is asked every tick: without a
  recipe it is let go (:80); the recipe applies in the tick the stroke bottoms (`RETRACTING`,
  timer 1000, :83); a deployer that comes to rest starts a new stroke for it, unless locked (:88).
- A punch (`DeployerHandler.java:241`): `getDestroyProgress × 16` a stroke, as the fake player is
  never on the ground and digs at a fifth of a player's rate: an iron pickaxe takes stone in three
  strokes (0.427 each); without a held stack nothing breaks. A broken block's drops go into the
  fake player's inventory (:416).
- A use (:286): the hand's `useOn` on the block two ahead. A block item places there, or one block
  nearer when that is taken, and nowhere when the clicked block cannot be replaced
  (`BlockItemMixin`). An axe, a shovel or a hoe turns the block, a shovel not from below.
- By hand: the wrench on its front half changes the mode; anywhere else it turns the deployer as
  any wrenchable block, and sneaking picks it up. A click on its front swaps the hand and the held
  stack. A click on the side that holds its filter slot sets or clears the filter. Its item, used
  on top of a belt, a basin, a depot or a weighted ejector, places it two above, facing down;
  a depot takes the item instead unless the player sneaks. Clicked on a deployer, it places the
  next one beside it, on the free side nearest the click.

### Deployers: not driven, and why

- Entities: a use on one, a punch, food and drinks (`finishUsingItem`), spawn eggs, carts and
  rails. The harness has no entities but items, which it discards in flight.
- Buckets and flint and steel from the fake player (`ItemUseWorld`, the bucket's own ray): fluid
  item flight is outside the item transport ([item transport](item-transport.md)).
- Sequenced assembly's deploying steps: their in-between items carry components.
- Tools with enchantments or a name: the hand's components.
- `DeployerRecipeSearchEvent`: other mods cancel recipe lookups there; none is installed.
- `DeployerMovementBehaviour`, `DeployerMovingInteraction`: a contraption (out of scope).
- The filter's amount (`filter_setting`): a deployer's filter matches items only
  (`FilteringBehaviour.test`, :236); the amount changes nothing it does.
- The owner's identity beyond "player placed it": the fake player's UUID and name are its owner's
  (`DeployerFakePlayer.java`, `getUUID`), which no harness action reads.
- A mechanical arm filling a deployer's hand: the arm family is not driven.
- `clearContent` (the game's empty-command path) and `invalidate`.

### Deployers: port differences found

Open, against `main` at `ed78c18e`. `cases/` holds only cases that match on `main` (of the 47
written for deployers, the five that do: an item used on a depot, a deployer item refused above a
taken belt, the wrench on a deployer's back, sneaking with it, and a deployer removed in the tick
it is placed), so a difference's repro is the
shrunk scenario named in the first column, kept with the report rather than here; the ticks in
brackets are its checkpoints. The driver mirrors the port, so each shows the port's own answer.
The three sweeps agree with the cases: `fuzz-lane` on each residue (427 seeds each, 1281 in all)
matched 0 and differed 1281, none java-unstable or driver-unsupported; one seed of each history
is shrunk beside the cases: 64, 320, 576, 832, 1088, 1344, 1600, 1856 and 2112.

| Repro | Java | Port | Deciding Java | Port code |
| --- | --- | --- | --- | --- |
| `deployer-punch-stone` (85), `deployer-fist-bump` (86-94), `deployer-placement` (86-96) | A stroke started, and each wait to look again, counts from the next tick: 1000 on the tick a stroke starts, 968 on the next at 16 rpm; a fist bump's initiator keeps 1000; the look-again period is 11 ticks (320 at 96 where the port is at 256) | The start tick counts: 968 on it, so every stroke after the first runs a tick early, and the period is 10 ticks | `DeployerBlockEntity.java:234-235`, `:222-226`, `:184-187`, `:229-230`, `:299-305` | `glue/processing/deployer.rs:427-441` starts or waits, then `:446-472` ticks the same deployer in the same tick |
| `deployer-stroke` (2), `deployer-reach` (3) | Its first tick with a speed is its second tick, and its first stroke starts there (`expanding 1000`) | Still `waiting 0` there: the deployer's pass reads no speed before that tick's solve (#370); it starts on the next tick and counts it. From then on the two errors cancel | `KineticBlockEntity.java:98-99` (`attachKinetics` in its own `tick`, before the subclass) | `glue/processing/mod.rs:615-621`, `glue/processing/deployer.rs:372-381` |
| `deployer-filter-mismatch` (8), `deployer-removed-held` (10), `deployer-lock` (70), seed 320 | A using deployer over a depot or belt strokes on its own with an empty hand or a hand `shouldActivate` lets (`expanding 840` at tick 8 over an empty depot), and holds what arrives until its stroke bottoms; a stack under a deployer that was already retracting is let go when it comes to rest, without a recipe (the shaft x8 at 70); over a belt a held stack's timer counts `timerSpeed × 10` down (5120 to 0 on seed 320) | It starts strokes only from the depot's and the belt's callbacks: at rest its timer stands still (`waiting 0`; `waiting -12` from tick 140 on, on seed 320), and the shaft under `deployer-lock` is made a cogwheel at 74 by the stroke its arrival started, though the deployer was locked at 60 | `DeployerBlockEntity.java:221-235`, `:351-353`, `DeployerHandler.java:133-137` | `glue/processing/deployer.rs:412-415` |
| `deployer-belt-deploying` (72, 105-106) | A belt formed after its deployer ticks after it: a held stack's recipe applies in the tick the stroke bottoms (105), and a stack held under a deployer that just came to rest starts its stroke at once (72: `expanding 1000`) | Belts and depots always come first: `waiting 500` at 72, the recipe at 106 (see #370) | `BeltDeployerCallbacks.java:83-92`, block entities in placement order | `glue/processing/mod.rs:324` |
| `deployer-reach` (3), `deployer-tools` (100), `deployer-punch-stone` (85), `deployer-axe-log` (10-15) | `reach` follows the block two ahead: 0.501 against stone, dirt or a log with air between, 0.5625 over farmland's top | 1.25 whenever the block one ahead is replaceable; with a solid block one ahead too, 0.5 where Java has 0.501 | `DeployerBlockEntity.java:258-272` | `glue/processing/deployer.rs:422-426` |
| `deployer-punch-stone` (200), seeds 832, 893 | The broken stone's cobblestone is in the fake player's inventory, then in the deployer's overflow for a hopper or funnel to take; no item entity | An item entity at the block's middle (`pop_resource`) | `DeployerHandler.java:416-417`, `DeployerBlockEntity.java:200-219` | `glue/processing/deployer.rs:1106-1109` (`PlayerAction::BreakBlock`) |
| `deployer-removed-held` (41) | The held stack is dropped by the fake player where it stands, in the block in front, with a throw (0.74, 9.36, 0.81) | Dropped at the deployer's middle (0.5, 10.375, 0.5) without a throw | `DeployerBlockEntity.java:466-474` | `glue/processing/mod.rs:109-117` (`drop_slot`) |
| `deployer-handler` (11-16) | The handler's slot 0 is the hand: 3 planks then 2 more are taken (5), dirt refused, one placed at 35, 2 and then the rest extracted | The container has the 11 overflow slots only: both inserts refused whole, nothing placed, nothing to extract | `DeployerItemHandler.java:49-118` | `glue/processing/containers.rs:145-157` |
| `deployer-placed-by` (11), `deployer-placed-operator-depot` (11) | A player-placed deployer has an `owner`, saved as `Owner`; its fake player answers with that UUID | None; one fixed fake player | `DeployerBlock.java:86-90`, `DeployerBlockEntity.java:406-407`, `DeployerFakePlayer.java:104-107` | `engine/src/deployer.rs:65-80` (no owner), `glue/processing/deployer.rs:35-36` |
| `deployer-placed-operator-basin` (11), `deployer-placed-operator-belt` (25) | Placed two above what it works on, it faces down, whatever the player's view | Faces by the player's view: up when not sneaking and looked at from above (pitch 60); `down` only when the player sneaks (`-depot`, which matches but for its owner) | `DeployerBlock.java:167-173`, `AssemblyOperatorBlockItem.java:37`, `:45-48` | `glue/processing/operator.rs:131-146` |
| `deployer-reload` (31) | `reach` is 0 after a reload: not saved | 1.25 kept | `DeployerBlockEntity.java:389-392`, `:422-425` | `glue/processing/persist.rs:407-440` |
| `deployer-places-deployer` (13) | A player's deployer item clicked on a deployer places a second one beside it, on the free side nearest the click (above it here); the hand stays empty | No placement helper: the click is the front's, so the first deployer takes the item in its hand | `DeployerBlock.java:104-108`, `:176-203` | `glue/processing/mod.rs:774-814` (`use_block`), `glue/processing/deployer.rs:1284-1323` (`use_deployer`) |

What matches, tick for tick apart from those: the stroke's progress, a punch's `getDestroyProgress × 16`
a stroke (stone goes in the third stroke of an iron pickaxe, the pickaxe wears), a fist bump
between two punchers two to four apart and one stopped mid-bump, a block item placed two ahead or
nowhere against a wall, an axe, a shovel and a hoe turning blocks, the recipes over a depot and a
belt, the filter slot, the wrench on the front and the back, and the hand swapped by a click.

## Not driven yet

| Machine | Java | Why not yet |
| --- | --- | --- |
| A drill in a cobble generator | `kinetics/drill/DrillBlockEntity.onBlockBroken` (:41), `CobbleGenOptimisation` | Lava and water sources around a drill with a belt, hopper or chute under it; lava is not among the fluids the harness places |
| Dynamic Trees | `saw/TreeCutter.canDynamicTreeCutFrom` (:51), `findDynamicTree` | That mod is not installed |
| Mechanical arm | `kinetics/mechanicalArm/ArmBlockEntity.tick` (:128), interaction points | Placement selection packets and points |
| Mechanical crafter | `kinetics/crafter/MechanicalCrafterBlockEntity.tick` (:255) | Grids, connected inputs and crafting recipes |
| Fans on entities | `kinetics/fan/AirCurrent`, `fan/processing/FanProcessing` | Item entities in flight, which the harness discards |
