//! A crack on one part of a multiblock, drawn on its other parts too (Create's
//! `LevelRendererMixin`): what `Hooks::client_break_progress` is told is kept, and shown before
//! the next frame with `LevelRef::set_client_break_progress`, which the hook's read-only host
//! cannot call.

use super::{SUBSYSTEMS, client_table, states};
use create_engine::Block;
use create_engine::cracks::Cracks;
use create_engine::parts::StateInfo;
use mod_api::{ClientBreakProgressEvent, ClientHost, LevelRef};
use std::cell::RefCell;

/// A crack the hook was told of.
#[derive(Clone, Copy)]
struct Told {
    level: Option<LevelRef>,
    breaker: i32,
    pos: [i32; 3],
    stage: i32,
}

thread_local! {
    /// The parts each breaker's crack is drawn on (`create_engine::cracks::Cracks`).
    static SHOWN: RefCell<Cracks> = RefCell::new(Cracks::default());
    /// What the hook was told and no frame has shown yet.
    static TOLD: RefCell<Vec<Told>> = const { RefCell::new(Vec::new()) };
}

/// `Hooks::client_break_progress`: kept for [`show`].
pub fn told(host: &dyn ClientHost, event: &ClientBreakProgressEvent<'_>) {
    TOLD.with_borrow_mut(|told| {
        told.push(Told {
            level: host.level(event.dimension),
            breaker: event.breaker,
            pos: [event.pos.x, event.pos.y, event.pos.z],
            stage: event.progress,
        });
    });
}

/// Shows what [`told`] kept, each as Create's mixin has it: on the parts `getExtraPositions`
/// names for its block, at its stage, and off the parts it left.
pub fn show(host: &mut dyn ClientHost) {
    if TOLD.with_borrow(Vec::is_empty) {
        return;
    }
    let mut told = TOLD.with_borrow_mut(std::mem::take);
    for t in told.drain(..) {
        let Some(level) = t.level else {
            continue;
        };
        let parts = parts(host, level, t.pos);
        let cracks = SHOWN.with_borrow_mut(|s| s.told(t.breaker, t.pos, parts, t.stage));
        for c in cracks {
            level.set_client_break_progress(&mut *host, c.breaker, c.pos, c.stage);
        }
    }
    TOLD.with_borrow_mut(|waiting| *waiting = told);
}

/// `MultiPosDestructionHandler.getExtraPositions` of the Create block at `pos`: the first
/// subsystem's that owns the block.
fn parts(host: &dyn ClientHost, level: LevelRef, pos: [i32; 3]) -> Option<Vec<[i32; 3]>> {
    let (block, info) = block_at(host, level, pos)?;
    SUBSYSTEMS
        .iter()
        .find_map(|s| s.crack_parts(host, level, block, pos, &info))
}

/// The Create block at `pos` of a client's level `level`, as this client's table knows it.
pub(crate) fn block_at(
    host: &dyn ClientHost,
    level: LevelRef,
    pos: [i32; 3],
) -> Option<(Block, StateInfo)> {
    let state = level.block_state(host, pos)?;
    client_table(host, |t| t.get(states::id(state)?).map(|(b, i)| (b, *i))).flatten()
}

/// The client left its world: no crack is shown, and none is waiting.
pub fn forget() {
    SHOWN.with_borrow_mut(std::mem::take);
    TOLD.with_borrow_mut(Vec::clear);
}
