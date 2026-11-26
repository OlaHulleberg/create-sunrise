//! The actor's inventory as `ItemInputs.apply` moves it. The offered stack goes into the main
//! hand (`setItemInHand`), which a drop empties again; what the port gives the player takes
//! `PlayerRef::give_stack`'s path (`Inventory.add`: onto the stacks of the item, then the empty
//! slots, the selected one first). The outcome is what the harness records of it: the hand after
//! a use, and the inventory's gain by item.
use crate::transport::{Input, Outcome, Stack};
use std::collections::BTreeMap;

/// A player's main inventory: 36 slots, the first nine its hotbar.
const MAIN: usize = 36;

/// What the inventory holds, by item.
type Totals = BTreeMap<String, i32>;

/// The actor's inventory. A new player's selected slot is the first.
#[derive(Debug, PartialEq)]
pub(super) struct Inventory {
    slots: Vec<Option<Stack>>,
    selected: usize,
}

impl Inventory {
    pub(super) fn new() -> Inventory {
        Inventory {
            slots: vec![None; MAIN],
            selected: 0,
        }
    }

    /// `ItemInputs.apply`'s start: the offered stack is put in the main hand, which a drop
    /// empties again; what the inventory then holds, by item.
    pub(super) fn begin(&mut self, input: &Input) -> Totals {
        self.slots[self.selected] = match input {
            Input::DropOn { .. } => None,
            Input::Use { stack, .. } => stack.clone(),
        };
        self.totals()
    }

    /// `ItemInputs.apply`'s record of the port's answer: `remainder` is the hand the use left
    /// (for a drop, the item's leftover) and `taken` what the port gave the player, which
    /// `placeItemBackInInventory` takes back into the inventory. The hand the harness reads
    /// (only when something was offered), and the inventory's gain by item (a drop's is not
    /// recorded).
    pub(super) fn outcome(&mut self, input: &Input, outcome: Outcome, before: &Totals) -> Outcome {
        let Outcome {
            pos,
            offered,
            mut remainder,
            taken,
            handler,
        } = outcome;
        if let Input::Use { .. } = input {
            self.slots[self.selected] = remainder.take();
        }
        for stack in taken {
            self.give(stack);
        }
        let remainder = match input {
            Input::DropOn { .. } => {
                return Outcome {
                    pos,
                    offered,
                    remainder,
                    handler,
                    taken: Vec::new(),
                };
            }
            Input::Use { stack, .. } => stack.is_some().then(|| self.hand()).flatten(),
        };
        let mut after = self.totals();
        if let Some(s) = &offered {
            *after.entry(s.item.clone()).or_default() += i32::from(s.count);
        }
        if let Some(s) = &remainder {
            let held = after.entry(s.item.clone()).or_default();
            *held -= i32::from(s.count);
        }
        let taken = after
            .into_iter()
            .filter_map(|(item, n)| {
                let gain = n - before.get(&item).copied().unwrap_or(0);
                (gain > 0).then(|| Stack {
                    item,
                    count: u8::try_from(gain).expect("a gain within a byte"),
                })
            })
            .collect();
        Outcome {
            pos,
            offered,
            remainder,
            taken,
            handler,
        }
    }

    /// The stack in the main hand.
    pub(super) fn hand(&self) -> Option<Stack> {
        self.slots[self.selected].clone()
    }

    /// Takes the stack in the main hand (`set_inventory_slot(selected, None)`).
    pub(super) fn take_hand(&mut self) -> Option<Stack> {
        self.slots[self.selected].take()
    }

    fn totals(&self) -> Totals {
        let mut totals = Totals::new();
        for s in self.slots.iter().flatten() {
            *totals.entry(s.item.clone()).or_default() += i32::from(s.count);
        }
        totals
    }

    /// `PlayerRef::give_stack` (`Inventory.add`): onto the stacks of the item, then into the
    /// empty slots, the selected one first. A stack of the transport contract's limit.
    pub(super) fn give(&mut self, mut stack: Stack) {
        let limit = crate::basin::limit(&stack.item);
        for held in self.slots.iter_mut().flatten() {
            if stack.count == 0 {
                break;
            }
            if held.item == stack.item && held.count < limit {
                let moved = stack.count.min(limit - held.count);
                held.count += moved;
                stack.count -= moved;
            }
        }
        for i in std::iter::once(self.selected).chain((0..MAIN).filter(|&i| i != self.selected)) {
            if stack.count == 0 {
                break;
            }
            if self.slots[i].is_none() {
                let count = stack.count.min(limit);
                self.slots[i] = Some(Stack {
                    item: stack.item.clone(),
                    count,
                });
                stack.count -= count;
            }
        }
        debug_assert_eq!(
            stack.count, 0,
            "what does not fit is thrown from the player"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item: &str, count: u8) -> Option<Stack> {
        Some(Stack {
            item: item.into(),
            count,
        })
    }

    fn given(items: &[(&str, u8)]) -> Vec<Stack> {
        items
            .iter()
            .map(|&(item, count)| Stack {
                item: item.into(),
                count,
            })
            .collect()
    }

    /// `processing-basin-bucket`: a bucket emptied into a basin leaves its empty bucket in the
    /// hand; one filled there lands in the empty main hand, as `give_stack` fills the selected
    /// slot first.
    #[test]
    fn what_is_given_lands_in_the_empty_main_hand() {
        let mut inventory = Inventory::new();
        let use_on = |stack: Option<Stack>| Input::Use {
            stack,
            face: crate::actions::Face::Up,
            yaw: 0.,
            sneaking: false,
        };
        let before = inventory.begin(&use_on(stack("minecraft:water_bucket", 1)));
        let outcome = inventory.outcome(
            &use_on(stack("minecraft:water_bucket", 1)),
            Outcome {
                pos: [0, 6, 0],
                offered: stack("minecraft:water_bucket", 1),
                remainder: stack("minecraft:bucket", 1),
                taken: Vec::new(),
                handler: None,
            },
            &before,
        );
        assert_eq!(outcome.remainder, stack("minecraft:bucket", 1));
        assert_eq!(outcome.taken, []);
        let before = inventory.begin(&use_on(stack("minecraft:bucket", 1)));
        let outcome = inventory.outcome(
            &use_on(stack("minecraft:bucket", 1)),
            Outcome {
                pos: [0, 6, 0],
                offered: stack("minecraft:bucket", 1),
                remainder: None,
                taken: given(&[("minecraft:water_bucket", 1)]),
                handler: None,
            },
            &before,
        );
        assert_eq!(outcome.remainder, stack("minecraft:water_bucket", 1));
        assert_eq!(outcome.taken, []);
    }

    /// An empty hand taking a depot's stacks reports them all as picked up (the harness reads no
    /// hand from an empty offer), and the next offered stack replaces the first in the main hand.
    #[test]
    fn an_empty_offer_reports_everything_given() {
        let mut inventory = Inventory::new();
        let empty = || Input::Use {
            stack: None,
            face: crate::actions::Face::Up,
            yaw: 0.,
            sneaking: false,
        };
        let before = inventory.begin(&empty());
        let outcome = inventory.outcome(
            &empty(),
            Outcome {
                pos: [1, 5, 0],
                offered: None,
                remainder: None,
                taken: given(&[("create:andesite_alloy", 2), ("minecraft:dirt", 3)]),
                handler: None,
            },
            &before,
        );
        assert_eq!(outcome.remainder, None);
        assert_eq!(
            outcome.taken,
            given(&[("create:andesite_alloy", 2), ("minecraft:dirt", 3)])
        );
        assert_eq!(inventory.hand(), stack("create:andesite_alloy", 2));
        let held = stack("minecraft:bucket", 1);
        let before = inventory.begin(&Input::Use {
            stack: held.clone(),
            face: crate::actions::Face::Up,
            yaw: 0.,
            sneaking: false,
        });
        let outcome = inventory.outcome(
            &Input::Use {
                stack: held.clone(),
                face: crate::actions::Face::Up,
                yaw: 0.,
                sneaking: false,
            },
            Outcome {
                pos: [1, 5, 0],
                offered: held.clone(),
                remainder: None,
                taken: given(&[("minecraft:water_bucket", 1)]),
                handler: None,
            },
            &before,
        );
        assert_eq!(outcome.remainder, stack("minecraft:water_bucket", 1));
        assert_eq!(outcome.taken, []);
    }

    /// A dropped stack leaves the hand, and its leftover is reported without a pickup.
    #[test]
    fn a_drop_leaves_the_hand_empty() {
        let mut inventory = Inventory::new();
        let dropped = Input::DropOn {
            stack: stack("minecraft:dirt", 2).unwrap(),
        };
        let before = inventory.begin(&dropped);
        let outcome = inventory.outcome(
            &dropped,
            Outcome {
                pos: [0, 6, 0],
                offered: stack("minecraft:dirt", 2),
                remainder: None,
                taken: Vec::new(),
                handler: None,
            },
            &before,
        );
        assert_eq!(outcome.remainder, None);
        assert_eq!(outcome.taken, []);
        assert_eq!(inventory.hand(), None);
    }
}
