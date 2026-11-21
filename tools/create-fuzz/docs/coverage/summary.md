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
  in the layer stops it. `FluidTankItem` went from 32 of 50 branches (an earlier, interrupted run without the variants) to 34. Mutated tank cases
  (random sneaking) also multi-place, so only the 2 branches of the layer itself are its own. Java
  and the port agree on every case.
- Fluid reactions (pump family from seed 1792): water and lava pumped into one junction over a tank
  break the pipe into cobblestone (`handlePipeFlowCollision`); water poured into a lava source makes
  obsidian and lava into a water source stone (`handlePipeSpillCollision`). `FluidReactions` went
  from 0 of 50 branches in that run to 15. In campaign C, these cases and their mutants alone reached 37
  branches: 10 in `handlePipeSpillCollisionFallback`, 11 in `OpenEndedPipe.provideFluidToSpace`,
  5 in `PipeConnection.manageFlows`, 4 in `PumpBlockEntity.hasReachedValidEndpoint`, 5 in
  `FluidNetwork` and 2 more. They first showed a driver gap: the pipe a collision breaks dropped as
  an item in Java and not in the driver, which set air where the glue calls `destroy_block(pos,
  true)`. The driver now drops it; then every case matches.
- Guided campaigns skip a case the Java side refuses (fluid reaching the slot's edge after a
  mutation) instead of aborting.

## Port differences

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
