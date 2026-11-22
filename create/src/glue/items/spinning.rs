//! Wrench, potato cannon and worldshaper item models. Their original jar parts draw under the
//! shell's display transform; only the moving part takes Create's rotation about its pivot.

use crate::glue::pipelines::Pose;
use create_engine::equipment::ToolSpin;
use mod_api::{
    ClientHost, Identifier, SpecialModelBakeContext, SpecialModelCollector, SpecialModelContext,
    SpecialModelFactory, SpecialModelGeometry, SpecialModelRenderer, SpecialModelRenderers,
    SpecialModelStack, SpecialModelTints,
};

#[derive(Clone, Copy)]
struct Spinner {
    spin: ToolSpin,
    fixed: &'static [&'static str],
    moving: &'static str,
}

struct Spinning {
    spin: ToolSpin,
    fixed: Vec<SpecialModelGeometry>,
    moving: SpecialModelGeometry,
}

pub(super) fn register(renderers: &mut dyn SpecialModelRenderers) {
    for (id, renderer) in [
        (
            "create:wrench",
            Spinner {
                spin: ToolSpin::Wrench,
                fixed: &["create:item/wrench/item"],
                moving: "create:item/wrench/gear",
            },
        ),
        (
            "create:potato_cannon",
            Spinner {
                spin: ToolSpin::PotatoCannon,
                fixed: &["create:item/potato_cannon/item"],
                moving: "create:item/potato_cannon/cog",
            },
        ),
        (
            "create:handheld_worldshaper",
            Spinner {
                spin: ToolSpin::Worldshaper,
                fixed: &[
                    "create:item/handheld_worldshaper/item",
                    "create:item/handheld_worldshaper/core",
                    "create:item/handheld_worldshaper/core_glow",
                ],
                moving: "create:item/handheld_worldshaper/accelerator",
            },
        ),
    ] {
        let id = Identifier::parse(id).expect("valid identifier");
        let factory = SpecialModelFactory::new(
            |_| Ok(()),
            move |(), bake: &dyn SpecialModelBakeContext| {
                Ok(Spinning {
                    spin: renderer.spin,
                    fixed: renderer
                        .fixed
                        .iter()
                        .map(|m| bake.bake_model(m))
                        .collect::<Result<_, _>>()?,
                    moving: bake.bake_model(renderer.moving)?,
                })
            },
        );
        renderers.register(id, factory);
    }
}

impl SpecialModelRenderer for Spinning {
    type Argument = f64;

    fn extract_argument(&self, host: &dyn ClientHost, _stack: &SpecialModelStack<'_>) -> f64 {
        host.millis() as f64 / 1000.0
    }

    fn submit(
        &self,
        seconds: &f64,
        _context: &SpecialModelContext,
        collector: &mut SpecialModelCollector<'_>,
    ) -> Result<(), String> {
        for part in &self.fixed {
            collector.submit_geometry(part, SpecialModelTints::Untinted)?;
        }
        let (degrees, axis, pivot) = self.spin.pose(*seconds);
        let pose = Pose::at([0.0; 3]).rotate(axis, degrees, pivot);
        collector.with_pose(pose.affine(), |c| {
            c.submit_geometry(&self.moving, SpecialModelTints::Untinted)
        })
    }
}
