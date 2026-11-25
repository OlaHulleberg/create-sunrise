//! The placement helpers' ghost (catnip's `IPlacementHelper.displayGhost` and
//! `GhostBlockRenderer.transparent()`): the block a helper would place, drawn at its target
//! from its block model, white, translucent at `getBreathingAlpha() * 0.75` of the indicator's
//! fade-in, full bright, shrunk to 0.85 about its centre.
//!
//! The block's model is the game's own (`RenderAssets::block_state_model`), drawn as one
//! model instance (`WorldDrawContext::draw_model_instances`) whose colour's alpha makes it
//! translucent (`ModelInstance::with_color`). A copycat's ghost is the copycat base cut to its
//! shape, as its block entity draws an empty one: its quads go through the glue's
//! `Textured::CopycatBase` pipeline.

use super::super::pipelines::{self, Textured};
use super::super::states;
use create_engine::copycat::{self as cc, Shape};
use mod_api::{
    ClientHost, Direction, HostBlockStates, LightCoords, ModelInstance, Overlay, PipelineQuad,
};

/// A copycat's shape from its state's properties (`CopycatPanelModel`, `CopycatStepModel`).
fn copycat_shape(id: &str, props: &[(&str, &str)]) -> Option<Shape> {
    let get = |k: &str| props.iter().find(|(p, _)| *p == k).map(|(_, v)| *v);
    match id {
        "create:copycat_panel" => Some(Shape::Panel(cc::dir_named(get("facing")?)?)),
        "create:copycat_step" => Some(Shape::Step {
            facing: cc::dir_named(get("facing")?)?,
            top: get("half") == Some("top"),
        }),
        _ => None,
    }
}

/// `GhostBlocks.getBreathingAlpha`: 0.35 to 0.75 and back every 2.5 seconds of the clock.
fn breathing_alpha() -> f32 {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let timer = (ms % 2500) as f64;
    (0.55 - 0.2 * ((2.0 / 2500.0) * std::f64::consts::PI * timer).cos()) as f32
}

/// `TransparentGhostBlockRenderer.render`: `state` at `pos`, at `progress` of the indicator's
/// fade-in (`PlacementClient.getCurrentAlpha`).
pub(super) fn draw(
    host: &dyn ClientHost,
    event: &mut mod_api::SubmitCustomGeometryEvent<'_>,
    state: &str,
    pos: [i32; 3],
    progress: f32,
) {
    let alpha = breathing_alpha() * 0.75 * progress;
    // `ColoringVertexConsumer(1, 1, 1, alpha)` over the model's vertex colours, which are white:
    // the virtual level's shade is 1 and its light full.
    let color = (((alpha * 255.0).round().clamp(0.0, 255.0) as u32) << 24) | 0x00FF_FFFF;
    let (id, props) = states::split(state);
    if let Some(shape) = copycat_shape(id, &props) {
        draw_copycat(host, event, shape, pos, color);
        return;
    }
    let Some(mut block) = host.block_state_id(id) else {
        return;
    };
    let facts = host.block_states();
    for (k, v) in &props {
        if let Ok(next) = block.with(&facts, k, v) {
            block = next;
        }
    }
    let Ok(model) = host.render_assets().block_state_model(block) else {
        return;
    };
    // `translate(.5)`, `scale(.85)`, `translate(-.5)` in the block at `pos`.
    let shrink = 0.85_f32;
    let shift = (1.0 - shrink) * 0.5;
    #[rustfmt::skip]
    let transform = [
        shrink, 0.0, 0.0, shift,
        0.0, shrink, 0.0, shift,
        0.0, 0.0, shrink, shift,
    ];
    let instance = ModelInstance::new(
        pos.map(f64::from),
        transform,
        0xFFFF_FFFF,
        LightCoords::FULL_BRIGHT,
        Overlay::NONE,
    )
    .with_color(color);
    let _ = event.draw.draw_model_instances(model, &[instance]);
}

/// A copycat's ghost: its base's faces cut to `shape`, shrunk as the model ghosts are.
fn draw_copycat(
    host: &dyn ClientHost,
    event: &mut mod_api::SubmitCustomGeometryEvent<'_>,
    shape: Shape,
    pos: [i32; 3],
    color: u32,
) {
    let Some(sprites) = host
        .block_state_id(cc::BASE)
        .and_then(|b| host.block_sprites(&[b]).pop().flatten())
    else {
        return;
    };
    let Some(pipeline) = pipelines::resolve(host, Textured::CopycatBase) else {
        return;
    };
    let at = |p: [f32; 3]| {
        // `translate(.5)`, `scale(.85)`, `translate(-.5)` in the block at `pos`.
        [0, 1, 2].map(|i| f64::from(pos[i]) + 0.5 + (f64::from(p[i]) - 0.5) * 0.85)
    };
    let mut quads = Vec::new();
    for [from, to] in shape.boxes() {
        for side in Direction::ALL {
            for face in &sprites.sides[side as usize] {
                if face.inset != 0.0 {
                    continue;
                }
                let Some((corners, [u0, v0, u1, v1])) = face.quad_on(side, from, to) else {
                    continue;
                };
                let uv = [[u0, v0], [u0, v1], [u1, v1], [u1, v0]].map(|c| c.map(|t| t / 16.0));
                quads.push(PipelineQuad::new(corners.map(at), uv, color));
            }
        }
    }
    let _ = event.draw.pipeline_quads(pipeline, &quads);
}
