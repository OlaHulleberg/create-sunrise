//! Owned copies of the stacks the API lends: a player's inventory, a living entity's hands and
//! equipment, a dropped item's and a place's stack, and a level's entities and players. None
//! where the API lends none.

use mod_api::{
    EntityRef, EquipmentSlot, Hand, Host, ItemEntityRef, ItemPlace, ItemStack, LevelRef, LivingRef,
    PlayerRef, ServerHost,
};
use std::ops::ControlFlow;

pub trait OwnedInventory {
    /// `Inventory.items`: the non-empty main inventory stacks in slot order.
    fn inventory(self, host: &dyn Host) -> Option<Vec<ItemStack>>;
    /// Every occupied inventory slot with its stack, by the inventory's slot number.
    fn inventory_slots(self, host: &dyn Host) -> Option<Vec<(u32, ItemStack)>>;
}

impl OwnedInventory for PlayerRef {
    fn inventory(self, host: &dyn Host) -> Option<Vec<ItemStack>> {
        self.with_inventory(host, |view| {
            view.iter()
                .take_while(|(slot, _)| *slot < 36)
                .filter_map(|(_, stack)| stack.project())
                .collect()
        })
    }

    fn inventory_slots(self, host: &dyn Host) -> Option<Vec<(u32, ItemStack)>> {
        self.with_inventory(host, |view| {
            view.iter()
                .filter_map(|(slot, stack)| Some((slot, stack.project()?)))
                .collect()
        })
    }
}

pub trait OwnedEquipment {
    fn hand_item(self, host: &dyn Host, hand: Hand) -> Option<ItemStack>;
    fn equipment(self, host: &dyn Host, slot: EquipmentSlot) -> Option<ItemStack>;
}

impl OwnedEquipment for LivingRef {
    fn hand_item(self, host: &dyn Host, hand: Hand) -> Option<ItemStack> {
        self.with_hand_item(host, hand, |stack| stack.project())
            .flatten()
    }

    fn equipment(self, host: &dyn Host, slot: EquipmentSlot) -> Option<ItemStack> {
        self.with_equipment(host, slot, |stack| stack.project())
            .flatten()
    }
}

pub trait OwnedStack {
    fn stack(self, host: &dyn Host) -> Option<ItemStack>;
}

impl OwnedStack for ItemEntityRef {
    fn stack(self, host: &dyn Host) -> Option<ItemStack> {
        self.with_stack(host, |stack| stack.project()).flatten()
    }
}

pub trait OwnedPlace {
    fn stack(&self, host: &mut dyn ServerHost) -> Option<ItemStack>;
}

impl OwnedPlace for ItemPlace {
    fn stack(&self, host: &mut dyn ServerHost) -> Option<ItemStack> {
        self.with_stack(host, |stack| stack.project()).flatten()
    }
}

pub trait LevelLists {
    /// `Level.getEntities`: the entities whose boxes meet `min`..`max`.
    fn entities_in(self, host: &dyn Host, min: [f64; 3], max: [f64; 3]) -> Option<Vec<EntityRef>>;
    /// `ServerLevel.players`.
    fn online_players(self, host: &dyn Host) -> Option<Vec<PlayerRef>>;
}

impl LevelLists for LevelRef {
    fn entities_in(self, host: &dyn Host, min: [f64; 3], max: [f64; 3]) -> Option<Vec<EntityRef>> {
        let mut found = Vec::new();
        let _ = self
            .visit_entities_in(host, min, max, &mut |e| {
                found.push(e);
                ControlFlow::Continue(())
            })
            .ok()?;
        Some(found)
    }

    fn online_players(self, host: &dyn Host) -> Option<Vec<PlayerRef>> {
        let mut found = Vec::new();
        let _ = self
            .visit_online_players(host, &mut |p| {
                found.push(p);
                ControlFlow::Continue(())
            })
            .ok()?;
        Some(found)
    }
}
