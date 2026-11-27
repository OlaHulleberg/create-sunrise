//! Two punching deployers facing each other bump fists (`DeployerBlockEntity.startFistBump`,
//! `triggerFistBump`): one at rest finds the other two to four blocks ahead, and both reach
//! out to meet halfway; the first to reach the end of its stroke, its partner there too, pulls
//! both back with a punch's sound, and earns its placer `FIST_BUMP`. A bump is not saved, as
//! Create's `fistBump` is not.

use super::super::server::ServerLevel;
use super::super::world::with_world;
use create_engine::Block;
use create_engine::deployer::{Deployer, Mode, State};
use create_engine::kinetics::Dir;

/// The first deployer two to four blocks ahead of the one at `pos` facing `facing`, and how
/// far; None when one of those blocks is not loaded or none is there.
fn partner(lv: &ServerLevel<'_>, pos: [i32; 3], facing: Dir) -> Option<([i32; 3], i32)> {
    let dim = lv.dim;
    for i in 2..5 {
        let p = super::step(pos, facing, i);
        if !lv.is_loaded(p) {
            return None;
        }
        if with_world(|w| w.dims[dim].deployers.contains_key(&p)) == Some(true) {
            return Some((p, i));
        }
    }
    None
}

/// `startFistBump` for the punching deployer `d` at `pos` at rest: true when it and the one
/// facing it set off toward each other.
pub(super) fn start(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    d: &mut Deployer,
) -> bool {
    let dim = lv.dim;
    let Some((other, i)) = partner(lv, pos, facing) else {
        return false;
    };
    let faces_back = lv.facing_of(other, Block::Deployer) == Some(facing.opposite());
    let punching = with_world(|w| w.dims[dim].deployers.get(&other).map(|d| d.mode)).flatten()
        == Some(Mode::Punch);
    if !faces_back || !punching || lv.speed(other) == 0.0 {
        return false;
    }
    let reach = (i - 2) as f32 * 0.5;
    bump(d, reach);
    with_world(|w| {
        let dd = &mut w.dims[dim];
        if let Some(dp) = dd.deployers.get_mut(&other) {
            bump(dp, reach);
            dd.deployer_changes.push(other);
        }
    });
    true
}

fn bump(d: &mut Deployer, reach: f32) {
    d.fist_bump = true;
    d.reach = reach;
    d.timer = 1000;
    d.state = State::Expanding;
}

/// `triggerFistBump` for the deployer `d` at `pos` at the end of its stroke: its partner there
/// too is drawn back, the bump sounds between them and its placer earns `FIST_BUMP`.
pub(super) fn trigger(lv: &mut ServerLevel<'_>, pos: [i32; 3], facing: Dir, d: &mut Deployer) {
    let dim = lv.dim;
    let Some((other, _)) = partner(lv, pos, facing) else {
        return;
    };
    let ready = with_world(|w| {
        w.dims[dim]
            .deployers
            .get(&other)
            .is_some_and(|o| o.fist_bump && o.state == State::Expanding && o.timer <= 0)
    }) == Some(true);
    if !ready {
        return;
    }
    d.fist_bump = false;
    with_world(|w| {
        let dd = &mut w.dims[dim];
        if let Some(dp) = dd.deployers.get_mut(&other) {
            dp.fist_bump = false;
            dp.state = State::Retracting;
            dp.timer = 1000;
            dd.deployer_changes.push(other);
        }
    });
    crate::glue::advancements::award_placer(lv, pos, "fist_bump");
    // Between the two, `PLAYER_ATTACK_NODAMAGE` at .75, .75.
    let at = std::array::from_fn(|i| (((pos[i] + other[i]) as f64 + 1.0) * 0.5).floor() + 0.5);
    let _ = lv.level.play_sound(
        &mut *lv.host,
        "minecraft:entity.player.attack.nodamage",
        at,
        mod_api::SoundSource::Blocks,
        0.75,
        0.75,
        None,
    );
}
