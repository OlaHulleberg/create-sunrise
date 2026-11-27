//! Which of Create's blocks have the redstone power they receive read through them to their
//! neighbours (`shouldCheckWeakPower`). A block answers whether it is a redstone conductor
//! (`BlockBehaviour`); Create's overrides all answer no, so a lever on one of these powers
//! nothing beside it.

use crate::Block;

/// Whether Create overrides `shouldCheckWeakPower` to false for `block`: `false` here leaves the
/// block's own answer.
pub fn refuses(block: Block) -> bool {
    matches!(
        block,
        // `AbstractEncasedShaftBlock`: `GearshiftBlock` (`ClutchBlock`), `EncasedShaftBlock`
        Block::Gearshift
            | Block::Clutch
            | Block::AndesiteEncasedShaft
            | Block::BrassEncasedShaft
            // `ChainDriveBlock`: `ChainGearshiftBlock`
            | Block::EncasedChainDrive
            | Block::AdjustableChainGearshift
            | Block::SequencedGearshift
            | Block::RoseQuartzLamp
            | Block::RedstoneContact
            | Block::ElevatorContact
            | Block::Sticker
            | Block::TrackSignal
            | Block::Packager
            | Block::Repackager
            | Block::RedstoneRequester
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal is each class's: `AbstractEncasedShaftBlock`, `ChainDriveBlock` and their
    /// subclasses, and the blocks that override it alone.
    #[test]
    fn the_refusal_is_the_class_s() {
        for class in [
            // `AbstractEncasedShaftBlock`
            Block::Gearshift,
            Block::Clutch,
            Block::AndesiteEncasedShaft,
            Block::BrassEncasedShaft,
            // `ChainDriveBlock`
            Block::EncasedChainDrive,
            Block::AdjustableChainGearshift,
        ] {
            assert!(refuses(class), "{class:?}");
        }
        for alone in [
            Block::SequencedGearshift,
            Block::RoseQuartzLamp,
            Block::RedstoneContact,
            Block::ElevatorContact,
            Block::Sticker,
            Block::TrackSignal,
            Block::Packager,
            Block::Repackager,
            Block::RedstoneRequester,
        ] {
            assert!(refuses(alone), "{alone:?}");
        }
        // `GirderEncasedShaftBlock` is no `EncasedShaftBlock`; a shaft, a gearbox and a cog keep
        // the default.
        for keeps in [
            Block::MetalGirderEncasedShaft,
            Block::Shaft,
            Block::Gearbox,
            Block::Cogwheel,
        ] {
            assert!(!refuses(keeps), "{keeps:?}");
        }
    }
}
