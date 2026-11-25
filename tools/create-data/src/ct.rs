//! Create's connected textures (`ConnectedTextureBehaviour`, `AllCTTypes`) as Sunrise's
//! connected-texture rules (`assets/create/sunrise/connected/`), which the game's terrain mesher
//! draws from Create's own `_connected` sheets in the player's jar, by their sprite ids. A rule per
//! block and sheet matches the block and the texture Create shifts, on the faces Create shifts;
//! its table gives, for each outcome of the neighbour tests, the tile Create's index function
//! (`AllCTTypes.getTextureIndex`) takes from the sheet for the context those neighbours stand
//! for. Block models stay Create's: whatever draws a block outside the terrain (a copycat, a
//! moving structure, a block display) shows its plain texture.
//!
//! Faces are oriented as Create's `ConnectedTextureBehaviour.buildContext` (a side's up is up,
//! its right as its texture reads from outside; the top's up is north, the bottom's south), which
//! are a cube's default UVs. Where Create turns a face's directions with the block (pillars,
//! chassis, vaults, encased cogs' sides), so that its sheet reads as its turned texture does, the
//! rule reads the quad's own texture directions (`orient: texture`). Create's `isBeingBlocked` (a
//! neighbour covered by the same block in front doesn't join) is a table's `cover`. Create's joins
//! that ask more than the block are approximated by the same block state (`connect: state`:
//! encased blocks, trapdoors). Vaults and fluid tanks join only their own multiblock
//! (`connect_group`), by the connection group the glue publishes for each block's controller.
//!
//! Left out: copper roofs (`ROOF`: joined by collision heights, stairs mapped by shape), the
//! crafters' and brass tunnels' sides (joined by their block entities), a pillar's
//! `ConnectedPillarBlock` sides (the port keeps them unset: same pillars join), the
//! glass panes' mirrored left and right on their north and west faces
//! (`GlassPaneCTBehaviour.reverseUVsHorizontally`), casings joining the encased blocks of their
//! casing (`CasingConnectivity`: a casing joins only its own block, an encased block its own
//! state), and the weathered iron window's sheet picked at random per face
//! (`WeatheredIronWindowCTBehaviour`), which no rule can pick (`API_GAPS.md`): it keeps its plain
//! texture.

use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// Create's CT types the port's blocks use, with their sheet sizes (`AllCTTypes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtType {
    HorizontalKryppers,
    /// `HORIZONTAL`.
    Horizontal,
    /// A `VERTICAL` or `HORIZONTAL` sheet whose slit runs along its texture's width, read in
    /// the texture's own directions: its second tile is joined on the texture's left, its third
    /// on its right (the encased cogs' sides, which Create reads in turned directions).
    HorizontalByTexture,
    Vertical,
    Omni,
    Rectangle,
}

/// `CTContext`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ctx {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub top_left: bool,
    pub top_right: bool,
    pub bottom_left: bool,
    pub bottom_right: bool,
}

impl CtType {
    pub fn sheet_size(self) -> u32 {
        match self {
            Self::HorizontalKryppers
            | Self::Horizontal
            | Self::HorizontalByTexture
            | Self::Vertical => 2,
            Self::Omni => 8,
            Self::Rectangle => 4,
        }
    }

    /// The name of the type's shared table.
    fn name(self) -> &'static str {
        match self {
            Self::HorizontalKryppers => "horizontal_kryppers",
            Self::Horizontal => "horizontal",
            Self::HorizontalByTexture => "horizontal_by_texture",
            Self::Vertical => "vertical",
            Self::Omni => "omni",
            Self::Rectangle => "rectangle",
        }
    }

    /// `getTextureIndex`.
    pub fn index(self, c: Ctx) -> u32 {
        let b = u32::from;
        match self {
            Self::HorizontalKryppers => match (c.left, c.right) {
                (false, false) => 0,
                (_, false) => 3,
                (false, _) => 2,
                _ => 1,
            },
            Self::Horizontal => b(c.right) + 2 * b(c.left),
            Self::HorizontalByTexture => b(c.left) + 2 * b(c.right),
            Self::Vertical => b(c.up) + 2 * b(c.down),
            Self::Rectangle => {
                let x = match (c.left, c.right) {
                    (true, true) => 2,
                    (true, false) => 3,
                    (false, true) => 1,
                    _ => 0,
                };
                let y = match (c.up, c.down) {
                    (true, true) => 1,
                    (true, false) => 2,
                    (false, true) => 0,
                    _ => 3,
                };
                x + y * 4
            }
            Self::Omni => omni(c),
        }
    }
}

/// `OMNIDIRECTIONAL.getTextureIndex`.
fn omni(c: Ctx) -> u32 {
    let (mut x, mut y): (i32, i32) = (0, 0);
    let borders = [c.up, c.down, c.left, c.right]
        .iter()
        .filter(|j| !**j)
        .count();
    if c.up {
        x += 1;
    }
    if c.down {
        x += 2;
    }
    if c.left {
        y += 1;
    }
    if c.right {
        y += 2;
    }
    if borders == 0 {
        if c.top_right {
            x += 1;
        }
        if c.top_left {
            x += 2;
        }
        if c.bottom_right {
            y += 2;
        }
        if c.bottom_left {
            y += 1;
        }
    }
    let i = i32::from;
    if borders == 1 {
        if !c.right && (c.top_left || c.bottom_left) {
            y = 4;
            x = -1 + i(c.bottom_left) + i(c.top_left) * 2;
        }
        if !c.left && (c.top_right || c.bottom_right) {
            y = 5;
            x = -1 + i(c.bottom_right) + i(c.top_right) * 2;
        }
        if !c.down && (c.top_left || c.top_right) {
            y = 6;
            x = -1 + i(c.top_left) + i(c.top_right) * 2;
        }
        if !c.up && (c.bottom_left || c.bottom_right) {
            y = 7;
            x = -1 + i(c.bottom_left) + i(c.bottom_right) * 2;
        }
    }
    if borders == 2
        && ((c.up && c.left && c.top_left)
            || (c.down && c.left && c.bottom_left)
            || (c.up && c.right && c.top_right)
            || (c.down && c.right && c.bottom_right))
    {
        x += 3;
    }
    (x + 8 * y) as u32
}

// Neighbour bits, in a face's directions.
const U: u8 = 1;
const UR: u8 = 2;
const R: u8 = 4;
const DR: u8 = 8;
const D: u8 = 16;
const DL: u8 = 32;
const L: u8 = 64;
const UL: u8 = 128;

/// The context a mask stands for: a corner joins only with both sides beside it, as Create's
/// `buildContext` tests it.
fn context(mask: u8) -> Ctx {
    let has = |b: u8| mask & b != 0;
    let (up, down, left, right) = (has(U), has(D), has(L), has(R));
    Ctx {
        up,
        down,
        left,
        right,
        top_left: up && left && has(UL),
        top_right: up && right && has(UR),
        bottom_left: down && left && has(DL),
        bottom_right: down && right && has(DR),
    }
}

/// Which faces a rule changes (`HorizontalCTBehaviour`: its shift on the sides, its
/// top shift on the top and bottom), by their direction in the world.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Faces {
    All,
    Sides,
    Ends,
    /// These faces, by their names in a rule's `faces`.
    Some(Vec<&'static str>),
}

/// How a block joins (`connectsTo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Join {
    /// The same block.
    Block,
    /// Any compatible casing appearance. The rule only joins the same block.
    Casing,
    /// The same block state: the same block in the same form (`state == other`, or the axis,
    /// shafts and sides Create compares).
    State,
    /// The same block (`state`: the same state) of the same multiblock
    /// (`ConnectivityHandler.isConnected`), by the connection group the glue publishes.
    Group { state: bool },
}

/// `block`'s faces showing `original` take `sheet` (Create's `_connected` sheet
/// path, under `block/`).
struct Rule {
    original: String,
    sheet: String,
    ty: CtType,
    faces: Faces,
    /// Create's `isBeingBlocked`: a neighbour whose face the same block covers doesn't join.
    seams: bool,
    /// The face's directions are its texture's (`orient: texture`), where Create turns them with
    /// the block (`getUpDirection`, `getRightDirection` and the reversed UVs of pillars,
    /// chassis, vaults and encased cogs), not a cube's.
    by_texture: bool,
    /// Create picks one of these sheets per face at random (`WeatheredIronWindowCTBehaviour`),
    /// which no rule can: such a rule is left out.
    random: Vec<String>,
}

impl Rule {
    fn seams(mut self, seams: bool) -> Self {
        self.seams = seams;
        self
    }

    fn by_texture(mut self) -> Self {
        self.by_texture = true;
        self
    }
}

struct Plan {
    /// The start of its rules' file names.
    name: String,
    /// The block states it matches (`name:prop=value`, without `create:`).
    blocks: Vec<String>,
    join: Join,
    rules: Vec<Rule>,
}

fn rule(original: &str, sheet: &str, ty: CtType, faces: Faces) -> Rule {
    Rule {
        original: format!("create:block/{original}"),
        sheet: sheet.to_owned(),
        ty,
        faces,
        seams: true,
        by_texture: false,
        random: Vec::new(),
    }
}

const WOODS: [&str; 11] = [
    "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "crimson", "warped",
    "cherry", "bamboo",
];

const STONES: [&str; 14] = [
    "granite",
    "diorite",
    "andesite",
    "calcite",
    "dripstone",
    "deepslate",
    "tuff",
    "asurine",
    "crimsite",
    "limestone",
    "ochrum",
    "scorchia",
    "scoria",
    "veridium",
];

/// The faces whose direction is on `axis` (`true`) or not.
fn faces_of(axis: char, along: bool) -> Faces {
    match (axis, along) {
        ('x', true) => Faces::Some(vec!["east", "west"]),
        ('y', true) => Faces::Ends,
        ('z', true) => Faces::Some(vec!["north", "south"]),
        ('x', false) => Faces::Some(vec!["north", "south", "up", "down"]),
        ('y', false) => Faces::Sides,
        _ => Faces::Some(vec!["east", "west", "up", "down"]),
    }
}

/// The face on `axis` toward its positive (`true`) or negative end.
fn end_face(axis: char, positive: bool) -> &'static str {
    match (axis, positive) {
        ('x', true) => "east",
        ('x', false) => "west",
        ('y', true) => "up",
        ('y', false) => "down",
        ('z', true) => "south",
        _ => "north",
    }
}

/// Every block given connected textures, as Create registers them.
fn plans() -> Vec<Plan> {
    use CtType::*;
    let mut out = Vec::new();
    let mut plan = |name: &str, blocks: &[&str], join: Join, rules: Vec<Rule>| {
        out.push(Plan {
            name: name.to_owned(),
            blocks: blocks.iter().map(|b| (*b).to_owned()).collect(),
            join,
            rules,
        });
    };
    for casing in [
        "andesite_casing",
        "brass_casing",
        "copper_casing",
        "shadow_steel_casing",
        "refined_radiance_casing",
    ] {
        let sheet = format!("{casing}_connected");
        plan(
            casing,
            &[casing],
            Join::Casing,
            vec![rule(casing, &sheet, Omni, Faces::All)],
        );
    }
    plan(
        "railway_casing",
        &["railway_casing"],
        Join::Casing,
        vec![
            rule(
                "railway_casing_side",
                "railway_casing_side_connected",
                Omni,
                Faces::Sides,
            ),
            rule(
                "railway_casing",
                "railway_casing_connected",
                Omni,
                Faces::Ends,
            ),
        ],
    );
    // `EncasedCTBehaviour` and `CasingConnectivity`: an encased block's casing joins on the
    // faces its entry calls valid, those of another block of the same casing whose faces are
    // valid too. Here a block joins the same block in the same state (the same axis and
    // shafts), on the faces that state makes valid.
    for (casing, kinds) in [("andesite_casing", "andesite"), ("brass_casing", "brass")] {
        let sheet = format!("{casing}_connected");
        let shaft = format!("{kinds}_encased_shaft");
        for axis in ['x', 'y', 'z'] {
            // `encasedShaft`: the faces off the shaft's axis.
            let state = format!("{shaft}:axis={axis}");
            plan(
                &format!("{shaft}_{axis}"),
                &[&state],
                Join::State,
                vec![rule(casing, &sheet, Omni, faces_of(axis, false))],
            );
            // `encasedCogwheelBase`: the faces on the cog's axis without a shaft.
            for cog in [
                format!("{kinds}_encased_cogwheel"),
                format!("{kinds}_encased_large_cogwheel"),
            ] {
                for (positive, prop) in [(true, "top_shaft"), (false, "bottom_shaft")] {
                    let state = format!("{cog}:axis={axis}:{prop}=false");
                    plan(
                        &format!("{cog}_{axis}_{prop}"),
                        &[&state],
                        Join::State,
                        vec![
                            rule(
                                casing,
                                &sheet,
                                Omni,
                                Faces::Some(vec![end_face(axis, positive)]),
                            )
                            // `EncasedCogCTBehaviour.reverseUVs`: a horizontal cog's positive faces
                            // read their turned texture.
                            .by_texture(),
                        ],
                    );
                }
            }
            // `EncasedCogCTBehaviour`: a small cog's sides show the cog through their slit,
            // joined along it to the same cog on the same axis. Create turns the sheet by the
            // face (`VERTICAL` on some, `HORIZONTAL` on others, and reversed UVs) so that its
            // slit runs along the texture's width: there it is one row.
            let cog = format!("{kinds}_encased_cogwheel");
            let side = format!("{kinds}_encased_cogwheel_side");
            let state = format!("{cog}:axis={axis}");
            plan(
                &format!("{cog}_{axis}_sides"),
                &[&state],
                Join::State,
                vec![
                    rule(
                        &side,
                        &format!("{side}_connected"),
                        HorizontalByTexture,
                        faces_of(axis, false),
                    )
                    .seams(false)
                    .by_texture(),
                ],
            );
        }
    }
    // The gearbox: the faces on its axis.
    for axis in ['x', 'y', 'z'] {
        let state = format!("gearbox:axis={axis}");
        plan(
            &format!("gearbox_{axis}"),
            &[&state],
            Join::State,
            vec![rule(
                "andesite_casing",
                "andesite_casing_connected",
                Omni,
                faces_of(axis, true),
            )],
        );
    }
    // The encased fluid pipe: each face without a pipe through it.
    for face in ["north", "south", "east", "west", "up", "down"] {
        let state = format!("encased_fluid_pipe:{face}=false");
        plan(
            &format!("encased_fluid_pipe_{face}"),
            &[&state],
            Join::State,
            vec![rule(
                "copper_casing",
                "copper_casing_connected",
                Omni,
                Faces::Some(vec![face]),
            )],
        );
    }
    let glass = "palettes/framed_glass";
    let framed = |faces| rule(glass, "palettes/framed_glass_connected", Omni, faces);
    let horizontal = |faces| {
        rule(
            glass,
            "palettes/horizontal_framed_glass_connected",
            HorizontalKryppers,
            faces,
        )
    };
    let vertical = |faces| {
        rule(
            glass,
            "palettes/vertical_framed_glass_connected",
            Vertical,
            faces,
        )
    };
    plan(
        "framed_glass",
        &["framed_glass"],
        Join::Block,
        vec![framed(Faces::All)],
    );
    plan(
        "horizontal_framed_glass",
        &["horizontal_framed_glass"],
        Join::Block,
        vec![horizontal(Faces::Sides), framed(Faces::Ends)],
    );
    plan(
        "vertical_framed_glass",
        &["vertical_framed_glass"],
        Join::Block,
        vec![vertical(Faces::Sides)],
    );
    plan(
        "framed_glass_pane",
        &["framed_glass_pane"],
        Join::Block,
        vec![framed(Faces::All).seams(false)],
    );
    plan(
        "horizontal_framed_glass_pane",
        &["horizontal_framed_glass_pane"],
        Join::Block,
        vec![horizontal(Faces::All).seams(false)],
    );
    plan(
        "vertical_framed_glass_pane",
        &["vertical_framed_glass_pane"],
        Join::Block,
        vec![vertical(Faces::All).seams(false)],
    );
    // `TrapdoorCTBehaviour`: framed glass, joined to the same trapdoor lying or standing the
    // same way (`TrainTrapdoorBlock.isConnected`).
    plan(
        "framed_glass_trapdoor",
        &["framed_glass_trapdoor"],
        Join::State,
        vec![framed(Faces::All).seams(false)],
    );
    let mut window = |name: &str, ty: CtType| {
        let tex = format!("palettes/{name}");
        let sheet = format!("palettes/{name}_connected");
        plan(
            name,
            &[name],
            Join::Block,
            vec![rule(&tex, &sheet, ty, Faces::Sides)],
        );
        let pane = format!("{name}_pane");
        plan(
            &pane,
            &[&pane],
            Join::Block,
            vec![rule(&tex, &sheet, ty, Faces::All).seams(false)],
        );
    };
    for wood in WOODS {
        window(&format!("{wood}_window"), Vertical);
    }
    window("ornate_iron_window", Vertical);
    window("industrial_iron_window", Rectangle);
    // `WeatheredIronWindowCTBehaviour`: each side takes one of four sheets at random.
    let weathered = |faces| {
        let mut r = rule("palettes/weathered_iron_window", "", Rectangle, faces);
        r.random = (1..=4)
            .map(|k| format!("palettes/weathered_iron_window_{k}_connected"))
            .collect();
        r
    };
    plan(
        "weathered_iron_window",
        &["weathered_iron_window"],
        Join::Block,
        vec![weathered(Faces::Sides)],
    );
    plan(
        "weathered_iron_window_pane",
        &["weathered_iron_window_pane"],
        Join::Block,
        vec![weathered(Faces::All).seams(false)],
    );
    for stone in STONES {
        let layered = format!("palettes/stone_types/layered/{stone}_cut_layered");
        let pillar = format!("palettes/stone_types/pillar/{stone}_cut_pillar");
        let cap = format!("palettes/stone_types/cap/{stone}_cut_cap");
        let block = format!("layered_{stone}");
        plan(
            &block,
            &[&block],
            Join::Block,
            vec![
                rule(
                    &layered,
                    &format!("{layered}_connected"),
                    HorizontalKryppers,
                    Faces::Sides,
                ),
                rule(&cap, &format!("{cap}_connected"), Omni, Faces::Ends),
            ],
        );
        // `RotatedPillarCTBehaviour`: the pillar's sides and caps turned with its axis, as its
        // turned `cube_column` model is. Create's `ConnectedPillarBlock` sides (joined only to a
        // column of the same length) are not kept by the port: same pillars join.
        let block = format!("{stone}_pillar");
        plan(
            &block,
            &[&block],
            Join::Block,
            vec![
                rule(
                    &pillar,
                    &format!("{pillar}_connected"),
                    Rectangle,
                    Faces::All,
                )
                .seams(false)
                .by_texture(),
                rule(&cap, &format!("{cap}_connected"), Omni, Faces::All).by_texture(),
            ],
        );
    }
    // `MetalScaffoldingCTBehaviour`: a horizontal scaffolding's (`bottom`) outside and inside
    // in rows, its casing top joined like the casing; only bottom scaffolding joins (the same
    // state: horizontal scaffolding are all at the template's distance 7, as they never fall).
    for metal in ["andesite", "brass", "copper"] {
        let block = format!("{metal}_scaffolding");
        let state = format!("{block}:bottom=true");
        let side = format!("scaffold/{metal}_scaffold");
        let inside = format!("scaffold/{metal}_scaffold_inside");
        let casing = format!("{metal}_casing");
        plan(
            &block,
            &[&state],
            Join::State,
            vec![
                rule(
                    &side,
                    &format!("{side}_connected"),
                    Horizontal,
                    Faces::Sides,
                )
                .seams(false),
                rule(
                    &inside,
                    &format!("{inside}_connected"),
                    Horizontal,
                    Faces::Sides,
                )
                .seams(false),
                rule(&casing, &format!("{casing}_connected"), Omni, Faces::Ends),
            ],
        );
    }
    // `ChassisCTBehaviour`: the sides joined to the same chassis, the ends (sticky or not) too
    // unless covered, turned with the chassis' axis.
    for (block, side) in [
        ("linear_chassis", "linear_chassis_side"),
        ("secondary_linear_chassis", "secondary_linear_chassis_side"),
    ] {
        let end = |t: &str| rule(t, &format!("{t}_connected"), Omni, Faces::All).by_texture();
        plan(
            block,
            &[block],
            Join::Block,
            vec![
                rule(side, &format!("{side}_connected"), Omni, Faces::All)
                    .seams(false)
                    .by_texture(),
                end("linear_chassis_end"),
                end("linear_chassis_end_sticky"),
            ],
        );
    }
    // `GirderCTBehaviour`: a lone pole's sides, joined to the poles above and below.
    plan(
        "metal_girder",
        &["metal_girder:x=false:z=false"],
        Join::Block,
        vec![
            rule(
                "girder_pole_side",
                "girder_pole_side_connected",
                Vertical,
                Faces::Sides,
            )
            .seams(false)
            .by_texture(),
        ],
    );
    // `ItemVaultCTBehaviour`: each face's sheet of the vault's size (a large vault's when it is
    // 3 wide, else the medium one's), joined to the same vault block in the same state of the
    // same vault.
    for (large, size) in [(false, "medium"), (true, "large")] {
        let state = format!("item_vault:large={large}");
        let rules = ["front", "side", "top", "bottom"]
            .iter()
            .map(|face| {
                let original = format!("vault/vault_{face}_small");
                rule(
                    &original,
                    &format!("vault/vault_{face}_{size}"),
                    Rectangle,
                    Faces::All,
                )
                .seams(false)
                .by_texture()
            })
            .collect();
        plan(
            &format!("item_vault_{size}"),
            &[&state],
            Join::Group { state: true },
            rules,
        );
    }
    // `FluidTankCTBehaviour` (`HorizontalCTBehaviour` of `RECTANGLE` sheets): the sides', the
    // top's and the inside's sheets, joined to the same tank block of the same tank.
    for (block, side, top, inner) in [
        (
            "fluid_tank",
            "fluid_tank",
            "fluid_tank_top",
            "fluid_tank_inner",
        ),
        (
            "creative_fluid_tank",
            "creative_fluid_tank",
            "creative_casing",
            "creative_casing",
        ),
    ] {
        let mut rules = vec![
            rule(side, &format!("{side}_connected"), Rectangle, Faces::Sides),
            rule(top, &format!("{top}_connected"), Rectangle, Faces::Ends),
        ];
        if inner != top {
            rules.push(rule(
                inner,
                &format!("{inner}_connected"),
                Rectangle,
                Faces::Ends,
            ));
        }
        plan(block, &[block], Join::Group { state: false }, rules);
    }
    out
}

/// A rule's faces by their names in a rule's `faces`; none for a random rule.
fn rule_faces(rule: &Rule) -> Option<Vec<&'static str>> {
    if !rule.random.is_empty() {
        return None;
    }
    Some(match &rule.faces {
        Faces::All => vec!["all"],
        Faces::Sides => vec!["sides"],
        Faces::Ends => vec!["up", "down"],
        Faces::Some(names) => names.clone(),
    })
}

/// A plan's block entry as a rule names it: `name:prop=value` becomes a block with that state.
fn native_block(b: &str) -> serde_json::Value {
    let mut parts = b.split(':');
    let block = format!("create:{}", parts.next().unwrap_or_default());
    let state: serde_json::Map<String, serde_json::Value> = parts
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_owned(), json!([v])))
        .collect();
    if state.is_empty() {
        json!(block)
    } else {
        json!({ "block": block, "state": state })
    }
}

/// The rules, their shared tables and the sheets they draw from. The outcomes use exactly
/// Create's context and sheet index; only ids and neighbour tests are written.
pub(super) fn native_assets() -> (BTreeMap<String, Vec<u8>>, BTreeSet<String>) {
    let mut files = BTreeMap::new();
    let mut sprites = BTreeSet::new();
    for plan in plans() {
        let group = matches!(plan.join, Join::Group { .. });
        let connect = match plan.join {
            Join::Block | Join::Casing | Join::Group { state: false } => "block",
            Join::State | Join::Group { state: true } => "state",
        };
        for (i, rule) in plan.rules.into_iter().enumerate() {
            let Some(faces) = rule_faces(&rule) else {
                continue;
            };
            let sheet = format!("create:block/{}", rule.sheet);
            sprites.insert(sheet.clone());
            let table = if rule.seams {
                rule.ty.name().to_owned()
            } else {
                format!("{}_uncovered", rule.ty.name())
            };
            files
                .entry(format!(
                    "assets/create/sunrise/connected_tables/{table}.json"
                ))
                .or_insert_with(|| {
                    let grid = rule.ty.sheet_size();
                    let tests: &[(&str, u8)] = match rule.ty {
                        CtType::Horizontal
                        | CtType::HorizontalKryppers
                        | CtType::HorizontalByTexture => &[("left", L), ("right", R)],
                        CtType::Vertical => &[("up", U), ("down", D)],
                        CtType::Rectangle => &[("up", U), ("right", R), ("down", D), ("left", L)],
                        CtType::Omni => &[
                            ("up", U),
                            ("up_right", UR),
                            ("right", R),
                            ("down_right", DR),
                            ("down", D),
                            ("down_left", DL),
                            ("left", L),
                            ("up_left", UL),
                        ],
                    };
                    let tiles: Vec<_> = (0..1 << tests.len())
                        .map(|outcome| {
                            let mask = tests
                                .iter()
                                .enumerate()
                                .filter(|(bit, _)| outcome & (1 << bit) != 0)
                                .fold(0, |mask, (_, (_, offset))| mask | offset);
                            rule.ty.index(context(mask))
                        })
                        .collect();
                    crate::pretty(&json!({
                        "tests": tests.iter().map(|(name, _)| name).collect::<Vec<_>>(),
                        "cover": rule.seams,
                        "grid": [grid, grid],
                        "tiles": tiles,
                    }))
                });
            let mut file = json!({
                "method": "table",
                "blocks": plan.blocks.iter().map(|b| native_block(b)).collect::<Vec<_>>(),
                "sprites": [rule.original],
                "faces": faces,
                "connect": connect,
                "sheet": sheet,
                "table": table,
            });
            if group {
                file["connect_group"] = json!(true);
            }
            if rule.by_texture {
                file["orient"] = json!("texture");
            }
            files.insert(
                format!("assets/create/sunrise/connected/{}_{i}.json", plan.name),
                crate::pretty(&file),
            );
        }
    }
    (files, sprites)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(mask: u8) -> Ctx {
        context(mask)
    }

    #[test]
    fn sunrise_tables_keep_create_s_face_directions() {
        let (files, _) = native_assets();
        let table = |name| {
            serde_json::from_slice::<serde_json::Value>(
                &files[&format!("assets/create/sunrise/connected_tables/{name}.json")],
            )
            .unwrap()
        };
        let horizontal = table("horizontal_kryppers");
        assert_eq!(horizontal["tests"], json!(["left", "right"]));
        assert_eq!(horizontal["tiles"], json!([0, 3, 2, 1]));
        let vertical = table("vertical");
        assert_eq!(vertical["tests"], json!(["up", "down"]));
        assert_eq!(vertical["tiles"], json!([0, 1, 2, 3]));
        let omni = table("omni");
        assert_eq!(omni["tiles"][0x82], json!(0)); // two unsupported diagonal neighbours
        assert_eq!(omni["tiles"][0xff], json!(54));
        assert_eq!(omni["cover"], json!(true));
        let pane = table("omni_uncovered");
        assert_eq!(pane["cover"], json!(false));
        let by_texture = table("horizontal_by_texture_uncovered");
        assert_eq!(by_texture["tests"], json!(["left", "right"]));
        assert_eq!(by_texture["tiles"], json!([0, 1, 2, 3]));
    }

    #[test]
    fn rules_name_the_block_texture_faces_and_join() {
        let (files, sheets) = native_assets();
        let rule = |name: &str| {
            serde_json::from_slice::<serde_json::Value>(
                &files[&format!("assets/create/sunrise/connected/{name}.json")],
            )
            .unwrap()
        };
        let casing = rule("andesite_casing_0");
        assert_eq!(casing["blocks"], json!(["create:andesite_casing"]));
        assert_eq!(casing["sprites"], json!(["create:block/andesite_casing"]));
        assert_eq!(casing["faces"], json!(["all"]));
        assert_eq!(casing["connect"], json!("block"));
        assert_eq!(
            casing["sheet"],
            json!("create:block/andesite_casing_connected")
        );
        assert_eq!(casing["table"], json!("omni"));
        // An encased shaft joins its own state on the faces off its axis.
        let shaft = rule("andesite_encased_shaft_x_0");
        assert_eq!(
            shaft["blocks"],
            json!([{ "block": "create:andesite_encased_shaft", "state": { "axis": ["x"] } }])
        );
        assert_eq!(shaft["connect"], json!("state"));
        assert_eq!(shaft["faces"], json!(["north", "south", "up", "down"]));
        // A pillar's sides read its turned texture.
        let pillar = rule("granite_pillar_0");
        assert_eq!(pillar["orient"], json!("texture"));
        assert_eq!(pillar["table"], json!("rectangle_uncovered"));
        let pipe = rule("encased_fluid_pipe_up_0");
        assert_eq!(pipe["faces"], json!(["up"]));
        // The weathered window's random sheets have no rule.
        assert!(!files.keys().any(|f| f.contains("weathered_iron_window")));
        assert!(!sheets.iter().any(|s| s.contains("weathered_iron_window")));
        assert!(sheets.contains("create:block/brass_casing_connected"));
    }

    #[test]
    fn omni_reads_create_s_sheet() {
        // Alone: Create's top-left tile; all four sides and no corner: (3, 3); joined all round
        // with every corner: (6, 6), the sheet's plain middle.
        assert_eq!(CtType::Omni.index(ctx(0)), 0);
        assert_eq!(CtType::Omni.index(ctx(U | R | D | L)), 3 + 8 * 3);
        assert_eq!(CtType::Omni.index(ctx(0xff)), 6 + 8 * 6);
        // Joined up only: column 1 of row 0; down only: column 2; left only: row 1.
        assert_eq!(CtType::Omni.index(ctx(U)), 1);
        assert_eq!(CtType::Omni.index(ctx(D)), 2);
        assert_eq!(CtType::Omni.index(ctx(L)), 8);
        // Up and left with the corner between: (1 + 3, 1).
        assert_eq!(CtType::Omni.index(ctx(U | L | UL)), 4 + 8);
        // Three sides (not right) and the top-left corner: row 4, column 1.
        assert_eq!(CtType::Omni.index(ctx(U | D | L | UL)), 1 + 8 * 4);
    }

    #[test]
    fn a_corner_joins_only_beside_two_joined_sides() {
        assert_eq!(ctx(UL), Ctx::default());
        assert!(!ctx(U | UL).top_left);
        assert!(ctx(U | L | UL).top_left);
    }

    #[test]
    fn straight_types() {
        use CtType::*;
        let side = |l, r| Ctx {
            left: l,
            right: r,
            ..Ctx::default()
        };
        assert_eq!(HorizontalKryppers.index(side(false, false)), 0);
        assert_eq!(HorizontalKryppers.index(side(true, true)), 1);
        assert_eq!(HorizontalKryppers.index(side(false, true)), 2);
        assert_eq!(HorizontalKryppers.index(side(true, false)), 3);
        let col = |u, d| Ctx {
            up: u,
            down: d,
            ..Ctx::default()
        };
        assert_eq!(Vertical.index(col(true, false)), 1);
        assert_eq!(Vertical.index(col(false, true)), 2);
        // A lone rectangle tile is (0, 3); joined all four ways (2, 1).
        assert_eq!(Rectangle.index(Ctx::default()), 12);
        assert_eq!(Rectangle.index(ctx(U | R | D | L)), 2 + 4);
    }

    #[test]
    fn horizontal_sheets_by_create_and_by_texture() {
        // `HORIZONTAL`: joined right is the second tile; read by texture, joined left is.
        let side = |l, r| Ctx {
            left: l,
            right: r,
            ..Ctx::default()
        };
        assert_eq!(CtType::Horizontal.index(side(false, true)), 1);
        assert_eq!(CtType::HorizontalByTexture.index(side(true, false)), 1);
        assert_eq!(CtType::HorizontalByTexture.index(side(true, true)), 3);
    }
}
