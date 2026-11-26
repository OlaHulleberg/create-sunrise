//! The linked controller's frequency screen (`LinkedControllerScreen` over `LinkedControllerMenu`,
//! a `HeldItemGhostItemMenu`) on its menu (`create:linked_controller`; the server's side is
//! `filters`, which keeps every held ghost menu's session): Create's panel with six buttons' two
//! frequency slots each, reset and confirm, the player's inventory under it and the held
//! controller five times its size beside it. The menu's slots are the game's: the twelve ghost
//! slots and the player's inventory, clicked on the server as `create_engine::ghost_menu` rules.
//! A sneaking use with the controller in the main hand opens it (`LinkedControllerItem.use`);
//! what was chosen goes back when it closes (`HeldItemGhostItemMenu.removed`, `saveData`), on the
//! controller still in that hand.
//!
//! The frequencies are kept as item ids (the port's link frequencies), so a slot shows its item
//! without the components the stack it was copied from had.
//!
//! Reset sends `ClearMenuPacket` and confirm closes the menu, both at the client's next tick
//! (`filter_screen`).

use super::filter_screen;
use crate::glue::owned::OwnedEquipment as _;
use create_engine::controller::SLOTS;
use mod_api::{
    ClientHost, DrawContext, Flow, Hand, Host, ItemStack, MenuPart, PlayerRef,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent, ServerHost, SoundPlay,
    Text,
};
use std::cell::Cell;

/// `AllMenuTypes.LINKED_CONTROLLER`.
pub(crate) const CONTROLLER_MENU: &str = "create:linked_controller";

// ---------------------------------------------------------------------------------------------
// The server

fn item_stack(host: &dyn Host, id: &str) -> Option<ItemStack> {
    let key = host.item_key(&mod_api::Identifier::parse(id).ok()?)?;
    Some(ItemStack::of(key))
}

/// `player.openMenu(controller)`: the menu over the player's inventory, with the controller's
/// frequencies in its ghost slots.
pub(crate) fn open(host: &mut dyn ServerHost, player: PlayerRef, items: &[String]) {
    let h: &dyn Host = &*host;
    let ghost = (0..SLOTS)
        .map(|i| {
            items
                .get(i)
                .filter(|id| !id.is_empty())
                .and_then(|id| item_stack(h, id))
        })
        .collect();
    super::filters::open_controller(host, player, ghost);
}

/// The controller's menu: its ghost slots.
pub(crate) struct ControllerScreen;

#[mod_api::sunrise_part]
impl MenuPart for ControllerScreen {
    /// `HeldItemGhostItemMenu.stillValid`: the controller it opened from is still in that hand
    /// (the selected slot is checked each tick, `filters::player_tick`).
    fn menu_still_valid(
        &self,
        _context: &dyn mod_api::LoopContext,
        event: &mut mod_api::MenuStillValidEvent<'_>,
    ) {
        if let Some(opened) = &event.opening_item {
            event.valid &= opened.stack_matches;
        }
    }

    fn menu_clicked(
        &self,
        host: &mut dyn mod_api::ServerHost,
        event: &mut mod_api::MenuClickedEvent<'_>,
    ) -> mod_api::Flow {
        super::filters::menu_clicked(host, event)
    }

    fn menu_removed(
        &self,
        host: &mut dyn mod_api::ServerHost,
        event: &mut mod_api::MenuRemovedEvent<'_>,
    ) {
        super::filters::menu_removed(host, event);
    }

    /// The ghost slots take nothing from the cursor and give nothing up: the server's clicks
    /// copy into them, and the client's own prediction leaves them alone.
    fn menu_slot_rules(&self, _host: &dyn Host, event: &mut mod_api::MenuSlotRulesEvent<'_>) {
        if event.menu == CONTROLLER_MENU && event.container == mod_api::MenuContainer::Main {
            event.may_place = false;
            event.may_pickup = false;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The layout (`AllGuiTextures.LINKED_CONTROLLER`, `LinkedControllerMenu.addSlots`, the screen's
// `init`)

/// `LINKED_CONTROLLER` in `curiosities_2.png`.
const BG: [f32; 4] = [0.0, 0.0, 179.0, 109.0];
const INVENTORY_SIZE: [f32; 2] = [176.0, 108.0];
/// `setWindowOffset(1, 0)`.
const OFFSET: [f32; 2] = [1.0, 0.0];
/// `addPlayerSlots(8, 131)`.
const INVENTORY: [f32; 2] = [8.0, 131.0];

/// `setWindowSize(max(bg, inventory) wide, bg + 4 + inventory high)`.
fn window() -> [f32; 2] {
    [
        BG[2].max(INVENTORY_SIZE[0]),
        BG[3] + 4.0 + INVENTORY_SIZE[1],
    ]
}

/// `leftPos`, `topPos`: the window centred, then moved by its offset.
#[cfg(test)]
fn corner(size: [f32; 2]) -> [f32; 2] {
    let w = window();
    [
        ((size[0] - w[0]) / 2.0).floor() + OFFSET[0],
        ((size[1] - w[1]) / 2.0).floor() + OFFSET[1],
    ]
}

/// A ghost slot's top left: six columns of two, 24 apart, 11 more after the fourth.
fn slot_at(i: usize) -> [f32; 2] {
    let (column, row) = (i / 2, i % 2);
    let x = 12.0 + column as f32 * 24.0 + if column > 3 { 11.0 } else { 0.0 };
    [x, 34.0 + row as f32 * 18.0]
}

fn reset_button(c: [f32; 2]) -> [f32; 4] {
    [c[0] + BG[2] - 62.0, c[1] + BG[3] - 24.0, 18.0, 18.0]
}

fn confirm_button(c: [f32; 2]) -> [f32; 4] {
    [c[0] + BG[2] - 33.0, c[1] + BG[3] - 24.0, 18.0, 18.0]
}

fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

/// `ControlsUtil.getControls()`' mappings, button by button.
const MAPPINGS: [&str; 6] = [
    "key.forward",
    "key.back",
    "key.left",
    "key.right",
    "key.jump",
    "key.sneak",
];

// ---------------------------------------------------------------------------------------------
// The client

thread_local! {
    /// Whether the primary button is down, for the buttons' pressed look.
    static PRESSED: Cell<bool> = const { Cell::new(false) };
}

fn tr(host: &dyn ClientHost, key: &str) -> String {
    host.translate(key).unwrap_or_else(|| key.to_owned())
}

fn main_hand(host: &dyn ClientHost) -> Option<ItemStack> {
    let player = host.local_player()?;
    host.living(player.entity())?.hand_item(host, Hand::Main)
}

fn click_sound(host: &dyn ClientHost) {
    host.play_local_sound(
        &SoundPlay::ui("minecraft:ui.button.click").with_volume(0.25),
        0,
    );
}

/// `LinkedControllerScreen.init`: the window (`setWindowSize`, moved by `setWindowOffset`,
/// which a centred screen gets by growing twice the offset), the ghost slots and the player's
/// inventory where the menu puts them.
pub(crate) fn open_screen(
    _host: &dyn ClientHost,
    _open: mod_api::MenuScreenOpen<'_>,
) -> Option<mod_api::MenuScreen> {
    let w = window();
    let layout = mod_api::MenuScreenLayout {
        width: w[0] - 2.0 * OFFSET[0],
        height: w[1] - 2.0 * OFFSET[1],
        slots: (0..SLOTS)
            .map(|i| {
                Some(mod_api::ScreenSlot {
                    position: slot_at(i),
                    no_item_icon: None,
                })
            })
            .collect(),
        inventory_x: INVENTORY[0],
        inventory_y: INVENTORY[1],
        own_labels: true,
        own_background: true,
        ..Default::default()
    };
    Some(
        mod_api::MenuScreen::new(layout)
            .with_draw(screen_draw)
            .with_click(screen_click)
            .with_tooltip(screen_tooltip),
    )
}

/// `LinkedControllerScreen.renderBg` and the buttons, under the slots.
fn screen_draw(
    host: &dyn ClientHost,
    frame: mod_api::MenuScreenFrame<'_>,
    draw: &mut dyn DrawContext,
) {
    let mouse = frame.pointer.map(|p| p as f32);
    let c = frame.origin;
    // `renderPlayerInventory` at `getLeftOfCentered`, under the background.
    let p = [
        c[0] - OFFSET[0] + ((window()[0] - INVENTORY_SIZE[0]) / 2.0).floor(),
        c[1] + BG[3] + 4.0,
    ];
    draw.sprite(
        "create:gui/player_inventory",
        [0.0, 0.0, INVENTORY_SIZE[0], INVENTORY_SIZE[1]],
        [p[0], p[1], INVENTORY_SIZE[0], INVENTORY_SIZE[1]],
        None,
    );
    draw.text(
        &tr(host, "container.inventory"),
        [p[0] + 8.0, p[1] + 6.0],
        0xFF40_4040,
        false,
    );
    draw.sprite(
        "create:gui/curiosities_2",
        BG,
        [c[0], c[1], BG[2], BG[3]],
        None,
    );
    // The title (the controller's name) at (15, 4).
    let title = main_hand(host).map_or_else(
        || tr(host, "item.create.linked_controller"),
        |s| crate::glue::item_name(host, &s),
    );
    draw.text(&title, [c[0] + 15.0, c[1] + 4.0], 0xFF59_2424, false);
    // The held controller, five times its size.
    if let Some(held) = main_hand(host) {
        draw.item_stack(&held, [c[0] + BG[2] - 4.0, c[1] + BG[3] - 56.0], 80.0);
    }
    // Reset (`I_TRASH`) and confirm (`I_CONFIRM`).
    for (r, icon) in [
        (reset_button(c), [16.0, 0.0]),
        (confirm_button(c), [0.0, 16.0]),
    ] {
        let hovered = inside(r, mouse);
        let u = if hovered && PRESSED.get() {
            36.0
        } else if hovered {
            18.0
        } else {
            0.0
        };
        draw.sprite("create:gui/widgets", [u, 0.0, 18.0, 18.0], r, None);
        draw.sprite(
            "create:gui/icons",
            [icon[0], icon[1], 16.0, 16.0],
            [r[0] + 1.0, r[1] + 1.0, 16.0, 16.0],
            None,
        );
    }
}

/// The buttons (`IconButton.onClick`): reset is `ClearMenuPacket`, confirm closes the menu.
fn screen_click(
    host: &dyn ClientHost,
    frame: mod_api::MenuScreenFrame<'_>,
    right: bool,
) -> Option<i32> {
    if right {
        return None;
    }
    let at = frame.at;
    if inside(reset_button([0.0, 0.0]), at) {
        click_sound(host);
        filter_screen::clear_later();
        return Some(0);
    }
    if inside(confirm_button([0.0, 0.0]), at) {
        click_sound(host);
        filter_screen::close_later();
        return Some(0);
    }
    None
}

/// `renderTooltip`: a frequency slot's item tooltip and its button's key, in gold, unless the
/// cursor carries something.
fn screen_tooltip(
    host: &dyn ClientHost,
    _frame: mod_api::MenuScreenFrame<'_>,
    slot: Option<u32>,
    out: &mut Vec<(Text, u32)>,
) {
    let Some(i) = slot.map(|s| s as usize).filter(|i| *i < SLOTS) else {
        return;
    };
    let carried = host
        .local_player()
        .and_then(|p| p.container_menu(host))
        .and_then(|m| m.carried(host))
        .flatten();
    if carried.is_some() {
        return;
    }
    let key = crate::glue::keys::label(host, MAPPINGS[i / 2])
        .unwrap_or_else(|| Text::translate(create_engine::controller::KEYS[i / 2], Vec::new()));
    let mut style = mod_api::text::TextStyle::default();
    style.color = Some("gold".to_owned());
    out.push((
        Text::translate(
            format!("create.linked_controller.frequency_slot_{}", i % 2 + 1),
            vec![key],
        )
        .with_style(style),
        0xFFFF_FFFF,
    ));
}

pub(crate) fn screen_mouse_pressed(
    _host: &dyn ClientHost,
    event: &mut ScreenMouseButtonPressedPreEvent<'_>,
) -> Flow {
    if event.screen.menu == Some(CONTROLLER_MENU) {
        PRESSED.set(event.button == 0);
    }
    Flow::Continue
}

pub(crate) fn screen_mouse_released(
    _host: &dyn ClientHost,
    event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
) -> Flow {
    if event.screen.menu == Some(CONTROLLER_MENU) {
        PRESSED.set(false);
    }
    Flow::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_sit_in_six_pairs_with_a_gap_after_the_fourth() {
        // `LinkedControllerMenu.addSlots`: x 12, +24 a column, +11 after column 3; rows 18 apart.
        assert_eq!(slot_at(0), [12.0, 34.0]);
        assert_eq!(slot_at(1), [12.0, 52.0]);
        assert_eq!(slot_at(7), [84.0, 52.0]);
        assert_eq!(slot_at(8), [119.0, 34.0]);
        assert_eq!(slot_at(11), [143.0, 52.0]);
    }

    #[test]
    fn the_window_sits_as_creates() {
        // A 400 x 300 GUI: the window is 179 x 221, moved by (1, 0).
        let c = corner([400.0, 300.0]);
        assert_eq!(c, [111.0, 39.0]);
        let w = window();
        let grown = [w[0] - 2.0 * OFFSET[0], w[1] - 2.0 * OFFSET[1]];
        assert_eq!(
            [
                ((400.0 - grown[0]) / 2.0_f32).floor(),
                ((300.0 - grown[1]) / 2.0_f32).floor()
            ],
            c
        );
        assert_eq!(reset_button(c), [111.0 + 117.0, 39.0 + 85.0, 18.0, 18.0]);
        assert_eq!(confirm_button(c), [111.0 + 146.0, 39.0 + 85.0, 18.0, 18.0]);
    }
}
