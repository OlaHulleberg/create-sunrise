//! The placement helpers' ghost (catnip's `IPlacementHelper.displayGhost` and
//! `GhostBlockRenderer.transparent()`): the block a helper would place, drawn at its target
//! from its block model, white, translucent at `getBreathingAlpha() * 0.75` of the indicator's
//! fade-in, full bright, shrunk to 0.85 about its centre. A copycat's ghost is the copycat base
//! cut to its shape, as its block entity draws an empty one, through a world pipeline sampling
//! the base's sprite.

use super::super::states;
use create_engine::copycat::{self as cc, Shape};
use mod_api::{
    ClientHost, Direction, Identifier, LightCoords, ModelInstance, ModelRef, Overlay,
    PipelineBlend, PipelineQuad, PipelineTexture, RegisterRenderPipelinesEvent, RenderPipeline,
    WorldDepth,
};
use std::cell::RefCell;
use std::collections::HashMap;

/// The copycat base's sprite.
const COPYCAT_BASE: &str = "create:block/copycat_base";

const COPYCAT_PIPELINE: &str = "create:ghost_copycat";

/// A quad's corners in a block's units and their UVs.
type CopycatQuad = ([[f32; 3]; 4], [[f32; 2]; 4]);

/// `translate(.5)`, `scale(.85)`, `translate(-.5)`.
const SCALE: f32 = 0.85;

thread_local! {
    /// Each ghosted state's model, resolved again once a reload leaves it stale.
    static MODELS: RefCell<HashMap<String, ModelRef>> = RefCell::new(HashMap::new());
}

/// `register_render_pipelines`: a translucent pipeline sampling the copycat base.
pub(crate) fn register(event: &mut RegisterRenderPipelinesEvent<'_>) {
    let (Ok(shader), Ok(vertex), Ok(id), Ok(sprite)) = (
        Identifier::parse("create:core/textured"),
        Identifier::parse("minecraft:core/position_tex_color"),
        Identifier::parse(COPYCAT_PIPELINE),
        Identifier::parse(COPYCAT_BASE),
    ) else {
        return;
    };
    event.register(
        RenderPipeline::new(id, shader, PipelineBlend::Translucent, WorldDepth::Tested)
            .vertex_shader(vertex)
            .textured(PipelineTexture::Sprite(sprite))
            .optional(),
    );
}

/// The copycat base's quads for a copycat state, in a block's units with their UVs.
fn copycat_quads(host: &dyn ClientHost, shape: Shape) -> Vec<CopycatQuad> {
    let mut out = Vec::new();
    let Some(sprites) = host
        .block_state_id(cc::BASE)
        .and_then(|b| host.block_sprites(&[b]).pop().flatten())
    else {
        return out;
    };
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
                out.push((corners, uv));
            }
        }
    }
    out
}

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
    let color = (((alpha.clamp(0.0, 1.0) * 255.0) as u32) << 24) | 0x00FF_FFFF;
    let (id, props) = states::split(state);
    if let Some(shape) = copycat_shape(id, &props) {
        let at = |p: [f32; 3]| {
            [0, 1, 2].map(|i| f64::from(pos[i]) + 0.5 + (f64::from(p[i]) - 0.5) * f64::from(SCALE))
        };
        let quads: Vec<PipelineQuad> = copycat_quads(host, shape)
            .into_iter()
            .map(|(corners, uv)| PipelineQuad::new(corners.map(at), uv, color))
            .collect();
        if let Ok(id) = Identifier::parse(COPYCAT_PIPELINE)
            && let Ok(p) = host.render_assets().render_pipeline(&id)
        {
            let _ = event.draw.pipeline_quads(p, &quads);
        }
        return;
    }
    let shift = f64::from(0.5 * (1.0 - SCALE));
    let instance = ModelInstance::new(
        pos.map(|v| f64::from(v) + shift),
        [
            SCALE, 0.0, 0.0, 0.0, 0.0, SCALE, 0.0, 0.0, 0.0, 0.0, SCALE, 0.0,
        ],
        0xFFFF_FFFF,
        LightCoords::FULL_BRIGHT,
        Overlay::NONE,
    )
    .with_color(color);
    for fresh in [false, true] {
        let Some(model) = model(host, state, fresh) else {
            return;
        };
        if event.draw.draw_model_instances(model, &[instance]).is_ok() {
            return;
        }
    }
}

/// The model of block state `state` (`create:cogwheel[axis=x]`), resolved again when `fresh`.
fn model(host: &dyn ClientHost, state: &str, fresh: bool) -> Option<ModelRef> {
    if !fresh && let Some(m) = MODELS.with_borrow(|m| m.get(state).copied()) {
        return Some(m);
    }
    let resolved = host.block_state_id(state)?;
    let m = host.render_assets().block_state_model(resolved).ok()?;
    MODELS.with_borrow_mut(|models| models.insert(state.to_owned(), m));
    Some(m)
}
