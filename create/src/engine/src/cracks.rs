//! Create's `LevelRendererMixin`: a crack on one part of a multiblock is drawn on the parts its
//! `MultiPosDestructionHandler.getExtraPositions` names, at the same stage, and leaves them when
//! it does (`BlockDestructionProgressExtension`). The client keeps one crack per breaker, so each
//! part shows the crack as a breaker of its own.

use crate::Block;
use crate::belt;
use crate::kinetics::Dir;
use crate::parts::StateInfo;
use crate::waterwheel;
use std::collections::HashMap;

/// A crack for the client to show: `breaker`'s on the block at `pos`, at `stage` (0 to 9; any
/// other stage takes it away).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crack {
    pub breaker: i32,
    pub pos: [i32; 3],
    pub stage: i32,
}

/// The breaker whose crack stands for `breaker`'s on the part at `pos`: negative, so it never
/// meets an entity's, and its own for each breaker and part.
pub fn part_breaker(breaker: i32, pos: [i32; 3]) -> i32 {
    let h = breaker.wrapping_mul(0x85EB_CA6Bu32 as i32)
        ^ pos[0].wrapping_mul(0xC2B2_AE35u32 as i32)
        ^ pos[1].wrapping_mul(0x27D4_EB2F)
        ^ pos[2].wrapping_mul(0x1656_67B1);
    h | i32::MIN
}

/// `BeltBlock.RenderProperties.getExtraPositions`: `BeltBlock.getBeltChain` from the belt's
/// controller, which `BeltBlock.initBelt` finds by walking back along the chain.
pub fn belt_chain(
    pos: [i32; 3],
    segment_at: &impl Fn([i32; 3]) -> Option<belt::Segment>,
) -> Vec<[i32; 3]> {
    belt::controller_of(pos, segment_at).map_or_else(Vec::new, |c| belt::chain(c, segment_at))
}

/// `WaterWheelStructuralBlock.RenderProperties.getExtraPositions`: the wheel the frame cell at
/// `pos` facing `facing` holds (`stillValid`, then `getMaster`); `None` when the cell holds none.
pub fn frame_wheel(
    pos: [i32; 3],
    facing: Dir,
    info_at: &impl Fn([i32; 3]) -> Option<(Block, StateInfo)>,
) -> Option<[i32; 3]> {
    waterwheel::frame_holds(pos, facing, false, info_at)
        .then(|| waterwheel::master(pos, facing, info_at))
        .flatten()
        .map(|(at, _, _)| at)
}

/// The parts each breaker's crack is drawn on, as Create's mixin keeps them on its crack
/// (`create$setExtraPositions`), by breaker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cracks {
    parts: HashMap<i32, Vec<[i32; 3]>>,
}

impl Cracks {
    /// `LevelRendererMixin.create$onDestroyBlockProgress` and `create$onRemoveProgress`: what
    /// the client is told so that `breaker`'s crack at `stage` shows on the block at `pos` and on
    /// `extra`, the parts `getExtraPositions` names (`None` for a block without one), and on no
    /// other. A stage outside 0 to 9 takes the crack away.
    pub fn told(
        &mut self,
        breaker: i32,
        pos: [i32; 3],
        extra: Option<Vec<[i32; 3]>>,
        stage: i32,
    ) -> Vec<Crack> {
        let mut parts = extra.unwrap_or_default();
        parts.retain(|p| *p != pos);
        parts.sort_unstable();
        parts.dedup();
        if !(0..10).contains(&stage) {
            parts.clear();
        }
        let before = self.parts.remove(&breaker).unwrap_or_default();
        if !parts.is_empty() {
            self.parts.insert(breaker, parts.clone());
        }
        before
            .iter()
            .filter(|p| parts.binary_search(p).is_err())
            .map(|p| Crack {
                breaker: part_breaker(breaker, *p),
                pos: *p,
                stage: -1,
            })
            .chain(parts.iter().map(|p| Crack {
                breaker: part_breaker(breaker, *p),
                pos: *p,
                stage,
            }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::belt::{Part, Segment, Slope};
    use crate::kinetics::Axis;
    use std::collections::HashMap;

    /// A belt chain of `len` segments from (0, 0, 0) east, as a level's belt blocks read.
    fn chain(len: i32) -> impl Fn([i32; 3]) -> Option<Segment> {
        move |p| {
            if p[1] != 0 || p[2] != 0 || !(0..len).contains(&p[0]) {
                return None;
            }
            let part = match p[0] {
                0 => Part::Start,
                n if n == len - 1 => Part::End,
                _ => Part::Middle,
            };
            Some(Segment {
                facing: Dir::East,
                slope: Slope::Horizontal,
                part,
            })
        }
    }

    fn row(from: i32, to: i32) -> Vec<[i32; 3]> {
        (from..=to).map(|x| [x, 0, 0]).collect()
    }

    fn frame(facing: Dir) -> (Block, StateInfo) {
        (
            Block::WaterWheelStructure,
            StateInfo {
                facing: Some(facing),
                ..StateInfo::default()
            },
        )
    }

    fn large(axis: Axis) -> (Block, StateInfo) {
        (
            Block::LargeWaterWheel,
            StateInfo {
                axis: Some(axis),
                ..StateInfo::default()
            },
        )
    }

    #[test]
    fn a_belt_cracks_its_whole_chain_far_beyond_eight_blocks() {
        let belts = chain(20);
        assert_eq!(belt_chain([3, 0, 0], &belts), row(0, 19));
        let mut cracks = Cracks::default();
        let shown = cracks.told(5, [0, 0, 0], Some(belt_chain([0, 0, 0], &belts)), 6);
        assert_eq!(
            shown.iter().map(|c| (c.pos, c.stage)).collect::<Vec<_>>(),
            row(1, 19).iter().map(|&p| (p, 6)).collect::<Vec<_>>()
        );
        // The stage is the broken segment's, on every segment: the one 19 blocks off too.
        assert!(shown.iter().all(|c| c.breaker < 0 && c.breaker != 5));
        let next = cracks.told(5, [0, 0, 0], Some(belt_chain([0, 0, 0], &belts)), 7);
        assert!(next.iter().all(|c| c.stage == 7));
        assert_eq!(next.len(), 19);
        let gone = cracks.told(5, [0, 0, 0], None, -1);
        assert_eq!(
            gone.iter().map(|c| (c.pos, c.stage)).collect::<Vec<_>>(),
            row(1, 19).iter().map(|&p| (p, -1)).collect::<Vec<_>>()
        );
        assert_eq!(
            gone.iter().map(|c| c.breaker).collect::<Vec<_>>(),
            shown.iter().map(|c| c.breaker).collect::<Vec<_>>()
        );
        // Nothing is left to take.
        assert!(cracks.told(5, [0, 0, 0], None, -1).is_empty());
    }

    #[test]
    fn the_middle_of_a_chain_names_itself_nowhere() {
        let mut cracks = Cracks::default();
        let shown = cracks.told(5, [9, 0, 0], Some(belt_chain([9, 0, 0], &chain(20))), 3);
        let mut positions: Vec<[i32; 3]> = shown.iter().map(|c| c.pos).collect();
        positions.sort_unstable();
        assert_eq!(positions, [row(0, 8), row(10, 19)].concat());
        assert!(shown.iter().all(|c| c.stage == 3));
    }

    #[test]
    fn a_part_that_left_the_multiblock_is_cleared() {
        let mut cracks = Cracks::default();
        cracks.told(5, [3, 0, 0], Some(belt_chain([3, 0, 0], &chain(20))), 1);
        let told = cracks.told(5, [3, 0, 0], Some(belt_chain([3, 0, 0], &chain(12))), 1);
        let cleared = told
            .iter()
            .filter(|c| c.stage < 0)
            .map(|c| c.pos)
            .collect::<Vec<_>>();
        let kept = told
            .iter()
            .filter(|c| c.stage > 0)
            .map(|c| c.pos)
            .collect::<Vec<_>>();
        assert_eq!(cleared, row(12, 19));
        assert_eq!(kept, [row(0, 2), row(4, 11)].concat());
        assert!(cracks.told(5, [3, 0, 0], None, -1).len() == 11);
    }

    #[test]
    fn a_block_without_a_handler_shows_nothing() {
        assert!(belt_chain([50, 0, 0], &chain(20)).is_empty());
        let mut cracks = Cracks::default();
        assert!(cracks.told(5, [50, 0, 0], None, 4).is_empty());
        assert!(cracks.told(5, [50, 0, 0], Some(Vec::new()), 4).is_empty());
        // A handler's own position never becomes a part.
        assert!(cracks.told(5, [1, 0, 0], Some(row(1, 1)), 4).is_empty());
    }

    #[test]
    fn a_crack_cleared_once_its_block_is_gone_takes_its_parts() {
        let mut cracks = Cracks::default();
        let shown = cracks.told(5, [3, 0, 0], Some(belt_chain([3, 0, 0], &chain(20))), 5);
        // The belt broke: the level no longer names a chain for it.
        let gone = cracks.told(5, [3, 0, 0], None, -1);
        assert_eq!(gone.len(), shown.len());
        assert!(gone.iter().all(|c| c.stage < 0));
        assert_eq!(
            gone.iter().map(|c| c.breaker).collect::<Vec<_>>(),
            shown.iter().map(|c| c.breaker).collect::<Vec<_>>()
        );
    }

    #[test]
    fn two_breakers_on_one_chain_show_apart() {
        let belts = chain(10);
        let mut cracks = Cracks::default();
        let a = cracks.told(5, [3, 0, 0], Some(belt_chain([3, 0, 0], &belts)), 2);
        let b = cracks.told(9, [7, 0, 0], Some(belt_chain([7, 0, 0], &belts)), 8);
        let of = |list: &[Crack], pos: [i32; 3]| {
            let c = list.iter().find(|c| c.pos == pos).unwrap();
            (c.breaker, c.stage)
        };
        assert_eq!(of(&a, [0, 0, 0]), (part_breaker(5, [0, 0, 0]), 2));
        assert_ne!(of(&a, [0, 0, 0]).0, of(&b, [0, 0, 0]).0);
        assert_eq!(of(&b, [0, 0, 0]).1, 8);
        // One crack going leaves the other's parts alone.
        let gone = cracks.told(5, [3, 0, 0], None, -1);
        assert!(
            gone.iter()
                .all(|c| c.breaker < 0 && c.breaker != part_breaker(9, c.pos))
        );
    }

    #[test]
    fn a_part_shows_with_a_breaker_of_its_own() {
        assert!(part_breaker(5, [0, 0, 0]) < 0);
        assert_ne!(part_breaker(5, [0, 0, 0]), part_breaker(6, [0, 0, 0]));
        assert_ne!(part_breaker(5, [0, 0, 0]), part_breaker(5, [1, 0, 0]));
        // No machine's: a deployer's or a drill's id at the same block differs.
        assert_ne!(
            part_breaker(5, [7, 64, -3]),
            crate::deployer::breaker_id([7, 64, -3])
        );
    }

    #[test]
    fn a_frame_cell_cracks_the_wheel_it_holds() {
        // The wheel turns on Z at (1, 1, 0); a side cell beside it and a corner cell hang on it.
        let cells: HashMap<[i32; 3], (Block, StateInfo)> = [
            ([0, 0, 0], frame(Dir::East)),
            ([1, 0, 0], frame(Dir::Up)),
            ([1, 1, 0], large(Axis::Z)),
        ]
        .into_iter()
        .collect();
        let at = |p: [i32; 3]| cells.get(&p).copied();
        assert_eq!(frame_wheel([1, 0, 0], Dir::Up, &at), Some([1, 1, 0]));
        assert_eq!(frame_wheel([0, 0, 0], Dir::East, &at), Some([1, 1, 0]));
        let mut cracks = Cracks::default();
        let shown = cracks.told(
            5,
            [0, 0, 0],
            frame_wheel([0, 0, 0], Dir::East, &at).map(|w| vec![w]),
            4,
        );
        assert_eq!(
            shown,
            [Crack {
                breaker: part_breaker(5, [1, 1, 0]),
                pos: [1, 1, 0],
                stage: 4
            }]
        );
        // A wheel turning along the cell's facing is not held, nor is air or a third cell.
        let wrong: HashMap<_, _> = [([0, 0, 0], frame(Dir::East)), ([1, 0, 0], large(Axis::X))]
            .into_iter()
            .collect();
        assert_eq!(
            frame_wheel([0, 0, 0], Dir::East, &|p| wrong.get(&p).copied()),
            None
        );
        let too_far: HashMap<_, _> = [
            ([0, 0, 0], frame(Dir::East)),
            ([1, 0, 0], frame(Dir::East)),
            ([2, 0, 0], frame(Dir::East)),
            ([3, 0, 0], large(Axis::Z)),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            frame_wheel([0, 0, 0], Dir::East, &|p| too_far.get(&p).copied()),
            None
        );
        assert_eq!(frame_wheel([5, 0, 0], Dir::East, &at), None);
    }
}
