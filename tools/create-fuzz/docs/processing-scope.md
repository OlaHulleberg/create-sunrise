# Processing scope

What the processing machines are in Create 6.0.11 (`b62e837`), as its Java says, and how much of
them the fuzzer drives. Paths are under `src/main/java/com/simibubi/create/content/`. The
[kinetics scope](kinetics-scope.md) left this machine work to its own subsystem; the machines'
shafts and cogs stay in the kinetic catalogue.

## Driven

Five families, one residue each modulo 256 (`rust/src/processing.rs`, the two basin ones in
`rust/src/basin.rs`). None reloads.

| Family | Residue | Machine |
| --- | --- | --- |
| `processing/millstone` | 205 | A millstone on a creative motor |
| `processing/crushing` | 221 | Two crushing wheels on two motors and the controller between them |
| `processing/saw` | 128 | An upward saw on a motor, a depot beside it or nothing |
| `processing/mixing` | 77 | A basin under a mechanical mixer on a motor, heat below the basin or nothing, a depot beside the heat or nothing |
| `processing/compacting` | 93 | A basin under a mechanical press on a motor, a depot beside it or nothing |

They took residues from `geometry/states` (205, 221, 77 and 93, which leaves it 109 and 125) and
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

## Not driven yet

| Machine | Java | Why not yet |
| --- | --- | --- |
| Drill and sideways saw | `kinetics/base/BlockBreakingKineticBlockEntity` (`tick` :86, `lazyTick` :43), `saw/TreeCutter` | Block breaking with hardness, drops and tree felling |
| Deployer | `kinetics/deployer/DeployerBlockEntity.tick` (:173), `DeployerHandler` | A fake player's item use |
| Mechanical arm | `kinetics/mechanicalArm/ArmBlockEntity.tick` (:128), interaction points | Placement selection packets and points |
| Mechanical crafter | `kinetics/crafter/MechanicalCrafterBlockEntity.tick` (:255) | Grids, connected inputs and crafting recipes |
| Fans on entities | `kinetics/fan/AirCurrent`, `fan/processing/FanProcessing` | Item entities in flight, which the harness discards |
