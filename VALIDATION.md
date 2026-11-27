# Validation

Checked on 2026-11-25 on branch `create-sunrise-glsl-6a02076` (Linux x86_64, Rust 1.99.1), pinned to the Sunrise
Dev build at main `6a02076f89cd189d74d63492dda5b023fa85a38f` in `create/sunrise-mod.toml`. A Dev build is a test
build with no stable mod API. `cargo sunrise-mod` used the release tool and SDK of
`sdk-6a02076f89cd189d74d63492dda5b023fa85a38f` taken from the draft release (a local copy of the draft folder named by
`SUNRISE_SDK_BASE`), not from the published one; the game of the in-game runs is the draft Dev release `dev-2026-11-25-6a02076`.

In game on `6a02076`: see "Dev build `6a02076` in game" below. `tools/create-fuzz/` has not been run on `6a02076`.
Earlier in-game results below are historical, from the builds named in their headings.

## Checks

In a fresh clone run `cargo sunrise-mod check` first: it fetches the SDK into `create/.sunrise/`, and the other
`create/` commands fail without it (`no matching package named sunrise-mod-api`).

| Where | Command | Result |
|---|---|---|
| `create/` | `cargo +1.99.1 fmt --check` | Pass |
| `create/` | `cargo +1.99.1 check` and `cargo +1.99.1 clippy --all-targets -- -D warnings` | Pass |
| `create/` | `cargo +1.99.1 test` | Pass (124 glue tests) |
| `create/` | `cargo +1.99.1 test -p create-engine` | Pass (923 engine tests) |
| `create/` | `cargo sunrise-mod check` | Pass (draft `6a02076f89` SDK) |
| `create/` | `cargo sunrise-mod build` | Pass (native library and `.smod` package) |
| `create/` | `cargo sunrise-mod check target/sunrise-mod/create.smod --build 6a02076f89cd189d74d63492dda5b023fa85a38f` | Pass (`made for`) |
| `create/src/engine/` | `cargo +1.99.1 fmt --check` | Pass |
| `create/src/engine/` | `cargo +1.99.1 check` and `cargo +1.99.1 clippy --all-targets -- -D warnings` | Pass |
| `tools/create-data/` | `cargo +1.99.1 fmt --check` | Pass |
| `tools/create-data/` | `cargo +1.99.1 check --locked` and `cargo +1.99.1 clippy --locked --all-targets -- -D warnings` | Pass |
| `tools/create-data/` | `cargo +1.99.1 test --locked` | Pass (56 tests) |
| `tools/create-data/` | `cargo run --locked -- registry --out ../../create --check` | Pass |
| `tools/create-fuzz/rust/` | `cargo +1.99.1 fmt --check`, `cargo +1.99.1 clippy --release --all-targets -- -D warnings` | Not run on `6a02076` |
| `tools/create-fuzz/rust/` | `cargo +1.99.1 test --release` with `CREATE_HORIZON_DIR` set to the checkout | Not run on `6a02076` |
| `tools/create-fuzz/` | `./run.sh fuzz check cases/*.json` against a running server | Not run on `6a02076` |

## Deployers (#529) in game on Dev `ba1f24e` (main `7669a8e6`)

One run on 2026-11-27 of the Linux build of Dev `dev-2026-11-26-ba1f24e` (headless, software Vulkan, driven by the control
socket), with launcher 1.3.7. The mod is `git archive` of `create/` at `7669a8e6` plus the uncommitted
`data/c/tags/block/barrels.json` (#359), built by `sunrise-launcher native-build` as `native-ba1f24e-0877052fa42e`, in a
fresh creative world. Deployers were placed with `/setblock` and by the player; items were handed over by clicking the
deployer's front, the mode changed with the wrench, and the state read with `/data get` (`DeployerState`'s `Values`:
state, timer, reach, count, mode, locked, damage). One later build added temporary DEBUG lines (`eprintln!`, not
committed) to trace the last row. Details and screenshots are not kept in the repository.

| Area | Result |
|---|---|
| Punch with an iron pickaxe (#404) | Facing north on a 16 rpm motor, stone two ahead: within a few strokes it breaks it, the cobblestone goes to the fake player's inventory and then the overflow (`DeployerBlockEntity.tick`, `:200-219`), the pickaxe takes 1 damage, and no item entity is in the world. While the overflow holds it the deployer idles at a timer of 320 (`getTimerSpeed() * 10`, `:195-197`). A hopper below pulls the cobblestone and then the pickaxe: with no filter the hand can be extracted (`DeployerItemHandler.extractItem`, `:101-103`). |
| Punch with an empty hand | Strokes keep coming; the stone never breaks and nothing drops, as in Create (`DeployerHandler.java:257`, `if (stack.isEmpty()) return` before any progress). |
| Using deployer placing blocks | Holding 16 cobblestone, it places one two ahead and then stops while that block stays (`shouldActivate`, `DeployerHandler.java:120-124`). |
| Over a depot (#402) | With an empty hand it strokes over the empty depot. Holding andesite alloy, three stripped oak logs put on the depot by hand become three andesite casings, one alloy and one log per stroke (`item_application/andesite_casing_from_log`), the results in the depot's output (`BeltDeployerCallbacks.whenItemHeld` starts the stroke and `activate` applies the recipe to one of the stack at the start of `RETRACTING`, `convertToAndLeaveHeld`, `:65-131`). |
| Over a belt | Holding alloy, it waits over the empty belt (`shouldActivate` false: held item, facing down, a belt below). Two logs stop under it and are held, it strokes twice, and each casing rides on while the rest of the stack stays held until the last; 8 alloys become 6 (`whenItemHeld`, `activate`). |
| Locked by redstone | With a block of redstone placed beside it, `Values` reads locked: four logs and four alloys stay as they are; when the block is broken it resumes and makes casings (`redstoneUpdate`; on a depot or belt `BeltDeployerCallbacks.onItemReceived` and `whenItemHeld` return `PASS` while locked, `:50-51`, `:89-90`). |
| Reach (#403) | Hands caught at the start of `RETRACTING`: over farmland and over a depot the hand stops at their tops, two above the floor it reaches much further, and with stone two ahead it touches the stone's face. With stone directly in front and air two ahead, the hand goes through that stone, as Create's ray from 1.5 blocks ahead gives (`start()`, `:258-272`). `Reach` is synced, not saved, so `/data get` always reads 0 there; the exact values (0.5625, 0.6875, 0.501) cannot be read in game. |
| Placement two above (#408) | The deployer item, clicked sneaking on top of a depot or a basin (without sneaking the depot takes the stack, as `SharedDepotBlockMethods.onUse` does) and without sneaking on top of a belt, goes two above facing down (`AssemblyOperatorBlockItem.place`, `getFacingForPlacement`). |
| Placement helper (#408) | The deployer item clicked on a deployer places a second one beside it on the free side nearest the hit, with the same facing and axis: on the underside of a downward one, north of it; on the front of a horizontal one holding cobblestone, above it, and the cobblestone stays in the first deployer's hand (`DeployerBlock.useItemOn`, `:104-108`, `PlacementHelper`). A click on the middle of a side took its filter slot instead and set the filter to the held deployer stack, count 6 kept, as `FilteringBehaviour.onShortInteract` does (`toApply` is a copy of the hand, `setCount(1)` only for filter items, `:294-317`). |
| Owner (#407) | Every player-placed deployer reads `Owner` equal to the player's `UUID`; one placed with `/setblock` has none (`setPlacedBy`, `:86-90`). After two reloads they still read it (`Owner`, `:406-407`). A sneak-placed one over a new depot, given a motor and alloy from a hopper, turns a log into a casing. The fake player's UUID itself is not visible in game. |
| Broken holding a stack (#405) | Holding 16 sticks after its strokes, broken by the player: the sticks lie in front, at about the fake player's place (`discardPlayer`, `:466-474`, eyes at its feet, `DeployerFakePlayer.java:73-74`). The throw is the known API gap. In an earlier try, with cobblestone the deployer had placed where its fake player stood, the 15 cobblestone ended up at z 341.1, south of where the deployer had stood; that position was not traced. `/setblock air` drops nothing, but on this game build that is also true of a vanilla chest; `/setblock ... destroy` drops. |
| Hopper into the hand (#406) | A hopper above an unfiltered, unpowered deployer moves 10 sticks into its hand (the hand draws them); above one whose filter is the deployer item they stay in the hopper (`insertItem`, `isItemValid`). |
| Saved mid-stroke | At 1 rpm, quit with a log on the depot and the deployer `EXPANDING` at timer 776: after reload it reads `EXPANDING` at 728, runs on, and the log becomes a casing (alloy 4 to 3) (`write`/`read`: `State`, `Timer`, `:375-377`, `:403-404`). The DEBUG run showed a reloaded stroke sent to the client with `at` 0, `Reach` not being saved (#403). At 16 rpm the reloaded stroke had already ended by the time it could be read, also with the casing made. |
| Motor removed mid-stroke | **Differs.** In Create the hand stays where it was (`getHandOffset`, `:531-551`: at speed 0 the client's `tick` returns before `timer` moves, and the offset is clamped to `0..1` of the reach and to 21/16). In the port the hand keeps moving: removed at `EXPANDING` timer 296, the server keeps `EXPANDING` 232 while the hand passes into the floor within a second and is gone by 15 s. DEBUG lines showed the server's last `create:deployer` message was the stroke's start (`at` 0, `per_tick` 0.04), with nothing more after the stop (`deployer_turn`: `Deployer::turn` reports no change at 0 rpm, so `send_deployer_changes` has nothing to send), and the client's `draw_deployer` drawing `at + per_tick * elapsed` unclamped: 1.0, 2.6, 3.4 blocks. |

Not observable in game: the exact reach values, the fake player's UUID and name, and the turn order within a tick. Not run:
deployers facing up, east, south or west, a fist bump, filters set by other means than the slot click above,
contraptions, multiplayer, sound.

## #458 in game on Dev `ba1f24e` (branch `fix/operator-reload`, main `54e58817`)

One run on 2026-11-27 of the Linux build of Dev `dev-2026-11-26-ba1f24e` (headless, driven by the control socket), with launcher 1.3.7. The mod is `git archive` of `create/` at `ce7bd129` plus the one uncommitted tag file of #359 (`data/c/tags/block/barrels.json`), built by `sunrise-launcher native-build` as `native-ba1f24e-e1ba491cbedf`. Two rigs on a stone platform, a mixer over a basin and a press over a basin, each on a creative motor at 64 rpm, built in one session, then saved and reloaded. Items go in as summoned item entities of at most one stack. Details and screenshots are not kept in the repository.

| Area | Result |
|---|---|
| Mixer and press after a reload (#458) | **Passes**, where the same row of the next section failed. New inputs dropped in after a reload: the mixer's basin went from 2 to 4 `andesite_alloy`, the press's from 2 to 3 snow blocks; 32 andesite and 32 iron nuggets made 15 more alloy and 64 snowballs 13 more snow blocks in the 12 s before the frames. The whisk, down in the basin, differs between frames 0.3 s apart (2206 and 2422 of the 17850 pixels of its region; 1 at rest), and the press head is up in one frame and down at the basin half a second later. |
| Saved mid-cycle | Saved with the mixer `Running` 1, `Ticks` 20 and the press `Running` 1, `Ticks` 208, and once more at `Ticks` 232: within 3 s of the reload both read `Running` 1 each time, and the frames of the second show them moving (2747 and 3547 pixels). Each run ended on its inputs alone: 16 andesite and 16 iron nuggets gave 16 alloy, 32 snowballs 8 snow blocks; the next time 44 snowballs and 9 snow blocks gave 2 ice and 2 snow blocks. Snowball entities waiting in the basin at the save were there after it. |
| Full output | With 64 `andesite_alloy` in its basin the mixer stops (`Running` 0) and leaves its 20 andesite and 20 iron nuggets: the outputs must fit (`BasinRecipe.apply`, `acceptItemOutputsIntoBasin`). |

Not run: #460 in game, which was read from Java and is proven by the fuzzer's cases. Found on the way, not among these fixes: an item entity holding more of an item than its stack limit (made here with `/summon`) is an endless source for a basin. 64 snowballs in one entity (limit 16) kept `count: 64` while the basin under the press filled: 49 snow blocks and 7 ice three minutes later. An entity of one stack is consumed exactly. `basin_take` (`glue/processing/basin.rs`) hands the rest back as `BasinBlock.updateEntityAfterFallOn` does; why the entity's count stays is not traced.

## Basin, sequenced gearshift and drill/saw fixes in game on Dev `ba1f24e` (main `f5f2449d`)

One run on 2026-11-27 of the Linux build of Dev `dev-2026-11-26-ba1f24e` (headless, software Vulkan, driven by the control
socket), with launcher 1.3.7 (`launcher@1.3.7`, checked against its `SHA256SUMS`). The mod is `git archive` of `create/`
at `f5f2449d`, built by `sunrise-launcher native-build` as `native-ba1f24e-9d1e11a56f2d`, with Create 6.0.10's jar by its
pinned sha512. Details and screenshots are not kept in the repository.

As committed, the mod does not start on `ba1f24e` (#359): the jar's `create:single_block_inventories` names the block tag
`#c:barrels`, which nothing in the merged content defines. The rows after the first ran on the committed code plus one
uncommitted file, `data/c/tags/block/barrels.json` holding `minecraft:barrel` (the vanilla barrel, as the repository's
own item tag `c:barrels/wooden`), and most of them also on builds with temporary DEBUG lines (`create_log!` and
`eprintln!`, not committed) added while tracing the differences below.

| Area | Result |
|---|---|
| Startup as committed | **Fails** (#359): `Sunrise: unresolved block tags` before the title screen. Without the mod the same client reaches its first frame; with the one tag file above the game starts. |
| Mixer over a basin (#345) | At 32 and 64 rpm, andesite and an iron nugget make `andesite_alloy`, the head staying down (`Ticks` 20) while the recipe still fits; at 16 rpm the head stays up, as in Create (`MechanicalMixerBlock` needs medium speed). Emptying the basin by hand (`BasinBlock.useItemOn` → `onEmptied`) and breaking it mid-mix both read `Running` 0, `Ticks` 0 one tick later, head up (`BasinOperatingBlockEntity.tick` → `onBasinRemoved`, then `runningTicks >= 40` in `MechanicalMixerBlockEntity.tick`). |
| Spout output (#346) | A raised basin under a mixer spouts its alloy onto a depot below: the depot holds one, the next two wait in the basin's `Overflow` (a depot does not merge: `DepotBehaviour.canMergeItems`). Into a basin below: 3 alloys. The turn order within a tick is not visible in game. |
| Press over a basin (#347) | A new press over 5 iron nuggets stays idle; 4 more nuggets and it presses an iron ingot (from the basin's block data; the screenshot does not show the press). The 231-tick backup check was not observed. |
| Mixer saved mid-mix (#348) | At 32 rpm with `Running` 1, `Ticks` 20, after quit and reload it reads `Running` 1, `Ticks` 20 and finishes the cycle in 82 turns (1 + 1 + 61 processing + 18 up + 1, as `MechanicalMixerBlockEntity.tick` from an unsaved `processingTicks` 0). `currentRecipe` is not saved, so that cycle makes nothing (`applyBasinRecipe` returns on a null recipe, `BasinOperatingBlockEntity.java:110-112`), and the basin's items are left; see the next row. |
| Mixer and press after a reload | **Fails.** A loaded mixer or press over a basin never looks at its basin again: items added later stay, a speed change does nothing; a freshly placed one works. A DEBUG log shows `persist::load_entity` inserting the saved mixer or press (`glue/processing/persist.rs`, `Block::MechanicalMixer`, `Block::MechanicalPress`) before the engine knows the block (`block_at` None), so `CreateWorld::place` (`engine/src/world.rs`, `!d.mixers.contains_key`, `!d.presses.contains_key`) creates no operator and `schedule_basin_check` finds none. In Create every operator has its `basinChecker` (`BasinOperatingBlockEntity.addBehaviours`, `:40-44`), which a change of the basin's contents schedules (`BasinBlockEntity.java:357-361`), as does `onSpeedChanged` (`:47-53`). |
| Sequenced gearshift's screen | Opens on use; the mouse wheel set Turn 180° forward, Turn 90° reversed, End, saved as `Instructions` the way `Instruction.serialize` writes them. A lever goes on it only sneaking: `SequencedGearshiftBlock.useItemOn` opens the screen for any item but a kinetic block. |
| Powered and unpowered | 16 rpm motor: instruction 0 turns the far cog at 16, instruction 1 at −16, then it is idle with state 2 while powered (`run` keeps the state while `hasNeighborSignal`) and state 0 once unpowered. Unpowered mid-sequence it runs to its End, then state 0. Powered with no rotation it stays idle (`onRedstoneUpdate` returns at speed 0). |
| Hand crank (#341) | Crank held, then the signal: the sequence runs, instruction 0 at the crank's −32 and instruction 1 at 32 (twice out of twice). Signal first, then the crank: it stays idle (three of three). #341's case, the crank's first turn and the signal between the same two ticks, was not reproduced: no try was tick-stamped. |
| Clutch in the line | **Fails** (not among these fixes; the earlier "In game" run below passed it). A clutch acts one change late: powered, the cog keeps turning (two screenshots a second apart differ); unpowered, it stops; also with no sequenced gearshift in the line. DEBUG lines show `reattach_scheduled_transmissions` re-attaching the clutch on the engine's old state (`powered` false after a rising signal, true after a falling one), with the mod's `block_changed` for the clutch's own `powered` write arriving after it. In Create `ClutchBlock.neighborChanged` writes `POWERED` before it detaches (`ClutchBlock.java:17-28`), and `ClutchBlockEntity.getRotationSpeedModifier` reads the block state. Same code path as #371 (detach and re-attach back to back in `tick_inputs`). The sequence kept its timing: no `onSpeedChanged` reached the gearshift when the clutch switched. |
| Lever on a sequenced gearshift | **Differs, expected from the Java, not confirmed in Create's game**: a lever on top of it powers a clutch beside it in the port. Create overrides `SequencedGearshiftBlock.shouldCheckWeakPower` to return false (`:56-59`), which is meant to keep the gearshift from passing that power on. |
| Sequenced gearshift saved mid-instruction | At 4 rpm (`Timer` 65 of 152): after quit and reload `InstructionIndex` 0, `Timer` and `InstructionProgress` are kept; it finishes instruction 0, runs 1 and ends (`SequencedGearshiftBlockEntity.write/read`). |
| Drill on a motor (#384) | At 16 rpm it cracks and breaks stone, an oak log and a column of three sand (each falling block in turn); bedrock and water stay (`NextTick` −1, `isBreakable`). Facing down works. Stone placed by the player in front of an idle drill reads `Progress` 1 two ticks later, without waiting for a lazy tick (`DrillBlock.neighborChanged` → `destroyNextTick`). |
| Drill broken mid-crack (#385) | The cracks go with it (`BlockBreakingKineticBlockEntity.invalidate`). |
| Drill saved mid-break (#386) | At 4 rpm with `Progress` 5: after quit and reload `Progress` 5 and the same crack stage, then it breaks the stone. |
| Motor stopped and restarted | Stopped mid-break, `Progress` and the cracks stay; restarted, it goes on and breaks the block. |
| Saw | A horizontal saw (shaft from behind, `SawBlock.hasShaftTowards`) cuts an oak's lowest log and the whole tree comes down: 5 logs and 35 leaves gone; logs, saplings, sticks and an apple drop. Against stone it does nothing (`SawBlockEntity.canBreak`). |

Not observable in game: #341's same-tick case, #346's order within a tick, #347's backup check, #387's lazy tick. Not run: a vertical sequenced gearshift (the shape of #341's case), belts or funnels at a basin's spout, a heated mixer, fluids in a basin, the saw's recipes, contraptions, multiplayer, sound.

## Dev build `6a02076` in game

One run of the Linux build of draft Dev `6a02076` (headless, software Vulkan, driven by the control socket), from the mod
folder `create-data prepare` writes, with Create 6.0.10's jar by its pinned sha512. Details and screenshots are not kept in
the repository.

| Area | Result |
|---|---|
| Startup | No error or panic; no `render pipeline ... refused` line, so the seven named pipelines and the 23 `create:steam_jet/N` pipelines built on the GLSL shader. The tag and entity-renderer warnings are those of the `bdad86d` run. |
| Worldshaper outline (`create:checkered`) | Checkered faces and edges drawn. |
| Super glue area (`create:glue`) | The selection preview draws with the glue texture; gluing works. |
| Value box (`create:icons`) | The creative motor's hover frame and its speed icon draw. |
| Display link and nixie tubes | The link targets the nixie tube and the tubes light; no side-by-side comparison. Signal glow not looked at. |
| Operators on top of a depot or basin | Sneak-clicking the top face puts the press, spout and deployer (on a depot) and the mixer (on a basin) two above. With the wand of symmetry active the press is mirrored once. Not observed: belts as the base, the advancement and the kinetic indicator. |
| Block drops (survival) | A door drops one door, a double slab 2, a horizontal gearbox `vertical_gearbox`; shaft and zinc ore (`raw_zinc`) drop correctly. |
| Kinetics | A large cog drives a small cog at 32 rpm and the mixer meshing it makes `andesite_alloy` from an iron nugget and andesite; two stacked mechanical crafters make 4 sticks from two planks. |
| Redstone link | Transmitter and receiver switch a lamp on and off. |
| Deployer | **Fails.** It runs at 16 rpm with a block in hand but places nothing: `perform_action_scoped` returns `Busy` on every stroke (checked with a temporary `eprintln!` in `act_with`, not committed). |

Not run: funnels, cart assembler, trains, schematic save and reload, the glow of signals, multiplayer, sound.

## #275 (press and brass tunnel) in game on Dev `3d34bbe` (main `3e0a710`)

Main now pins Dev `bdad86d` (#276 moved it to that API), so the build is #275's merge `3e0a710`, the last main commit on
`3d34bbe`. From there to `fc2f541`, `engine` is unchanged; in the glue `press.rs`, `depot.rs` and `tunnel.rs` are
unchanged, and `belt.rs` changed only its `collision_boxes` call. Built with `sunrise-launcher` 1.2.0 `native-build` for
`dev-2026-11-24-3d34bbe`, in the creative world `press275` on smooth stone at y 149, driven by `sunrise-ctl`, read back
with `/data get` (the depot's `Locked`, belt `Inventory`, `BrassTunnel`) and stepped with `/tick freeze` and
`/tick step 1`, then saved, quit and relaunched. Motor speeds and tunnel modes were set by pasting clipboards. Compared
with Create 6.0.11 (`b62e837`) Java. Screenshots are in `/mnt/lanes/game-main/game/`.

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | #270 press over a depot | Pass | 11 iron ingots on the depot under a 16 rpm press were pressed one per cycle (6 sheets before they were taken off). With the press running on an ingot, the ingot was swapped for 2 diamonds: locked on arrival (`onItemReceived`: `HOLD` while running), unlocked for exactly one tick when the head was at its lowest point (stepped at 16 rpm, and at 1 rpm: steps 120 and 242 of a 240-tick cycle), locked again the next tick, and unlocked for good when the cycle ended. This is `whenItemHeld` returning `PASS` at `runningTicks == CYCLE / 2` when `tryProcessOnBelt` finds no recipe, and `DepotBehaviour.tick` setting `locked = result == HOLD`. Diamonds put on the idle press did not start it and stayed unlocked. After the relaunch in item 3, an ingot swapped in for the let-go diamonds locked and came out as a sheet. See `275-01-press-depot-sheets.png` to `275-04-diamonds-alone-press-idle.png`. |
| 2 | Press over a belt | Pass | 3 iron ingots on a 6-long belt: one was locked under the press and came out as a sheet while the other 2 waited, and a single ingot later did the same. Diamonds alone rode under the idle press unlocked. Diamonds following an ingot queued behind it while it was pressed and then passed unlocked. See `275-07-belt-press-iron.png`. |
| 3 | Press save and reload mid-cycle | **Differs** (not from #275) | Saved at 1 rpm with the diamonds locked and the press 48 cycle ticks in. After the relaunch the press was idle (head up) and the depot unlocked the diamonds in the first tick. A temporary DEBUG log in `depot_turn` showed `running true ticks 48` before the save and `running false ticks 0` after. Java saves the cycle (`PressingBehaviour.write`: `Running`, `Finished`, `Ticks`; `read` restores them), so it would keep the stack locked and finish the cycle. The port keeps presses in `World::presses` and makes a `Press::default()` when the block loads (`engine/src/world.rs`, `Block::MechanicalPress`); the press's `/data get` shows no cycle fields, and `persist.rs` and `kinetics.rs` name none. An ingot put on after the relaunch was pressed normally. See `275-05-before-save-press-mid-cycle.png`, `275-06-after-reload-press-stopped.png` and `275-13-after-reload-ingot-pressed.png`. |
| 4 | #271 side belt into a synchronized brass tunnel | Pass | Two brass tunnels on side-by-side belts (one placed by hand, one by `/setblock` on a cased segment), set to synchronize. A 4-long side belt ends against the east tunnel's segment. The east tunnel held 4 gold waiting for its group, and 3 copper on the side belt waited at its end. With 5 emeralds behind the gold on the east belt, 2 iron went into the west tunnel. Stepped tick by tick: the group synced (step 48), the east tunnel handed out its gold (50), and the copper went in the next tick with both tunnels still `SyncedOutput: 1b` (51). The emeralds on the tunnel's own belt waited until the group cleared (87) and went in at 88. This matches `tryInsertingFromSide`, which asks only `getStackToDistribute().isEmpty()` and the flap filter; `canTakeItems` is for items on the tunnel's own belt. To make the state last (both tunnels placed by hand by then, after item 6), the west belt's motor was removed while that tunnel held iron. A stopped belt is no output and its tunnel does not tick, so the group stays active (`tick` :147-158: neither all empty nor all full). Copper from the side kept going in and through while 5 emeralds on the east belt waited. See `275-09-tunnel-holding-side-waits.png` and `275-10-synced-empty-side-in-own-waits.png`. |
| 5 | Unsynced tunnel, tunnel holding a stack | Pass | Pasted split onto the group: side copper went into the empty east tunnel and was split across both belts, and so were emeralds from its own belt. With a stack in the tunnel, side items waited at the side belt's end (item 4). |
| 6 | Break and place tunnels | Pass, one difference | Breaking the east tunnel by hand left its cased segment and disconnected the west one. Placing it again by hand gave `t_right`, connected both, and it took the group's synchronize mode. **Differs** (not from #275): `/setblock` of a brass tunnel on an uncased belt dropped it as an item at once. Java's `canSurvive` (casing needed) runs only from `neighborChanged` when the block below changes, and a `/setblock` reaches only `onPlace`, so by that code Java keeps it (read, not run). In the port every new tunnel state goes to `tunnel_fresh`, and `shape_tunnel` breaks it without casing (`glue/logistics/tunnel.rs`). Placing by hand cases the belt first, so players do not meet this. |
| 7 | Tunnel save and reload | Pass | Saved with the west belt stopped and its tunnel holding 2 iron, the east tunnel empty, both `SyncedOutput: 1b`, and 5 emeralds waiting on the east belt. After the relaunch all of that was kept, connections and mode included, and 6 copper from the side went in and through. With the west motor placed again, the iron was handed out, the group cleared and the emeralds went in. See `275-11-after-reload-synced.png` and `275-12-west-restarted-emeralds-in.png`. |

Neither difference was fixed here. The press one needs saving and loading the cycle on main's `bdad86d` API, which this
instance cannot run.

## Dev build `bdad86d` in game (historical)

Two runs on the Linux build of Dev `bdad86d` (headless, software Vulkan, driven by the control socket). Only the second had
Create's local `create-data prepare` output; the first had none, so its missing wand icon and sprite warnings and its slow
schematic preview after relaunch likely came from that. The first run met a startup crash: 26.3 has no `minecraft:block_state_property` loot
condition (it has `match_block`), and 79 generated block loot tables used it. #277 changed `tools/create-data` to emit
`match_block` and regenerated them. The second run is on main with #277 and no loot edits.

| Area | Result |
|---|---|
| Startup | No error or panic in three launches. |
| Block drops (survival) | Doors drop one door from either half, a double slab drops 2, a horizontal gearbox drops `vertical_gearbox` and a vertical one `gearbox`; shaft, cogwheel, seat, toolbox and zinc ore drop correctly. |
| Motor and shafts, belt to depot, pump with tank and valve, bearing and windmill, press, millstone | Pass in the first run, not repeated in the second. |
| Mechanical crafter, mixer with basin (32 rpm), redstone link, display link (nixie tube shows the cuckoo clock's time) | Pass. |
| Funnels | A belt funnel on the belt and an upward-facing brass funnel at the belt's end fill a chest. A horizontal funnel at a belt's end takes nothing, as `supportsDirectBeltInput` says. |
| Cart assembler | The planks ride in the cart and it rolls along powered rails. |
| Schematic save, deploy, relaunch; wand of symmetry | Pass (second run, after prepare). |
| Filter, toolbox and postbox menus; potato cannon; save and relaunch | Pass in the first run, not repeated in the second. |
| Deployer | **Fails.** Its strokes cycle, but every `perform_action_scoped` call is refused with `Busy`, so it places and uses nothing. Earlier Devs (`3d34bbe`) did this work. Logging at the refusal shows an open host transaction when it acts, and the game wraps `level_tick_post` listeners in one; whether removing the wrap fixes it is not tested: sunrise-org/sunrise#5750. |
| Trains | Not exercised: the track pieces could not be connected through the driver. |

Warnings that remain at startup: the jar's tags `c:tea`, `c:teleporting_not_supported` and `create:ignore_seat` name
members the port does not declare (see API_GAPS.md), and five entity types have no renderer.

Not tried: schematicannon, mechanical arm, steam, multiplayer, sound, a toolbox's contents when dropped.

## #261-#264 and #266 in game on Dev `3d34bbe` (main `46eb2c2`)

Built with `sunrise-launcher` 1.2.0 `native-build` for `dev-2026-11-24-3d34bbe`, resources prepared for that build with
its `sunrise-prepare`. In the creative world `ari266`, on a smooth stone platform at y 149, driven by `sunrise-ctl`,
read back with `/data get`, `sunrise-ctl container` and `sunrise-ctl entities`, then saved, quit and relaunched.
Compared with Create 6.0.11 (`b62e837`) Java. Screenshots are in `/mnt/lanes/game-main/game/`.

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | #261 deployer calm hive harvest | Pass | An east-facing deployer from `/setblock`, given shears by hand, on a full beehive holding 2 bees: `honey_level` went to 0, 3 honeycomb popped out, both bees stayed in the hive and the shears took 1 damage (0 → 1 on a single harvest, repeated). A north-facing deployer placed by hand while sneaking, given 3 glass bottles, on a full bee nest holding 3 bees: the bees stayed, the bottles went 3 → 2 with a honey bottle in its overflow; it then waited until a hopper below drained the overflow (`DeployerBlockEntity.tick`: `if (!overflowItems.isEmpty()) { timer = ...; return; }`), and the last bottle left the honey bottle in its hand (the PR's `shrink` then `set_held` path). A player shearing a full hive with bees, as the control, still released them all. This matches `BeehiveBlockMixin` (`original \|\| player instanceof DeployerFakePlayer`). The held shears, their damage and the held honey bottle were kept over the reload. See `261-01-shears.png` to `261-03-player-control.png`. |
| 2 | #262 knockback through `LivingRef::knockback` | Pass | Cardboard sword on a pig, four hits from three directions: pushed straight away from the player by 5.17 blocks each time. A simulation of `LivingEntity.knockback(1.0, ...)` with vanilla friction (`2 * 0.5` from `CardboardSwordItem.knockback`) gives 5.173. A fifth hit, after the relaunch, pushed the pig 5.3 blocks and gave it slowness amplifier 9 for 60 ticks (55 left when read), as in `CardboardSwordItem`. An earlier try is not counted, because the pig's position at the hit was not read. Potato cannon at 5 blocks: the pig lost 5 health and was pushed 5.6 blocks along the shot, close to the hurt knockback 0.4 plus `knockback * 0.6` = 0.9 from `PotatoProjectileEntity.onHitEntity` (`target.hurt`, then `livingentity.knockback(knockback * 0.6, ...)`; the potato type's `knockback` is 1.5) (5.69 in the same simulation; the pig also panics after the hit). Without Create's knockback, hurt's 0.4 alone would push it 2.07 blocks. See `262-02-potato-pig.png`. |
| 3 | #263 cog helper before the crafter | Pass | A north-facing idle crafter from `/setblock`: a cogwheel used off-centre on its front face went beside it, at the east with `axis=z`, and the crafter stayed empty; used at the centre (the shaft, `hitOnShaft`) it went into the crafter. A crafter placed by hand (`facing=west`): a large cogwheel used off-centre went to the diagonal `(16,151,21)` with `axis=x`, as `DiagonalCogHelper` picks (`closest`, then the nearest other side). A cogwheel on a millstone's top went beside it at the nearest side with `axis=y`. This is `CogwheelBlockItem.onItemUseFirst` running before `MechanicalCrafterBlock.useItemOn`. The crafter kept its cog over the reload. Breaking it then dropped that cog, and breaking the helper-placed cog left the crafter in place. See `263-01-crafter-offcentre-centre.png` and `263-01-crafter-cogs.png`. |
| 4 | #264 schematic block preview | Pass | Selected a 3x3x3 build with the schematic and quill, saved it as `ghosttest.nbt` with the prompt's convert button, and deployed the creative-given schematic: its blocks were drawn at the anchor, matching the original block for block. A copy turned `CLOCKWISE_90` drew log, bricks, glass and wool turned a quarter clockwise. A `FRONT_BACK` copy drew them with x flipped. A deployed schematic held in hand through a save and relaunch drew again at once. Switching straight between two hotbar schematics of the same file keeps the first one's placement, as Java's `SchematicHandler.tick` only calls `init` when `!active` or the file differs; going through an empty slot shows the other. The turned and mirrored copies were made with `/give`, since `sunrise-ctl` has no Alt key for the tool menu. The schematic table and schematicannon flows and block-entity previews were not tried. See `264-01-quill-selection.png` to `264-08-after-reload.png`. |
| 5 | #266 funnel inventory from its first tick | Pass | A 6-long belt from the connector into an upward brass funnel on a depot: 16 iron ingots went through into the depot while playing. Then 20 gold ingots waited at the belt's end (position 5.75) behind the iron and were saved there. After the relaunch they were still at 5.75 and went in once a hopper drained the depot. Replacing the depot with a chest (the block behind the funnel changed) let 24 copper ingots in. A `/setblock` andesite funnel on a chest took 5 thrown diamonds. A brass funnel placed by hand on a chest was extracting (`FunnelBlock.getStateForPlacement`: `EXTRACTING, !sneak`); placed while sneaking, it took 7 thrown emeralds. Belt funnels placed by hand on chests beside the belt: an extracting one pushed 10 redstone onto the belt and a sneak-placed inserting one downstream took all 10. Saved with 30 lapis mid-belt and coal and quartz still in the source chest; after the relaunch all 90 reached the inserting funnel's chest. Breaking the inserting belt funnel by hand let 9 flint ride on to the belt-end funnel. The one-tick wait itself is too short to see in game. The fuzz regression test was not run here. See `266-01-gold-waiting-before-save.png` and `266-02-after-reload-chest.png`. |

Found along the way, and not from these PRs: a broken deployer drops its held item by id and count only
(`Deployer.held` into `deployer_drops` in `engine/src/world.rs`, unchanged since `b3b56ed`), so shears with 1 damage dropped as new shears. Java
drops the fake player's own stacks: `DeployerBlockEntity.discardPlayer` calls `player.getInventory().dropAll()`.

## PR #243 processing on the Dev APIs in game (main `43aa858`)

Built with `sunrise-launcher` 1.2.0 for `dev-2026-11-22-7cfdf81`. In the creative world `proc243`, placed the
millstone by hand, set the other fixtures with `/setblock`, and changed the motors' speeds with clipboards.
Checked held items and timers with `/data get` and loose items with `sunrise-ctl entities`. Compared with
Create 6.0.11 (`b62e837`) Java. Screenshots are in `/mnt/lanes/agent-procgame/game/`.

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | Millstone admission | Pass | At 16 rpm, all 4 dirt stayed in the hopper. Swapping to 4 cobblestone filled the input and produced 4 gravel in the output. Dirt thrown by hand stayed a loose item. An inserting andesite funnel admitted all 3 cobblestone while 3 dirt stayed loose on the funnel, as in `MillstoneInventoryHandler.isItemValid` and `MillstoneBlock.updateEntityAfterFallOn`. See `01-millstone-dirt.png`, `03-millstone-gravel.png` and `14-millstone-funnel.png`. |
| 2 | Crushing wheel exits | Pass | With opposite 128 rpm motors, cobblestone dropped from above produced gravel below the downward controller at (8,105,0). Cobblestone thrown horizontally into the controller at (18,106,0) yielded gravel north at z≈−1.99 and south at z≈2.74 after reversing the motors. The results remained loose outside the controller. With the controller facing up and fed by a hopper removed after intake, gravel rose and settled directly on top at (8.5,106,0.5). This follows `CrushingWheelControllerBlockEntity.tick` and the tagged-item collision rule in `CrushingWheelControllerBlock.getCollisionShape`. See `04-crusher-up.png` through `07-crusher-south.png`. |
| 3 | Controller and saw removal spills | Pass | Breaking a wheel at 16 rpm removed the controller and spilled all 12 cobblestone it held with the recipe still unapplied. Breaking the powered upward saw at −16 rpm with 31 of 50 ticks left spilled its uncut oak log. An intact run yielded a stripped oak log. Both spills match the Java removal paths through `ItemHelper.dropContents`. See `10-crusher-spill.png` and `15-saw-powered-spill.png`. |
| 4 | Save, quit, relaunch mid-process | Pass | Saved with 2 cobblestone and 4 gravel in the millstone (timer 218 before save) and 12 cobblestone in the crusher (remaining time about 240.36). After reopening, both retained those stacks and their timers counted down (millstone 125 → 95, crusher 206.80 → 197.52). The millstone finished with 6 gravel and the crusher ejected 12 gravel. This agrees with both Java block entities' inventory and timer serialization. See `11-before-save.png` through `13-reloaded-output.png`. |

The three `committed_assets` tests passed, including the comparison with the cached Create jar.

## Dev build `7cfdf81` in game (branch `pin/dev-7cfdf81`)

Built with `sunrise-launcher` 1.2.0 `native-build` against `dev-2026-11-22-7cfdf81`, the profile switched from the
nightly, a creative world, driven by `sunrise-ctl`, read back with `/data get`. Rows 1-6 ran on `9880110`, the rest on
`b680a0f`.

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | Creative motor turning shafts | Pass | 16 rpm on the shafts, capacity 262144 |
| 2 | Belt carrying a diamond onto a depot | Pass | 5-long belt made with the connector; the depot at its west end holds the diamond after about 5 s |
| 3 | Pump filling a tank | Pass | 8 mB a tick, 1592 → 2400 → 3216 → 4024 mB, then 8000 |
| 4 | Fluid valve opened | Pass | Pointer `Value: 1.0f`, `enabled=true` |
| 5 | Vault and tank connection groups (#213, sunrise#3875) | Pass | Two touching 2x2 vaults each join into one texture with a seam between them; a 3x3 vault (`large=true`) joins as one; two touching 2x2 tanks show separate joined tops |
| 6 | Save, quit, relaunch | Pass | Speeds, the depot's diamond, the valve, the full tank and every vault and tank join kept |
| 7 | Overstress flash (sunrise#3383) | Pass | A held hand crank drives 512 SU of millstones on 256 SU: the goggles say overstressed, smoke puffs out and the crank's cog draws tinted red, fading |
| 8 | Haunted bell souls (sunrise#3400) | Pass | A rung bell raises souls on the dark blocks of each ring, upright and turned to the camera, edge-on from above, their bases flat on the ground; a glowstone placed among them clears them. Create's code-defined particles (souls, rotation indicators, air flow, cannon air) were sent with an untyped `ParticleOptions`, which this build drops; `b680a0f` sends their typed options |
| 9 | Fluid particles (sunrise#3399) | Pass | Water pouring from an open pipe end falls as blue-tinted crumbs |
| 10 | Rotation indicators, fan air, cannon air | Pass | With goggles a new shaft's sparks show green; a turning fan blows white air-flow wisps; a fired potato cannon puffs air ahead of the barrel |
| 11 | Wand screen model (sunrise#3404) | Pass | Sneak-using the symmetry wand opens its screen with the wand drawn tilted in 3D beside it |
| 12 | Bearing structure light (sunrise#3501) | Pass | Turning planks at noon read 80-94% of still planks' brightness as their faces turn (directional shading), not the half they drew before |
| 13 | Filter menus (sunrise#3767) | Pass | A list filter opens from the hand; replacing it in that hand, or selecting another slot, closes the menu |
| 14 | Item-group attribute (sunrise#3577, #3572) | Pass | An attribute filter on a cogwheel offers "is in group 'Create'" |
| 15 | Toolbox inserts (sunrise#3610) | Pass | A hopper fills a toolbox: 40 cobblestone in compartment 0, then 3 dirt in compartment 1 (slot 4) |
| 16 | Belt blocking (sunrise#3751, client) | Pass | A player riding a belt stops 0.4 blocks short of an armor stand and stays there; an area effect cloud (pistons ignore it) lets the player ride on to the belt's end |
| 17 | Ejector nudge on the server (sunrise#3751), cart passengers (sunrise#3865) | Not tried | Not set up in game on this build |

## Saved minecart riders on `9425651`

The stock package passed `tools/check-cart-rider.py`: the same rider UUID stayed aboard across chunk unload and reopen, a coupled cart moved back toward its original separation, and activator disassembly, lethal damage and `/kill` restored blocks. Main- and off-hand placement each spent one item. An obstructed vault did not overwrite the three diamonds in the stationary vault.

Run the test on Linux with Xvfb, signed Nightly client and `sunrise-ctl`, a private prepared resource tree and the verified jar directory. From the repository root, with `GAME`, `CTL`, `RESOURCES` and `JARS` set to absolute paths:

```sh
(cd create && xvfb-run -a cargo sunrise-mod run --game "$GAME" --resources "$RESOURCES" --jars "$JARS" -- --frames 1)
xvfb-run -a python3 tools/check-cart-rider.py --ctl "$CTL" \
  --game create/target/sunrise-mod/run/app/sunrise-client --resources "$RESOURCES" \
  --jars "$JARS" --scratch target/cart-smoke
```

In a **disposable checkout**, apply `git apply tools/fixtures/cart-rider.patch`, repeat the staging command, and append `--refusal-fixture` to the test. The fixture does not ship. It refuses a mount and item joins, swaps the exact rider for a pig in one callback, checks tank contents, and runs the actual server wrench handler once with both kinds of link. It also seeds a legacy level save: the ordinary link and nondefault assembler mode survive reopen, but the obsolete link pays no refund and the old payload does not create a rider. These tests do not assert exact occupied-cart rail physics or cross-chunk passenger saves with a loaded non-ticking root.

## Registry package smoke test on `ed3ac19`

From a clean `git archive` of the registry data commit, with the signed release SDK and no `create-data prepare`
outputs: `cargo sunrise-mod build`, `cargo sunrise-mod check` and `cargo sunrise-mod check` on the `.smod` passed.
The package carries `LICENSE` and `NOTICE.md`. The `[[jar]]` entry references the player's verified Create
1.21.1-6.0.10 jar; no jar is in the package. `cargo sunrise-mod run --resources <vanilla resources> -- <game args>`
under Xvfb and llvmpipe, without the launcher, staged the package's data and assets unchanged and opened a new
creative world. `/give @s create:cogwheel`, and the same command for `create:zinc_ingot` and `create:wrench`,
all succeeded. The cogwheel, motor and depot drew in the world, and the cogwheel and zinc ingot drew in the hotbar.
At that point the wrench occupied a slot but had no icon or held model. The visual test below verifies the
replacement item model and other assets migrated off local preparation.

## Repeat the toolbox smoke test

On Linux with Python 3.9+ and a display (or Xvfb), set `nightly`, `resources` and `jars` to absolute paths
for the pinned Nightly, a private copy of its prepared resources, and the directory containing the verified
Create jar named by SHA-512. From the repository root, stage the native app with a one-frame run, then start
the test using that app:

```sh
(cd create && xvfb-run -a cargo sunrise-mod run --game "$nightly/sunrise-client" \
    --resources "$resources" --jars "$jars" -- --frames 1)
xvfb-run -a python3 tools/check-toolbox.py "$nightly/sunrise-ctl" \
    create/target/sunrise-mod/run/app/sunrise-client "$resources" "$jars"
```

The script launches its own game with an isolated `SUNRISE_HOME`, `--game-dir` and control socket, then shuts
it down and removes those temporary files. It checks indexed compartment kinds, contents and UUID before
and after breaking and re-placing the sole drop. It does not drive `STORAGE`.

## Registry visuals on `ed3ac19`

Built and checked a second `.smod` from a clean checkout with no `create-data prepare` outputs. The 5,916 staged
manifest and resource entries matched the `.smod` byte for byte. Ran it without a launcher with the player's
pinned jar under Xvfb and llvmpipe:

| Scene | Observed |
|---|---|
| Nixie tube, display link, stock link | The original composite parts drew, including the nixie glass and numerals. |
| Two minecarts linked using a coupling item | Attachments, rings and textured link drew. |
| Steam whistle on a tank over a lit campfire, powered by a redstone block | The whistle blew; successive steam frames drew using the packaged shader and still drew after a resource reload. |
| Two connected chain conveyors driven by a creative motor | The motor drove the conveyors at 16 rpm and the chain texture scrolled between frames. |
| Wrench, potato cannon, handheld worldshaper | Hotbar icons and held models drew. In fixed item frames the gear, cog and accelerator changed angle while the bodies stayed still. |

Local `prepare` still supplies connected textures, tinted models, model-derived placement ghosts and other
unmigrated assets. The package does not replace them; the checkerboard, glue and icon pipelines still log
missing sprites without local `prepare`.

## Declared shapes and piston heads on `ed3ac19`

In the registry smoke-test world, placed a torch and a standing sign on a metal girder, and a wall sign on the
full declared face of a mechanical piston head. The attachments stayed in place. A creative motor extended a
mechanical piston through two poles. The new head was present and `/data get block` reported no block entity;
the same command on a vanilla player head returned `minecraft:skull`. Reversing the motor retracted the piston
and removed its head.

## In game

A local developer base built from Sunrise at the pin, the mod built by the launcher's `native-build` from
`create-data prepare` of Create 1.21.1-6.0.10's jar, fresh creative worlds, driven by `horizon-ctl` (player clicks,
commands, screenshots read back, `/data get` and the mod's `create_debug` readouts).

### Features

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | Stock ticker category editor | Pass | Cards added (list and attribute filters, typed names), renamed, deleted (filter refunded), moved down; same after reload |
| 2 | Stock keeper screen | Pass | Categories fold; search by name, `@create`, `#logs`; wheel scroll clipped to the list; lock toggles; addresses offered from a held and a placed clipboard; seated villager drawn facing the cursor; Send made a 2 iron + 1 diamond package to `Storage` |
| 3 | Signs as addresses | Pass | Sneak-placed sign "Bakery" stays on a packager, its package is addressed `Bakery`; wall signs stay on a frogport and a postbox |
| 4 | Potions through drain, pipe, tank, spout, bottle, open end | Pass | Custom colour and effects (jump boost II) kept to the filled bottle and given by the open end; tank and glass pipe drawn magenta |
| 5 | Harvester, roller, portable storage and fluid interfaces | Pass | Moving parts drawn placed, on a bearing and on a train |
| 6 | Actor heads on a bearing and a train | Pass | Drill, saw, harvester, roller turn between frames; deployer drawn |
| 7 | Contents on moving structures | Pass | Depot item, tank and basin water drawn on a turning bearing; depot item on a train |
| 8 | State across a piston move and a bearing stop | Pass | Depot items, tank and basin fluid, motor speed equal in `/data get` before and after; link frequencies and funnel filter still drawn |
| 9 | Elevator contacts and floors | Pass | Doors open for All, one side and None as set; contraption controls call a floor |
| 10 | Sloped belts | Pass | Sloped outline; items ride up and down |
| 11 | Station summary and factory gauge status sources | Pass | Board shows `now / Unnamed Train / Zeta`; gauge board shows `9 ▪ Diamond` (boards need 30 rpm, as Create's) |
| 12 | Clipboard copy and paste | Pass, after fixes | Funnel filter and count, chassis range, brass tunnel side filter (per side), steam engine direction, table cloth price copied and pasted, read back by copying again. Fixed: filter items on east and west value boxes were drawn inside the block (`0f4bd3b`); a chassis' range board did not open with the wrench, which turned it instead (`de419ed`) |
| 13 | Glass pipe flow and drips | Pass | Flow grows from the pump, drips at the open end, clears when the pump stops (as Create) |
| 14 | Open pipe ends | Pass | Milk clears poison, tea gives haste, potions their effect; water puts out fire and burning mobs and places a source, lava sets fire and places a source |

### Families

| Family | Variants | Result | Evidence |
|---|---|---|---|
| Shafts | andesite, brass, girder encased, 3 axes | Pass | Motor drives all, 16 rpm |
| Cogwheels | small, large, encased, shafted | Pass | Speeds pass; cog on shaft extends it |
| Gearboxes | gearbox, vertical gearbox | Pass | Reverses straight through |
| Clutch, gearshift, sequenced gearshift | powered and not | Pass | Clutch stops, gearshift reverses, sequence turns 90° |
| Speed controller | | Pass | Large cog above takes its speed |
| Motors, crank, valve handles | creative motor, crank, copper + 16 dyed valves | Pass | Crank 32 rpm 256 su; valve turns 90°; dyeing |
| Water wheels | small, large, 12 woods | Pass | 8 rpm 256 su / 4 rpm 512 su |
| Windmills | bearing, frame, 16 sails | Pass | 16 sails 2 rpm 1024 su, drawn turning |
| Steam engine and boiler | 5 engine faces, boiler sizes | Pass | Level 1 boiler, 64 rpm 16384 su |
| Gauges | speed, stress, 24 states | Pass | Dials on Create's faces, needle reads |
| Belts | lengths, slopes, casings | Pass | Items carried to a funnel |
| Funnels | andesite, brass, 24 states, belt funnels | Pass | Insert and extract; filter drawn |
| Tunnels | andesite, brass | Pass | Items pass; side filters |
| Chutes | 20 states, smart chute | Pass, after fix | A chute fed by a hopper never let the item fall (`8f9a74c`) |
| Depots, ejectors | | Pass | Ejector throws onto a depot |
| Mechanical arms | floor, ceiling | Pass | 16 apples moved depot to depot |
| Item vaults | 2×2, 3×3 | Pass | Hopper fills |
| Observers, switches | content observer, stockpile switch | Pass | Powered by chest contents |
| Press | | Pass | Iron sheets |
| Mixer, basin | | Pass | Andesite alloy at 32 rpm, idle at 16; basin output and filter |
| Blaze burner | states, coal, cake | Pass | Kindled, fading, smouldering, seething |
| Saw | 24 states | Pass | Stripped log |
| Drill | | Pass | Breaks stone |
| Millstone | | Pass | Flour |
| Crushing wheels | | Pass | Gravel |
| Fans | washing, smoking, blasting, haunting | Pass | All four recipes |
| Deployer | 12 states | Pass | Filter, crafting cogwheels |
| Crafter | 16 states | Pass | Planks |
| Pumps, pipes, valves | shapes, encased, glass, smart pipe, valve, 6 pump facings | Pass | Flows and filters |
| Tanks | 2×2×2, 3×3×3, 1×4, creative | Pass | Fill drawn, kept on reload |
| Spout, drain | | Pass | Buckets and bottles filled; drain takes a bucket |
| Hose pulley | | Pass | Drains a pool into a tank |
| Bearings | mechanical, windmill, clockwork | Pass | Assemble, turn, clock hands |
| Pistons | normal, sticky, poles | Pass | Push, retract, pull |
| Pulleys, elevators | rope, elevator | Pass | Load lowered; floors |
| Gantry | carriage, shafts | Pass | Runs both ways |
| Carts | assembler, cart contraption, couplings | Pass | Drill cart assembled, coupling drawn |
| Trains, stations, signals | track, curves, stations, bogeys, signals, observers | Pass, after fix | Carriages lost their glue and controls' settings when taken apart (`09011b2`); trains run between stations, signals and observers react |
| Moving actors, glue, sticker, seats, controls | harvester, plough, roller, drill, saw, interfaces | Pass, after fix | Glue clicked on a deployer or depot used the block (`26d0af2`); actors break, cut, harvest; interfaces move items |
| Packages, frogports, chains | packager, repackager, frogport, chain conveyor, cardboard | Pass | Placed and drawn; packager makes addressed packages |
| Stock | link, ticker, gauge, requester, table cloth shops | Pass | Rows 1, 2, 11, 12 |
| Redstone | links, latches, pulse diodes, analog lever, contacts, nixies (16), display link and board | Pass | Link pair transmits; latch holds; extender extends; facing contacts power; nixie shows `15` |
| Postboxes | 16 colours, 4 facings | Pass | Drawn; keep signs |
| Decor, stone, copycats | casings, 14 stones × cuts, ladders, bars, scaffolds, windows, doors, cloths, toolboxes, seats | Pass | Drawn; doors open; seats, toolboxes dye; copycats take blocks |
| Equipment | goggles, wrench, grip, potato cannon, wands, backtanks, diving gear, cardboard armour, foods | Pass | Drawn; goggles overlay on a motor |
| Schematics | table, cannon, schematic items | Pass | Screens open |

### Kinetics against Create's Java

The [create-fuzz](https://github.com/OlaHulleberg/create-fuzz) differential fuzzer runs random kinetic networks in
Create 6.0.11 (NeoForge 1.21.1) and in the engine and compares where they settle. Since the kinetic propagation was
ported from Create (#9: sources, networks and Create's order) and its state is saved and loaded as Create does (#10),
10299 of seeds 0-10299 match, comparator signals included; the one left is #7, which waits on the mod API. With network membership and sizes observed too (#12), blocks changed in place (#13, #15) and the day time set per case, 19996 of seeds 0-19999 match with none unstable; the four left (5215, 6655, 15359, 19199) are #7. Each fix, tried in game on the build
above:

| Change | Result | Evidence |
|---|---|---|
| A cog loop whose ratios disagree, nothing turning | Pass | Two small and two large cogs placed with `/setblock`: none breaks (one did before) |
| A motor placed facing a turning motor | Pass | Placed by the player and with `/setblock`: both stay; the second shows `Gen: -16`, turning at 16 with the first; the shaft turns between frames; same after reload |
| Three large cogs at right angles, then a motor | Pass | The x and z cogs break and drop, the y cog stays still, as in Create |
| A motor's speed set to overspeed the cogs it drives (#2) | Pass | Small and large cog, motor on the large one; 183 rpm pasted onto it from a clipboard: the large cog breaks, the motor runs at 183 |
| Kinetic propagation ported (#9) | Pass | Press line at 16; member motors placed by the player and with `/setblock` (`Gen: -16`, turning at 16); three large cogs: x and z break, y stays still; motor pasted to 183 breaks the conveying cog |
| A gauge's comparator after its network stops (#11) | Pass | Hand crank on a speedometer read by a comparator: 7 while turning at 32, 0 once the crank stops |
| A split shaft is one network member (#12) | Not gameplay-visible | Network size has no in-game readout and clutches and gearshifts add no stress; checked by the fuzzer (5299 of 5300, the other #7) and `split_shaft_is_one_network_entity` |
| A kinetic block's state changed in place (#13) | Pass | Motor at 16 facing north, `/setblock` to facing south beside a shaft: it keeps -16 and the shaft stays still, as in Create; after reload it turns at 16 and drives the shaft (`CreativeMotorBlockEntity.initialize`). A player's wrench, clicked outside the value box, turns a motor from east to down: 16 becomes -16 on the next tick (`switchToBlockState`'s `reActivateSource`) and stays -16 after reload |
| A speed change never re-enrols a detached member (247556e) | No gameplay path known | `KineticBlockEntity.setSpeed`/`onSpeedChanged` leave the network alone; `detached_member_speed_change_does_not_enroll_it_again` fails without it; fuzzer seeds 0-10299 unchanged |
| A motor changed in place to another axis (#15) | Pass | Motor up at -32, `/setblock` to west, then 32 pasted from a clipboard: turns at -32 with network capacity 0 and added capacity 16384, as in Create; after reload capacity 524288 (`initFromTE`, `addSilently`, `updateCapacity`). Right after the `/setblock` the saved data has no `Network`, as Create's `write` (it was merged, keeping the old one); reload turns at -16 in a fresh network |
| A block entity's owned data written whole (#16) | Pass | A table cloth given `Shop` and `Price` with `/data merge`, then an apple put on it by the player: its saved `TableCloth` is rewritten as Create writes it, without them (a merge kept them). Motor `/setblock` up → west: no `Network` left in `KineticState`; reload turns at -16 |
| A busy valve handle's value box (#22) | Pass | Copper valve on stone, turned at 32 by an empty-hand use, then red dye clicked on its centre while it turns: it becomes red (`showValue` hides the box while in use). The first build crashed on launch (a nested client-table borrow in `render_gui_post`), fixed before this check |
| A sequence context cleared when its run ends (#19) | Pass | Copper valve handle used once (45°): saved `Sequence` while turning at 32; once the use expires it stops and has no `Sequence` (it kept one before, `HandCrankBlockEntity.tick`) |
| A valve handle recoloured while turning (#21) | Pass | Copper valve on a shaft, used and dyed red within its use (ticks frozen across the two clicks): handle and shaft keep turning at 32 after the use ends, as Create in the same input (create-fuzz); dyed after the use ends, both stop. Still 32 after reload (not compared with Create) |
| A sequencer's next instruction attaches on the following tick (#20) | Pass | Motor, sequenced gearshift (default program) and shaft, powered by a redstone block: the shaft turns at 16, stops when the 90° turn ends, and the gearshift keeps state 1 while powered (`run(-1)`); the one-tick attach timing itself is checked by create-fuzz |
| An adjustable chain gearshift's flicker under signal changes (#18) | Pass | Motor and adjustable chain gearshift, a redstone block above toggled every 4 ticks (`/tick freeze`, `step 4`): it breaks at the 21st toggle, as Create does for the same input (create-fuzz) |
| Default states from Create (#17) | Pass | `/setblock` with no properties: encased chain drive `axis=y,axis_along_first=true,part=none`, clutch `axis=y,powered=false`, as Create's runtime defaults; a player-placed chain drive keeps `axis_along_first=true` after reload; fresh worlds load without errors |
| A pulley put into a driven belt (#24) | Pass | Motor, two x shafts joined by the belt connector (four segments, 16 rpm), a shaft used on the second segment: it becomes a pulley and the segments keep turning at 16 with network capacity 262144 (it was 0) |
| A belt's break cascade (#23) | Pass | Four-segment belt made with the connector, a pulley in the second segment, the start broken with ticks frozen: one tick later air, shaft, air, shaft, as Create for the same input (create-fuzz); the same after reload |
| Empty running belts tick (#25) | Not separately visible | A 1/8-block start offset and a one-tick handoff; checked by create-fuzz (both cases match, 129 seeds changed to match, none newly differ) |
| A depot used with the same item (#26) | Pass | 16 iron put on a depot, then one more iron used on it: the depot holds 17 and the hand is empty, as Create (`SharedDepotBlockMethods.onUse` gives back before placing) |
| A belt stack behind one that enters a depot (#27) | Not separately visible | One tick of follow-on movement; checked by create-fuzz (the three item cases match, 129 seeds changed to match, none newly differ) |
| A gearbox's and a split shaft's own speed (#28) | Pass | Motor east into a gearbox: the gearbox's saved speed is the incoming 16, the side shaft 16, the far shaft -16, the same signs as Create for that input (create-fuzz), and the same after reload |
| A crank's or valve's use saved and restored (#29) | Partly in game | A crank turned with ticks frozen saves `HandleState` with `InUse: 8` while turning at 32 (written only when it changes), and the world reloads without errors; the 8 ticks run out while the world loads, so the restore itself is checked by `crank_use_survives_cold_load`, `valve_use_survives_cold_load` and create-fuzz's reload cases |
| Saved network figures from the entity's cache (#33) | Pass | Motor and two shafts, one shaft removed, saved and reloaded: saved `Size` 1 on the motor, 3 then 2 on the shafts, the same after reload, exactly what Create saves for that input (create-fuzz: each entity's `networkSize` changes only when its network syncs) |
| Reloads keep depots' held items and network members (#30, #34) | Pass | A motor driving two shafts and a depot holding two diamonds (put on by the player), saved and reloaded: speeds 16, saved sizes 1 and 3, the depot still holding both diamonds, no errors; the sliding position and the transient member counts are checked by create-fuzz's reload cases |
| A loaded belt forms its controller before its first tick (#31) | Pass | A six-segment belt made with the connector, iron dropped on it, saved and reloaded: `BeltState` (controller, index, length 6) and speed 16 kept, no errors; item positions on the first tick after load are checked by create-fuzz's reload cases |
| Entity state published after every subsystem's tick (#32, #35) | Pass | A sequenced gearshift running its default turn, ticks frozen and stepped by 3: its saved `Timer` 3, 6, 9 and `InstructionProgress` 14.4, 28.8, 43.2 follow the live run each step (it was saved every 10 ticks); a depot filled by a belt is published after the belt phase, checked by create-fuzz's reload cases |
| A belt's saved inventory follows its items (#36) | Not separately visible | Belt items live in the column attachment, which `/data` can't show; checked by create-fuzz (the case matches: saved positions follow the moving items, published only when changed) |
| Loaded entities: sequencer timing and first-tick network enrolment (#37, #38) | Partly in game | A sequencer saved at tick 8 of its 21-tick turn reloads and ends its turn normally (the rest runs out while the world loads), with no errors; the continued timing and the enrolment order are checked by create-fuzz's reload cases, all matching |
| Fluid tanks split and reform as Create (#39, #40) | Pass | A 2x2x3 tank made with `/fill`, a lower corner broken: the upper 2x2x2 reforms as one tank (bottom at y+1, top at y+2) and the remaining lower block stands alone, as Create; window flags stay per member (create-fuzz) |
| Chain drives after a break, chain gearshift signal on load (#44, #48) | Pass | Three chain drives (start, middle, end), the middle one broken with ticks frozen: both ends are `part=none` after one tick, as `ChainDriveBlock.updateShape`; the saved signal on load is checked by create-fuzz's reload cases |
| Network caches of loaded entities (#43, #45) | Not separately visible | Cached network figures after a load (a ghost-only network, a handle expiring on its first tick); checked by create-fuzz (both cases match, 218 seeds changed to match); a world with a crank reloads without errors |
| Depots and belts tick in entity order; first empty-belt direction (#46, #47) | Pass | A 3-long belt at 16 rpm feeding a depot registered after it, ticks frozen and stepped: the arriving ingot is at `Pos` 0.125 on its first tick (it slid in the same tick, as `DepotBehaviour.tick` after the belt) and settles at 0.498; saved mid-slide at 0.219, it reloads and finishes its slide with no errors; the depot-first order and the empty belt's direction are checked by create-fuzz (both cases match, 49 seeds changed to match) |
| Fluid valve pointer: placed state and saves (#41) | Pass | A valve set with `enabled=true` and no rotation starts at `Pointer` 0 and is `enabled=false` after its first tick, as `FluidValveBlockEntity`'s constructor and `tick`; turned by a 16 rpm motor to 0.55, the motor removed (pointer stops at 0.6), saved and reloaded: `Pointer` still 0.6, the valve still shut, no errors; create-fuzz checks the reload case (matches, 19 seeds changed to match) |
| Interactions published when accepted (#42, #53) | Pass | Ticks frozen, a creative motor driving a shaft set from 16 to 96 rpm with a clipboard: the motor's and the shaft's saved `Speed` read 96 at once; saved and quit without a tick, the world reloads with both at 96 (before, the save kept 16); create-fuzz: 147 seeds of 0-1999 changed to match, none newly differ |
| A handle reloaded mid-use runs out as Create (#50, #54) | Pass | A copper valve handle on a shaft, used with ticks frozen: after 3 ticks `InUse` 4, network capacity 256; saved and reloaded, the use runs out and the shaft stops with no capacity left (before, 256 stayed); #54 was a create-fuzz driver bug (fixed there); create-fuzz: seeds 298, 314, 490, 506 changed to match, none newly differ |
| Tank reconnection and valve opening on the entity's tick (#51, #52) | Pass | A 2x2x2 tank from `/fill`, a lower corner broken with ticks frozen: one tick later the upper layer is one 2x2x1 tank and the lower blocks stand alone, as `removeController` → `updateConnectivity` on the next tick; a valve on a 16 rpm motor opens (`enabled=true`, `Pointer` 1.0) within a second, no errors; create-fuzz: seeds 182, 744, 1128, 1320, 1586, 1704 changed to match, none newly differ |
| Sequence, depot motion and generator updates across reloads (#56, #57, #58) | Pass | Ticks frozen: an item put on a depot is saved at once with `HeldItem` `Pos` 0.25 and, after a reload, finishes sliding to 0.498; a valve handle used is saved at once with `Sequence {Value 45, Run 0}` and `InUse` 7, and reloads without errors; #57 (facing motors reapplied before a reload) checked by create-fuzz; seeds 1322, 1338 changed to match, none newly differ |
| Directed speed modifiers along a chain (#55) | Pass | A creative motor (16 rpm) driving gearshift, drive, gearshift, drive, gearshift (axis z, chained along x): unpowered all 16; the first gearshift powered, the rest 32; both ends powered, 16 32 32 32 16; only the last powered, 16 16 16 16 8; no block breaks, no errors; which block a conflict destroys is checked by create-fuzz (the case matches; seeds 684, 700, 812, 828 changed to match) |
| Pending tank connectivity and network admission across reloads (#59, #60) | Pass | Ticks frozen, a player places a fluid tank on another: both stand alone (`top=true,bottom=true`) with connectivity pending; saved and reloaded, they form one 1x2 tank (`bottom=true,top=false` / `bottom=false,top=true`), as `Uninitialized` → `updateConnectivity`; network admission after an early reload checked by create-fuzz (both witnesses match, seeds 1256 and 1328 changed to match, none newly differ) |
| A valve handle recoloured mid-use keeps its source tree (#69) | Pass | A copper valve handle on a shaft, used (`InUse` 7, shaft 32 rpm), then dyed red with ticks frozen: it becomes `red_valve_handle`, keeps `InUse` 7 at 32 rpm, and the shaft's `Source` is still the handle; saved and reloaded, it stays red and comes to rest once the use runs out, no errors; the same-tick recolour and reload checked by create-fuzz (1386, 1402 changed to match, none newly differ) |
| Pipes connect to vanilla fluid targets (#67) | Pass | A fluid pipe placed by a player on the floor east of an empty cauldron becomes a straight east-west pipe (`east=true,west=true`, others false), as Create's connectivity to vanilla fluid targets; create-fuzz's witness matches; no seed changed status |
| Fluid machines in block-entity order; depot hold boundary (#65, #66) | Pass | A water bucket used on an item drain: the drain's tank reads 1000 of 1500 mB of water, no errors; the shared registration-order walk for drains, spouts, pumps and belts, the depot's held-item lock and drain turn timing are checked by create-fuzz (drain seeds 147 and others from 67 to 36 differing fields, none newly wrong; no seed changed status; pressure fields await #68) |
| Pipe connections with stored pressure (#61-#64, #68) | Pass | A creative motor drives a mechanical pump (facing east) through a cogwheel; a water source west of the pump through a pipe, a fluid tank east through a pipe: the pump's east connection saves `Pressure [0, 16]` with a flow in progress, and the tank fills 192 → 992 → 1000 mB of water within seconds while the source drains; saved and reloaded, the tank keeps 1000 mB; create-fuzz: 109 seeds of 0-1999 changed to match, none newly differ. Separate: with water placed in front of the pump's outlet the integrated server disconnects (also on 5dddd33, before this change), filed on its own |
| Drawn contents published only when they change (#70) | Pass | The test world that disconnected about 55 s after every load (a settled depot republishing its redraw marker every tick, 940 identical updates before the keep-alive timed out) stays connected past 120 s; the depot's `Shown` snapshot is unchanged between reads; a depot and a belt still redraw when diamonds are swapped for emeralds (agent's instance, 291 s connected) |
| Valve speed callbacks published before a save (#72) | Pass | Ticks frozen, a fluid valve set `enabled=true` beside a turning creative motor: after one tick its `Pointer` chases target 1.0 at 0.05; saved and reloaded it keeps target 1.0, reaches 1.0 and stays `enabled=true`, as Create's `onSpeedChanged` → `pointer.chase`; create-fuzz: both cases and seeds 7989, 7990, 7993, 10292-10297 match, no seed changed status in 0-1999 |
| Pipe pressure kept across a reload (#73) | Pass | Two mechanical pumps in series, each turned through a cogwheel by a creative motor, a pipe between: the pipe's connections hold `Pressure [0, 32]` / `[32, 0]`; saved and reloaded they read the same, and still the same 10 s later (before, a reload doubled them); create-fuzz: seeds 13810 and 17330 match, no seed changed status in 0-1999 |
| Sunrise nightly pin (2d81263) | Pass | Official nightly client, fresh world: a creative motor turns a shaft at 16 rpm, a belt carries a diamond onto a depot, a pump fills a fluid tank to 8000 mB, a valve opens (pointer 1); saved and reloaded, all four keep their state and the tank keeps filling |
| Clutch and gearshift placed beside power (#7) | Pass | Nightly game: a clutch and a gearshift placed by the player and by /setblock beside a powered block place `powered=true`; one set `powered=true` with no signal unpowers on its next tick; create-fuzz main 62f53ab: `fuzz 0 2000` 2000 matched, the 13 `powered` seeds of the 42c51d2 sweep fixed |
| Nightly APIs, blocks and world | Pass | Lava beside honey/chocolate (source → obsidian, flowing → limestone/scoria), by /setblock and bucket, kept on reload; drill and deployer breaks and a water wheel's material change play the destroy-block effect; symmetry wand mirrors fit fences and walls; wrench turns fit neighbours and survive reload; hand crank, valve handle, placard and factory gauge break when their support is removed, survive against fence collision in all four facings; factory gauge outlines kept on reload; requester placed by a deployer keeps its address; toolbox, backtank and roller placed facing south and east keep their states over two reloads; schematic export and print keep item frames and paintings |
| Nightly APIs, items, entities and client | Pass | Schematicannon shift-click sends a schematic, books and 64 gunpowder to slots 0, 2 and 4, kept on reload; deployer with a diamond sword hits a cow for 7 (full charge); cardboard sword hit stops a sprint; `/tick freeze` + `/tick step 5` keep a turning shaft at rest, `/tick unfreeze` resumes; fan pushes faster at 256 rpm; depot and belt draw stone 3D and diamonds flat; extendo grip pose with a block in the main hand; turning bearing, piston and clockwork bearing structures lit by the world around them; a bearing stopped mid-turn keeps its angle (43.2°) over a reload (fixed in 296b723). Not observable here (no audio device): bonk and fan pitch |
| A windmill's angle saved (#80) | Pass | Nightly game: a windmill (16 sails) turning at 2 rpm keeps its angle over two save/reloads (208° then 351°, on its 0.6°-a-tick line); taken apart after a reload its sails go back unturned, also with a glued drill; a mechanical bearing reversed 8 → −16 rpm rounds its angle to a whole degree (276.00012 → 276), and stopped in never-place mode it keeps −46° over a reload; create-fuzz `fuzz 0 2000` 2000 matched |
| Chain drives join and re-link in the block change (#94, #97, #98) | Pass | Nightly game: breaking a drive's chain partner re-links it with the drive above at once (start/end along y); a lone drive wrenched into line with a gearshift becomes x/end and the gearshift start in the same tick, both turning; gearshift, drive, gearshift placed by a player join as start, middle, end at once; breaking the middle drive leaves both ends `part=none`; all kept over a save and reload; create-fuzz (kgap-network) `fuzz 0 2000`: 54 seeds fixed, none newly differing, the six issue cases match |
| Generators keep their state over a reload (#89, #90, #91) | Pass | Nightly game (ed3ac19): a flowing water wheel (8 rpm, `FlowScore 1`, network 256/1), a large water wheel (4 rpm, network 512/1, 8 frame cells) and a steam engine on a 2×2 boiler with 4 kindled burners (64 rpm, network 16384/1, `EnginePower 1.0`) come back the same after save/quit/reload (the boiler's supply is re-sampled, as Create's); breaking a large wheel's frame corner with ticks frozen and stepped: the wheel goes the next tick, the four cells beside it the tick after, the corners the tick after that, as `WaterWheelStructuralBlock.updateShape`/`tick` decide; create-fuzz (kgap-generators) `fuzz 0 2000`: 2000 matched, 86 seeds fixed, none newly differing |
| A sneak-use with an item is the block's own use on cranks, valve handles and levers (#96) | Pass | Nightly game (ed3ac19): a hand crank, a valve handle and an analog lever each used with an empty hand, a shaft and an extendo grip (sneaking and not) and the wrench: sneaking with an item turns the crank and the valve backwards and lowers the lever, nothing is placed from the held stack, the wrench keeps its own use; states kept over a save and reload, clicks still work after it; create-fuzz `fuzz 0 2000`: 31 seeds fixed (the held-crank family), none newly differing |
| Bearings assemble on a speed change, not on a reload (#106) | Pass | Nightly game (ed3ac19): a mechanical bearing assembles when powered, comes apart by an empty-hand right-click (aimed off the side faces' value box, where Create puts it), and stays apart over a reload while still powered (before, every reload re-assembled what stood in front of it); assembled again by right-click; reloaded while turning it keeps `Rpm: 16` and comes apart; a windmill keeps `Rpm: 3` over a reload; a player-placed bearing and a glue box assemble; create-fuzz `fuzz 0 2000` 2000 matched |
| A wrench clicks an analog lever without sneaking (#111) | Pass | Nightly game (ed3ac19), the lever's level read from a redstone wire beside it: a standing wrench clicks it up (7 → 8 → 9), a sneaking wrench leaves it (7), an empty hand or a stick clicks up standing and down sneaking; the lever is never picked up; its level survives a save and reload (7, then 9); create-fuzz `fuzz 0 2000` 2000 matched |
| Sequenced gearshifts run from their scheduled block ticks (#99, #100) | Pass | Nightly game (ed3ac19): two sequenced gearshifts in a line powered by one `/fill`: the first starts (`state=1`, instruction 0), the second stays idle (`state=0`, `InstructionIndex -1`, `Duration -1`, `PrevPowered 1b`), as Create's `willTickThisTick` decides; a programmed gearshift run to its end, unpowered, saved, reloaded and repowered restarts its program (`state=1`, instruction 0, its far half turning); player placement and `/setblock` give the same state and data; create-fuzz (network inputs): the 31 `sequencer_pair` seeds fixed, none newly differing |
| Network size around reloads and between opposed motors (#95, #101) | Pass | Nightly game (520fc48): a shaft placed between two motors turning against each other breaks with its drop, and both networks keep ±16 rpm with their sources and size 2; a gearshift powered after a reload flips its output to -16 with the input at 16, and a relaunch with the power in place keeps `powered=true` and +16/+16/-16; create-fuzz `fuzz 0 2000` 2000 matched |
| Belt slicer follows BeltSlicer (#130, #131 except f2b, #132, #133) | Pass | Nightly game (520fc48): a belt shortened from its START and END with the wrench, lengthened and split in creative (the cut-off half stays still while the driven half carries its items) and joined again with the connector after a save and reload; a survival split is refused without a connector; create-fuzz `fuzz 0 2000` with main's driver unchanged from main |
| Belt tunnels, funnels, chutes, inventory and item handler (#134-#140) | Pass | Nightly game (520fc48): wrench-closed andesite and brass tunnel windows stay closed through 60 ticks and a neighbour change; breaking a segment takes its belt and four tunnels in one tick; a belt funnel fills a depot and the rest queue; a hopper inserts into a belt and stops when the segment is blocked; stacked chutes fill an empty depot and wait in the middle over a full one; items picked off a stopped belt are gone within 3 ticks; all kept across a save and reload; create-fuzz `fuzz 0 2000` fixes 21 seeds, none newly differing |
| A large water wheel's frame cell hands the sneak wrench to its wheel (#165) | Pass | Nightly game (520fc48): a sneak wrench on an edge frame cell takes the wheel into the inventory and the frame goes a moment later; a small wheel turned beside flowing water reads flow 0, speed 0 and keeps it across a relaunch; create-fuzz `fuzz 0 2000` 2000 matched |
| Sequenced gearshifts tick in block-entity order (#173) | Fuzz only | create-fuzz case and seeds 2356, 4788, 7220, 9652, 12084 match; not reproduced in game on `9425651` (see that smoke test, item 7) |
| Brass tunnels, funnels and the logistics tick order (#143-#149, #154) | Pass | Nightly game (`9425651`): see that smoke test, item 8; create-fuzz `fuzz 0 2000` fixes 26 seeds |
| Kinetic state saved and loaded (#10) | Pass | Two member-motor pairs keep their roles over two save/reload cycles; a world saved before #10 loads with both pairs intact and keeps them after saving again |

Known differences left: the category editor's inventory shows the inventory as it was at the last change made in
it, so items picked up while it is open appear after the next change (Create's menu syncs them at once). The game
dropped to the title screen ("End of stream") five times across three test sessions with no panic or crash report;
none reproduced.

## Potato projectile on `9425651`

`tools/check-potato-projectile.py` passed with the stock native package, the signed SDK, and the verified Create jar. A potato took a pig from 10 to 5 health and consumed one round; with no backtank, the cannon's existing wear stayed unchanged (see `API_GAPS.md`). A shallow shot planted one crop. An offhand cake took a pig to 2 health and used only the cake.

The disposable fixture checks ten cases. The join callback sees initialized ammo. A join veto keeps the dual-wield turn, and changing the selected slot at join leaves both the cannon and main-hand ammo unspent. Changing it on the ammo debit leaves the pickaxe alone; that callback sees the turn marker on the original cannon and four air already taken from the backtank. A sky shot keeps flying when `clip` gives no answer. A hit from an absent shooter has the shot's direct source but no causing entity; a second shot whose shooter returns before impact has both. Flight keeps the same shot and owner UUIDs on reopen and resumes at the independently calculated next position and velocity. Two in-game screenshots from opposite oblique angles show the face of the synced potato, rather than its edge. A sticky cake drops one round into the world with the inventory full, follows its target one block sideways and hits a second pig after reopen without recovering another round. Both the full and partially vetoed sweet-berry volleys hit on their first step, spend one berry and recover at most one, on the first admitted shot.

To reproduce on Linux, set `GAME`, `CTL`, `RESOURCES` and `JARS` to absolute paths for the signed Nightly client and matching control tool, a private prepared resource tree, and the verified jar directory. From the repository root:

```sh
(cd create && xvfb-run -a cargo sunrise-mod run --game "$GAME" --resources "$RESOURCES" --jars "$JARS" -- --frames 1)
xvfb-run -a python3 tools/check-potato-projectile.py --ctl "$CTL" \
  --game create/target/sunrise-mod/run/app/sunrise-client --resources "$RESOURCES" \
  --jars "$JARS" --scratch target/potato-smoke
```

In a **disposable checkout**, apply `git apply tools/fixtures/potato-projectile.patch`, repeat the staging command with that checkout, and append `--fixture` to the test (or `--fixture --case flight` for the two billboard screenshots). Do not ship the instrumented build or publish the screenshots. Dedicated multiplayer, a free camera and flight prediction were not checked.

## Issue #459: clutch and gearshift power in game on Dev `ba1f24e` (branch `fix/clutch-power`)

Built with launcher 1.3.7 `native-build` against `dev-2026-11-26-ba1f24e`, a fresh creative world, driven by `sunrise-ctl`:
a creative motor, a shaft, a clutch (and beside it a gearshift), a shaft and a cog, each with a lever on top flipped by the
player. Speeds read back with `/data get` right after each click; a temporary debug log (removed before the build below)
showed the clutch's state write delivered inside the neighbour change and the re-attach in the next tick's block ticks.

| Check | Result | Evidence |
|---|---|---|
| Clutch powered | Pass | The cog reads 0 rpm right after the click and stands still across frames |
| Clutch unpowered | Pass | The cog reads 16 rpm right after the click and turns |
| Gearshift powered and unpowered | Pass | The cog reads −16 rpm, then 16 rpm, right after each click |
| Save and reload | Pass | Powered clutch and gearshift reload as they were (0 and −16 rpm); after it, each still follows its lever at once |

## Fixes #227-#234 in game on `9425651` (main `11c43ca`)

Built with `sunrise-launcher native-build` against `nightly-2026-11-20-9425651`, a fresh creative world, driven by
`sunrise-ctl`, read back with `/data get`, `/tick freeze` and `step`, and the entity list. #228 is above; #230, #232 and
#233 change only the fuzzer and docs.

| PR | Item | Result | Evidence |
|---|---|---|---|
| #227 | Tunnel placement | Pass | An andesite tunnel is refused on stone and on an upward belt segment; on a flat belt, with ticks frozen, the segment is `casing=true` at once, saved `Casing: "andesite"`, kept over a reload |
| #227 | Brass tunnel group | Pass | Two brass tunnels on side-by-side belts placed with ticks frozen save `ConnectedRight`/`ConnectedLeft` at once; saved and reloaded before a tick, both kept |
| #227 | Lost side's filter | Pass | A depot beside the belt opens the tunnel `t_left`; breaking it shapes the tunnel `straight`: a diamond side filter goes, a `create:filter` one drops as an item |
| #227 | Funnel filter change | Pass | A brass funnel on a chest with `TransferCooldown` 400, its filter clicked to cobblestone by the player: 380 after 20 ticks, the chest untouched until it ran out |
| #227 | Live countdowns over a reload | Pass | Ticks frozen: funnel `TransferCooldown` 336, burner `Remaining` 1566, drain `ProcessingTicks` 8 (20 → 8 as it drained a bucket) saved; after the reload's 13 ticks 323 and 1553, the drain done with 1000 mB |
| #227 | Fan after a reload | Pass | An ingot in front of a 16 rpm fan reloads moving at 0.021 a tick and keeps being pushed |
| #227 | Depot to a pulling funnel | Pass | A belt-fed depot under a pulling brass belt funnel: the gold slides 0.125, 0.219, 0.289 … 0.45 and is in the chest on the 9th tick, still sliding. A spout above could not be filled by `/data merge` or a bucket, so the order against it was not seen |
| #227 | Slicer at the end | Pass | Redstone on the end segment of a 16 rpm belt, the end cut off with the wrench: thrown at (−0.125, 0.125), the belt's way |
| #227 | Ejector nudge | Pass | A gravity-free stick inside an ejector's block rises 151.5 → 151.625 as a landing ingot activates it; the ingot launches the next tick |
| #231 | Pump into a junction over a tank | Pass | Cauldron, pipe, pump, pipe, junction beside a tank, pipe (the fuzz case's layout): the tank fills 80 → 656 → 1000 mB; reloaded 8 mB into a refill, it goes on 2144 → 2496 → 2992 |
| #234 | Hopper into millstone, crusher, saws | Pass | Wheat milled to flour and seeds; dirt is taken too (`API_GAPS.md`). Crusher wheels turning down take one cobblestone at a time, 3 gravel out below (turning up, results are shot up into the hopper and crushed again, as Java). An upward saw takes one log at a time and throws stripped logs; an east-facing saw takes one and holds it |
| #234 | Breaking a saw or crusher | Pass | The east-facing saw drops its cobblestone in its block; a wheel broken mid-crush drops the controller's cobblestone |
| #234 | Reload mid-process | Fail | An upward saw reloaded mid-cut keeps its log while turning at −16 and never cuts it: `saw_turn` is never called for it (a temporary debug log; a saw filled after the load is called every tick). Repro: saw `facing=up` driven at 16, a hopper above with one oak log, save and quit within 2 s, relaunch. The crusher finished across a reload |
| — | Millstone progress over a reload | Fail, not from these merges | 104 of 150 ticks done at the save, the flour came about 155 ticks after it: the port saves no millstone `Timer` (`MillstoneBlockEntity.write`); `millstone.rs` is unchanged by #227-#234 |
| — | Hopper into a depot | Fail, not from these merges | The stack lands in slot 0 but the depot never holds it (no `HeldItem`, not drawn, a pulling funnel leaves it): at the commit `depot_contents_changed` reads slot 0 empty (a temporary debug log); unchanged since `b7b000d` |

## Fixes #203-#224 in game on `9425651` (main `c6f9470`)

Built with `sunrise-launcher native-build` against `nightly-2026-11-20-9425651`, a fresh creative world (survival where
noted), driven by `sunrise-ctl`, read back with `/data get`, screenshots and the player's state.

| PR | Item | Result | Evidence |
|---|---|---|---|
| #203 | Spout fill across a reload | Pass | Ticks frozen mid-fill at `ProcessingTicks` 16 (started tick 354), saved and reloaded: the next fill started at tick 375, so the first ran its 20 ticks, not restarted; all four bottles filled |
| #203 | Press over a depot | Pass | Three iron pressed one at a time into three sheets, held at `Pos` 0.5 |
| #206 | Ejector incoming stacks | Pass | Redstone-held ejector turning at 16 holding 5 iron, a hopper above, ticks stepped: each arrival sits in `Incoming` from `Pos` 0.125 to the centre, then joins (5, 6, 7 … 10) |
| #208 | Rest of a pressed belt stack | Pass | Four iron on a belt under a press all became sheets and moved on to the end depot |
| #208 | Funnel cooldown across a reload | Pass | `TransferCooldown` 400 set with `/data merge`, saved and reloaded: the next extraction came 14-20 s later (8 ticks without the saved cooldown) |
| #208, #209 | Brass tunnel targets and clicks | Pass | Two grouped tunnels: holding 16 iron it saved `Targets` for both; a standing wrench click took the 16 into the inventory, `shape=straight` kept; targets and shape kept over a reload |
| #210 | Fan progress cleared by a fill | Pass | A glass bottle on a depot in a blasting current (`FanProcessingTime` 111 and falling), filled by the spout: the water bottle restarted at 150 |
| #214 | Interfaces on a bearing | Pass | A glued arm with an interface and a chest docks once a turn with a stationary interface; a chute above put 128 cobblestone into the arm's chest, and the arm kept turning. Not tried on a cart |
| #215 | Standing on and riding structures | Fail | Stood on a turning bearing and on a cart (after a reload). A piston lift (2 poles, 16 rpm) carried the player up 152.0 → 153.875, then pushed them off sideways as it stopped: the rider is 1/8 below the top when the blocks are placed, and Create's `moveCollidedEntitiesOnDisassembly` (`AbstractContraptionEntity.java:690`) has no port. A cart assembled at rest left its structure and collider at the assembler when pushed |
| #216 | Belt lengthening | Pass | A 5-long belt driven at its end, lengthened at its start in creative: the motor and the end save size 6, the new segment 7; middle segments 6 to 3 in chain order (checked by create-fuzz) |
| #217 | Copycat friction and light | Fail | Sticks slide the same 1.3 blocks on ice copycat steps and empty ones (7.8 on ice). The friction hook gets no block entity data, so it reads air. A glowstone copycat made by a player gives block light 0 until a reload (15 after) |
| #217 | Deployer harvest | Pass | Punching stone two blocks ahead: a stick leaves it, an iron pickaxe breaks it and drops cobblestone |
| #217 | Diving helmet and boots | Pass | Survival, under water with a backtank holding `create:backtank_air` 900: air held at 300 for 15 s while the tank went 897 → 884. Boots sink at −0.21 per tick, −0.025 without |
| #217 | Coupled carts | Not tried | `sunrise-ctl` can't target minecarts |
| #218 | Boots on a sloped belt | Pass | 5-long upward belt: carried with no boots and with cardboard boots, not carried with diving boots |
| #218 | Toolbox fed by a hopper | Pass | Stone, dirt and a shulker box: stone to slot 0, dirt to slot 4, the shulker box stays |
| #219 | Basin recipes | Pass | Pressed: 4 honey bottles to a honey block and 4 glass bottles (slots 9, 10); 18 nuggets to 2 ingots |
| #220 | Filter screen | Pass | A ghost slot copies one diamond and the cursor keeps 5; shift-click fills the next ghost slot; clicking the held filter does nothing; deny saved; all kept over a reload |
| #220 | Campfire smoke on a cart | Fail | A lit campfire on an assembled minecart contraption, moving and stopped, before and after a reload: no smoke |
| #221 | Old world migration | Pass | A world saved on `7b4c440` (motor 64 rpm, depot 5 diamonds, tank 2000 mB water, glowstone copycat, basin 1 iron) opened with all of it in entity data (`create:fields`), the same after a second reload; light 15 at the copycat |
| #221 | `/data merge` | Pass | Reached a spout (`SpoutTank`), a funnel (`TransferCooldown`), a motor (`ScrollValue`, applied as Create's `read` does: the next change) and copycats |
| #221 | Placing with data | Fail | A brass funnel with `Filter` diamond filters at once, but its record has no `Filter` and after a reload it lets dirt through. A blaze burner with `BurnerState` places smouldering |
| #222 | Drops from broken blocks | Pass | Survival: an andesite casing broken by hand drops nothing; with an iron pickaxe it drops itself |
| #222 | Belt eject | Pass | Items off the end of a running belt with no receiver land just past its end (x −1.29) |
| #224 | Steam engine across a reload | Pass | 2×2 boiler, kindled burner, pump supply 48: engine at −64, capacity 16384; saved and reloaded the same, `Remaining` fuel 1387 → 1362 |

The four failures above were fixed in #228 and tried again in game on `9425651`, on a clean build of that branch:

| PR | Item | Result | Evidence |
|---|---|---|---|
| #228 | Piston lift rider | Pass | The same lift: the rider stays on the plank at y 154.0 when it stops (`moveCollidedEntitiesOnDisassembly`); nearby items and carts untouched |
| #228 | Copycat friction | Pass | A stick slides 7.8 blocks on ice copycat steps, 1.3 on empty ones; both sides read `minecraft:ice`. Light: the server answers 15 at once; the client shows it after a chunk resend (API_GAPS) |
| #228 | Campfire smoke on a cart | Pass | `campfire_cosy_smoke` at rest, rising from the moving campfire, and after a reload; a planks cart gets its collider boxes |
| #228 | Placing with data | Pass | The funnel with `Filter` diamond passes only diamonds from its first tick, keeps `Filter` and still holds back dirt after a reload; the burner places `kindled` with `BurnerState` read |

## In-game smoke test on `9425651` (main `684e89b`)

Built with `sunrise-launcher native-build` against `nightly-2026-11-20-9425651`, resources prepared by that build's
`sunrise-prepare` (output format 5; the format-4 resources crash its client with `missing field 'build'`), a fresh
creative world, driven by `sunrise-ctl`.

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | Creative motor turning shafts | Pass | 16 rpm on both shafts; motor `Speed: 16.0f`, capacity 262144 |
| 2 | Belt carrying a diamond onto a depot | Pass | 5-long belt made with the connector by a player; the depot holds the diamond after about 4 s |
| 3 | Pump filling a tank | Pass | 8 mB per tick from a pool, 2288 → 3448 mB, then full; a single source gives 1000 mB and becomes air; a bucket puts 1000 mB in; with air ahead the pump places a source |
| 4 | Fluid valve opened | Pass | Pointer `Value: 1.0f`, `enabled=true` |
| 5 | Bearing and windmill | Pass | 4 glued planks turn at 16 rpm; a 24-sail windmill at 3 rpm, 1536 SU |
| 6 | Player placement and breaking | Pass | Shaft (`axis=x`), gearbox (`axis=y`) and cogwheel (`axis=z`) placed against turning blocks turn at 16 rpm; broken in 1-2 ticks with the neighbour still turning; placed again and turning |
| 7 | Sequenced gearshifts saved at a program's last tick (#173) | Not reproduced | After a relaunch every block read 0 rpm and 0 SU, the second gearshift with no network. Java's 256 SU needs the run to end on the reloaded tick; here about 21 ticks passed before the first read and the crank may have stopped before the save. The fuzz case for #173 matches |
| 8 | Brass tunnels and funnels (#143-#149, #154) | Pass | Split, round robin and synchronize each delivered in Create's order to three depots; a tunnel replaced in a synchronizing group took its mode; a belt funnel pushed a chest onto a belt and waited when it backed up; a funnel over a depot pushed one ingot and kept the rest; a pulling funnel took a depot's item into a chest |
| 9 | Save, quit, relaunch | Pass | Speeds, depot items, valve, bearing and windmill (still turning), tunnel modes and funnel shapes kept, and all went on working; the tank went on filling |

Game log: no mod errors or crash reports once the resources matched the build. Unconfirmed: a sneak-placed funnel came
out extracting (Create's `FunnelBlock.getStateForPlacement` makes it non-extracting, as the port's placement does);
whether `ctl place` sent the sneak was not checked.

## In-game smoke test on `520fc48` (main `6373106`)

Built with `sunrise-launcher native-build` (launcher 1.1.1; 1.0.1 rejects the scaffolding data's
`context_collision` field) against `nightly-2026-11-19-520fc48`, a fresh creative world, driven by `sunrise-ctl`.

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | Creative motor turning a shaft | Pass | `create_debug speed`: 16 rpm, stress 0 / 262144 su; motor and shaft `Speed: 16.0f` |
| 2 | Belt carrying a diamond onto a depot | Pass | 5-long belt at -16 rpm; depot `Items: [{count: 1, id: "minecraft:diamond"}]` |
| 3 | Mechanical pump filling a tank | Pass | One source: 1000 mB; refilling pool: 48 → 544 → 1048 → 1544 → 2040 → 2544 mB at 3 s intervals, then 8000 (full) |
| 4 | Fluid valve opened | Pass | Closed: tank 0 for 8 s; driven at 16 rpm: `Pointer` value 1.0, `enabled=true`, tank 824 → 2302 mB |
| 5 | Bearing and windmill | Pass | Super-glued planks assembled at 16 rpm; 8-sail windmill at 1 rpm, 512 su; both seen turning |
| 6 | Player placement and breaking | Pass | Shaft, cogwheel, gearbox and side shaft placed against blocks turn at 16 rpm; breaking the cogwheel stops the gearbox, re-placing restarts it |
| 7 | Save, quit, relaunch | Pass | All 14 speeds read back the same; depot keeps its diamond and takes the next once emptied; tanks keep 1000/8000 mB, a two-high tank goes on filling; bearing (`Angle: 48.0f`) and windmill keep turning |

Game log: no panics, crash reports or Create errors.

## Issue #17: declared-property defaults only (2026-11-17)

Branch `fix/issue-17-declared`, based on `f4a7f18`, leaves the held `fix/issue-17`
branch at `681df08`. Only properties declared by a block take their default from
Create's runtime measurement. The measurement is byte-identical to the one documented
in `41e60e4`: create-fuzz's server writes `work/default-states.json` at startup.
Main's existing partial `TEMPLATE_PROPERTIES` table and lookup are unchanged, and every
emitted override for its known inherited properties was compared against main: no changes.
No vanilla-report extract or new template facts are included.

Two exact pending lists contain one reason per block/property pair, each awaiting the
owner's template-data decision: **975 unlisted properties across 263 blocks**, and
**9 differing inherited defaults across 9 blocks**. The first includes the previously
identified 61 modelling omissions and properties the partial table cannot resolve.
These lists do not emit defaults or change main's inherited-property handling.

Default keys must exist among a block's declarations or main's table. Shape references
are checked there too, with explicitly tracked unresolved Create runtime properties
remaining pending. Spout, turntable and frogport shapes are constant as their Java
`getShape` methods specify; they need no artificial state selector.

- Rust 1.99.1 formatting and Clippy (`--all-targets -- -D warnings`) pass for engine
  and generator; generator Clippy/tests use `--locked`.
- All 691 engine tests and 49 generator tests pass, including both asset licence checks.
- The chain placement regression fails with main's old declarations; the property-key
  regression rejects the held branch's invalid pillar defaults; the domain-order
  regression fails when declared defaults revert to first-value selection. All pass
  with the narrower fix.
- Issue #17's shrunk case matches Java, including all checkpoints. The current fuzzer
  was copied to scratch, with its hardcoded chain default replaced by the generated
  declaration's value. Shared create-fuzz and its server were not edited or restarted.
  The full seed sweep was not repeated for this narrower follow-up.
- `hz_rebuild` succeeded. With no live non-zombie game process present, `FRESH=1
  hz_launch t17b` reached `screen=playing`, joined the local world and advanced to tick 19.
  No new crash report appeared during launch or after `hz_quit`. The tested declarations
  match the final generated declarations exactly.
