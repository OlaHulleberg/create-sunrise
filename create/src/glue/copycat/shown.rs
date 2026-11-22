//! The material each side's copycats show, by position, for `CopycatBlock.getFriction`: the
//! `friction` hook names no level and its level view can't read a block entity's data, so the
//! server keeps each copycat's material as it shows it (`show`), by level, and a client what its
//! copycats' block entities draw.

use mod_api::BlockState;
use std::cell::RefCell;
use std::collections::HashMap;

/// The materials of the copycats at a position, by level.
type Materials = HashMap<[i32; 3], Vec<(usize, BlockState)>>;

thread_local! {
    /// The server's: each copycat holding a material, by position and level.
    static SERVER: RefCell<Materials> = RefCell::new(HashMap::new());
    /// A client's: each copycat drawn with a material, by position.
    static CLIENT: RefCell<HashMap<[i32; 3], BlockState>> = RefCell::new(HashMap::new());
}

/// The server's copycat at `pos` in level `dim` shows `material` (None: none).
pub(super) fn set_server(dim: usize, pos: [i32; 3], material: Option<BlockState>) {
    SERVER.with_borrow_mut(|m| {
        let list = m.entry(pos).or_default();
        list.retain(|(d, _)| *d != dim);
        if let Some(s) = material {
            list.push((dim, s));
        }
        if list.is_empty() {
            m.remove(&pos);
        }
    });
}

/// A client drew the copycat at `pos` with `material` (None: none).
pub(super) fn set_client(pos: [i32; 3], material: Option<BlockState>) {
    CLIENT.with_borrow_mut(|m| match material {
        Some(s) => m.insert(pos, s),
        None => m.remove(&pos),
    });
}

/// The material of the copycat at `pos`, on the server (the first level with one holding a
/// material there) or a client.
pub(super) fn material(server: bool, pos: [i32; 3]) -> Option<BlockState> {
    if server {
        SERVER.with_borrow(|m| m.get(&pos).and_then(|l| l.first()).map(|s| s.1))
    } else {
        CLIENT.with_borrow(|m| m.get(&pos).copied())
    }
}

pub(super) fn reset_server() {
    SERVER.with_borrow_mut(HashMap::clear);
}

pub(super) fn reset_client() {
    CLIENT.with_borrow_mut(HashMap::clear);
}
