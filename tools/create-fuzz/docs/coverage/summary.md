# Coverage summary

JaCoCo 0.8.14 branch coverage of Create 6.0.11 (`b62e837`) on the fuzzer's server, after the
campaigns below. The measured input is generated run data and stays untracked (`work/coverage`);
`./run.sh fuzz cover` then `./run.sh fuzz gaps` rebuilds the per-group reports from it and from
[`coverage/reasons.tsv`](../../coverage/reasons.tsv). Every missed group has a reason: the reporter
finds no unexplained branch in any subsystem.

| Subsystem | Classes | Branches | Reached | Missed | Missed groups | Reached, excluding branches unreachable here |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| kinetics | 279 | 9626 | 2891 (30.0%) | 6735 | 3105 | 2891 / 6871 (42.1%) |
| belts and items | 84 | 4717 | 2077 (44.0%) | 2640 | 1293 | 2077 / 3676 (56.5%) |
| fluids | 84 | 3054 | 1399 (45.8%) | 1655 | 845 | 1399 / 2378 (58.8%) |

Classes counts those with branches. Kinetics is all of `content/kinetics` (it includes the belt
classes and every kinetic machine: deployers, arms, mixers, saws, fans and so on). Belts and items are
belts, depots and ejectors, funnels, tunnels, chutes and packages. Fluids are `content/fluids`. The
last column leaves out the branches whose reason is not `harness-gap`: client-only code, other mods,
data generation, schematics and contraptions, config, chunk lifecycle, error paths and dead code.

## Processing

Measured on a separate campaign of its own (`FUZZ_COVER_SEEDS_FROM=40000 ./run.sh fuzz cover 120`
at `26906e95`, its corpus 40 seeds of each of the five processing families and 16 each of
`belt/press_processing`, `belt/depot_outputs` and `belt/chute_columns`: 248 scenarios), against the
processing subsystem of [`subsystems.json`](../../coverage/subsystems.json): the machines under
`content/kinetics` and all of `content/processing`. `./run.sh fuzz gaps processing` on it finds no
unexplained branch. Of its 368 cases 285 matched, 78 differed (the port differences of the
[processing scope](../processing-scope.md)), 2 were java-unstable across batch contexts (seed 7885
of the millstone family, whose milling rolls, and 40110), 1 was refused by the Java side after a
mutation and 2 are documented API gaps.

| Machine | Classes | Branches | Reached |
| --- | ---: | ---: | ---: |
| processing (all) | 128 | 4683 | 857 (18.3%); 857 / 3380 (25.4%) excluding branches unreachable here |
| millstone | 4 | 82 | 62 (75.6%) |
| crushing wheels | 6 | 294 | 135 (45.9%) |
| saw (with the horizontal saw and tree felling) | 10 | 520 | 95 (18.3%) |
| press (over belts, depots and basins) | 6 | 216 | 84 (38.9%) |
| mixer | 3 | 128 | 46 (35.9%) |
| basin | 8 | 538 | 277 (51.5%) |
| blaze burners (with `HeatCondition`) | 10 | 476 | 67 (14.1%) |

Before the basin families (`7e13401`) processing reached 462 branches (9.9%). Missed now:
harness-gap 1173 groups / 2523 branches (deployers, arms, crafters, drills, entities in machines,
and what the basin families leave: the basin's filter, potions, belts and funnels feeding a basin,
a fluid target under its spout, regular burner fuel), client 416 / 982, error-path 27 / 47,
contraptions 88 / 218, datagen 10 / 22, schematics and contraptions 8 / 24, dynamictrees 1 / 2,
unreachable 8 / 8. Every group of the basin, burner, mixer and press classes has a row in
`reasons.tsv` naming its Create line and the input that would reach it; where JaCoCo's count leaves
open which edge is missed, the row says so. The mixer and press classes count as kinetics in the
table above, whose campaign never drove them, so their rows keep the category they had there.

## Block breaking

[Drills and sideways saws](../processing-scope.md#block-breaking) (`breaking/drill`, 93 mod 256
where it was measured and 64 since the basin families took its residue, and `breaking/saw`, 192),
measured like processing, twice, on `cover 0` over a fixed corpus at `2dd98580` on
`fuzz/breakers-deployers` with the families' final histories (the commit after it): the replay corpus and the
processing campaign's 168 scenarios (392) without them, and with every drill history of the first
index cycle (432), every saw history (384) and the seven breaking cases (1215). `./run.sh fuzz
gaps processing` on the second finds no unexplained branch. The reporter's JVM needs more address
space than the driver's 5 GB limit leaves it, so `cover` ran without the limit. Java and the port
differ on 801 of the 1215 scenarios by [the open differences](../processing-scope.md); coverage
does not depend on their agreeing.

| Class | Branches | Reached without | Reached with |
| --- | ---: | ---: | ---: |
| `BlockBreakingKineticBlockEntity` | 42 | 22 | 35 |
| `DrillBlock` | 20 | 3 | 6 |
| `DrillBlockEntity` | 38 | 0 | 4 |
| `SawBlock` | 68 | 8 | 15 |
| `SawBlockEntity` | 190 | 109 | 119 |
| `TreeCutter` | 128 | 3 | 92 |
| The breaker base with `drill/*`, `SawBlock*` and `TreeCutter*` | 546 | 145 | 271 |

Missed in those classes: harness-gap 109 groups / 202 branches, from 162 / 350; client 15 / 31;
unreachable 7 / 7 (the breaker's `-1` progress sentinel, two guards `BlockHelper.destroyBlockAs`
already keeps, a null reader, a block both log and root, a leaf with another integer property);
out-of-scope: contraptions 4 / 10, schematics and contraptions 6 / 20, Dynamic Trees 3 / 3,
`bulkCutting` config 1 / 1, Tinkers' slime logs 1 / 1. What stays `harness-gap` there is not block
breaking: the drill and saw items' placement (the harness mirrors no item use for them), entities in
a breaker, a waterlogged drill, a cobble generator, the upward saw's item work, and targets the
families don't grow (chorus, cactus, sugar cane, kelp, weeping vines, scaffolding, wider root and
leaf arrangements, a block whose loot can roll nothing).

## Why the rest is never reached

Groups / missed branches by category.

| Category | Kinetics | Belts | Fluids |
| --- | ---: | ---: | ---: |
| harness-gap: reachable only through inputs the generator lacks | 1922 / 3980 | 876 / 1599 | 525 / 979 |
| client: client-side code or guards, renderers, visuals, models, particles, tooltips | 931 / 2189 | 323 / 813 | 239 / 528 |
| out-of-scope:contraptions | 99 / 234 | 25 / 54 | 9 / 21 |
| out-of-scope:schematics-and-contraptions (structure transforms) | 43 / 96 | 29 / 64 | 15 / 36 |
| out-of-scope:schematics | 4 / 8 | 3 / 6 | |
| out-of-scope:datagen | 45 / 132 | 33 / 98 | 7 / 18 |
| out-of-scope:ponder | 2 / 2 | 2 / 4 | 4 / 7 |
| out-of-scope:config (non-default server config, other dimensions) | | | 6 / 14 |
| out-of-scope:chunk-lifecycle-adapter (unloaded chunks; areas are force-loaded) | 5 / 5 | | 8 / 13 |
| out-of-scope:computercraft, dynamictrees (other mods) | 10 / 17 | | |
| out-of-scope:environment-source-adapter, mirror-adapter, registration | 10 / 21 | | |
| error-path: defensive guards and throws | 12 / 14 | 2 / 2 | 25 / 25 |
| unreachable: dead code and branches no caller takes | 22 / 37 | | 7 / 14 |

The fluid reasons are new: 252 rules, one per method or line range, each naming the Create source
lines (`FluidNetwork.java:173 every 40 ticks`, `PumpBlockEntity.java:115 "Reversed" is never
written`). The kinetics and belts reasons were already complete for this input; no new group came
up in either.

The largest fluid harness gaps: hose pulleys (only placed as static kinetic states, so
`FluidDrainingBehaviour`, `FluidFillingBehaviour` and the hose's handler never run), drains fed by
belts, funnels and other machines, potions and other component-bearing fluids (outside the admitted
contract), boilers with whistles, branching pipe networks with several endpoints, pumps reversed
while fluid flows, brackets on pipes (the driver brackets only shafts and cogs) and pipe runs past
the 16-block pump range.

## Campaigns

| Campaign | Start corpus | Rounds | Fuzzer | Branches after |
| --- | --- | ---: | --- | ---: |
| C | `replay-corpus` (224), 37 keepers of two earlier interrupted runs and 11 extension seeds: 272 | 2500 | `7b4c440` with the uncommitted extensions, committed as `0ab3606`..`e1bf4e3` | 5182 |
| D | C's corpus, 360 | 2000, fresh seeds from 2500 | `e1bf4e3` | 5283 |
| E (this input) | D's corpus, 387, plus every generated seed 0-1999 not already in it: 2273 | 300, fresh seeds from 4500 | `e1bf4e3` | 5409 |

New edges per subsystem stopped appearing in D: over its last 1360 rounds kinetics gained 14
branches, belts 11 and fluids 5; over its last 360, 4, 0 and 0. Guided rounds take a fresh seed only
one round in five, so some family variants keyed on high seed bits were never generated in C or D
(no pump ever poured into open air, `OpenEndedPipe.java:196`). E replays every seed 0-1999 once,
which added 56 kinetics, 49 belt and 55 fluid branches; its 300 fresh rounds after that added 11
branches in 4 rounds.

## What the extensions reached

- Tank layers (`FluidTankItem.tryMultiPlace`, tank family from seed 1152): a tank placed on top of a
  formed 2- or 3-wide tank without sneaking fills the free rest of that layer the next tick; a stone
  in the layer stops it. `FluidTankItem` went from 32 of 50 branches (an earlier, interrupted run
  without the variants) to 34. Mutated tank cases (random sneaking) also multi-place, so only the 2
  branches of the layer itself are its own. Java and the port agree on every case.
- Fluid reactions (pump family from seed 1792): water and lava pumped into one junction over a tank
  break the pipe into cobblestone (`handlePipeFlowCollision`); water poured into a lava source makes
  obsidian and lava into a water source stone (`handlePipeSpillCollision`). `FluidReactions` went
  from 0 of 50 branches in that run to 15. In campaign C, these cases and their mutants alone
  reached 37 branches: 10 in `handlePipeSpillCollisionFallback`, 11 in
  `OpenEndedPipe.provideFluidToSpace`, 5 in `PipeConnection.manageFlows`, 4 in `PumpBlockEntity.hasReachedValidEndpoint`, 5 in
  `FluidNetwork` and 2 more. They first showed a driver gap: the pipe a collision breaks dropped as
  an item in Java and not in the driver, which set air where the glue calls `destroy_block(pos,
  true)`. The driver now drops it; then every case matches.
- Guided campaigns skip a case the Java side refuses (fluid reaching the slot's edge after a
  mutation) instead of aborting.

## Kinetics scope and face families (F, G)

[The kinetics scope](../kinetics-scope.md) names the Java the kinetics subsystem covers and what it
leaves to others. In `reasons.tsv`, machine work (mixers, millstones, crushers, saws, drills,
deployers, arms, crafters, turntables, the cuckoo clock's animation and the stationary breaker)
became `out-of-scope:processing`, and `Block.rotate`/`mirror` of kinetic blocks, which only
structure transforms and the symmetry wand call, `out-of-scope:schematics-and-contraptions`.
Five families (`rust/src/kinetic_faces.rs`) drive what was left of placement and the wrench.

Both campaigns ran on a server of their own with `cover 0` over a fixed corpus: F the replay
corpus and every kinetics seed 0-1023 (`network/*`, `transmission/*`, `actions`, `geometry/*`,
water wheels, steam engines: 664 seeds, 888 scenarios) on `main` (`0ff1441`); G the same 888 and
392 seeds of the new families (face placement 192, face wrench 32, driven placement 96, integrated
cogs 48, cog casing 24) on `6c8f81a`, after a restart. Beyond the replay corpus neither ran fluid or
belt seeds, so their fluid and belt figures are below campaign E's. All 888 and 1280 scenarios
matched. None of the branches G gained over F is in a static initializer, which runs once per server.

| Kinetics | Reached | Harness-gap groups / branches | Core harness-gap groups / branches |
| --- | ---: | ---: | ---: |
| F, `main`'s reasons | 2513 / 9646 | 2077 / 4349 | 520 / 918 |
| F, this scope | 2513 / 9646 | 1086 / 2083 | 504 / 884 |
| G, this scope | 2542 / 9646 | 1071 / 2056 | 490 / 858 |

The scope moved 969 groups (2212 branches) to processing and 22 (54) to structure transforms. The
families reached 29 more kinetics branches (and 7 belt ones): gauges placed on a shaft end
(`GaugeBlock.getStateForPlacement`), cranks, sequencers and gauges against kinetic neighbours, the
flywheel's shaft faces, both motor and controller value boxes' sides, the cog helpers' state
predicates, and `EncasedCogwheelBlock.handleEncasing` beside shafts. Most of what they drive the
existing families already reached; the remaining in-scope gaps are lifecycle paths (stale sources
in `KineticBlockEntity.validateKinetics`, `switchToBlockState` without a kinetic entity), pick-block,
adventure-mode and spectator players, and steam engine and water wheel branches. The new
`swapShaftsForRotation`, `ChainDriveBlock.reversePart` and two fluid reasons are line-pinned.

## Port differences

### Pressure for one tick after a pipe flow collision (fixed)

[fluid-pipe-collision-pressure.json](../../cases/fluid-pipe-collision-pressure.json), the shrunk
sweep seed 1874 (1906 the same at 128 rpm): water and lava pumped at 8 rpm into one junction over a
tank.

- Java: the collision breaks the junction inside its block entity tick (`BlockHelper.destroyBlock`,
  FluidReactions.java:32), and `FluidPipeBlock.onRemove` walks
  `FluidPropagator.propagateChangedPipe` at once (FluidPipeBlock.java:148-151): at the end of that
  tick the pipes on both sides and the pump's connections hold no pressure.
- Port: `Transport::remove` only queued the change, and the walk ran in the next level tick's
  `prepare`, so for that one tick the pump and both pipes kept 8 (`tick 132: 2,4,0: pump pressure:
  java Some(0.0) rust Some(8.0)`, six more connection pressures). Seeds 1810 and 1842, the same
  junction at 32 rpm, matched at their checkpoints, which the family takes every second tick.
- Fix (`engine: a pipe broken by a fluid collision wipes its neighbours' pressure that tick`):
  `Transport::tick_pipe` walks `propagate_changed_pipe` from the pipe as soon as its flows collide,
  while it still stands. The glue and the driver share it. `main` has no case reaching this; its pump
  family has no junction of two fluids.

### A tunnel used off a belt drops as an item

A player clicks the top of a stone block with an
andesite (or brass) tunnel.

- Java: nothing is placed and the tunnel stays in hand. `BeltTunnelItem.canPlace`
  (BeltTunnelItem.java:24-33) asks `BeltTunnelBlock.isValidPositionForPlacement`
  (BeltTunnelBlock.java:73-80): a horizontal belt must be below.
- Port: the host places the tunnel's default state; `tick_tunnels` (create/src/glue/logistics/tunnel.rs)
  finds no cased belt below and calls `destroy_block(pos, true)`, so the tunnel stands for a tick and
  drops as an item.
- The driver mirror shows `drops: java [] rust [andesite_tunnel at (0.5, 4.5, 0.5), tick 6]`, and `main` differs the
  same way. Being fixed with the remaining belt differences.

### A belt funnel with no belt below (injected state only)

Belt funnels injected by state events over air (mutated belt cases whose belt never formed) stay belt
funnels in the port; Java turns them into floor funnels at their next shape update
(`BeltFunnelBlock.updateShape`, BeltFunnelBlock.java:137-144). A player cannot place a belt funnel
off a belt, so this is not a gameplay difference.
