//! Key mappings by name: vanilla's (`key.jump`) and the mod's own, resolved when asked.

use mod_api::{ClientHost, MappingError, Text};

/// `KeyMapping.isDown` of vanilla's mapping `name`.
pub fn down(host: &dyn ClientHost, name: &str) -> bool {
    host.vanilla_key_mapping(name)
        .and_then(|m| m.is_down(host))
        .unwrap_or(false)
}

/// Whether the key bound to vanilla's mapping `name` is held (`InputConstants.isKeyDown`).
pub fn physical_down(host: &dyn ClientHost, name: &str) -> bool {
    host.vanilla_key_mapping(name)
        .and_then(|m| m.physical_down(host))
        .unwrap_or(false)
}

/// `KeyMapping.setDown` of vanilla's mapping `name`.
pub fn set_down(host: &mut dyn ClientHost, name: &str, down: bool) -> Result<(), MappingError> {
    let mapping = host.vanilla_key_mapping(name)?;
    mapping.set_down(host, down)
}

/// `KeyMapping.getTranslatedKeyMessage` of vanilla's mapping `name`.
pub fn label(host: &dyn ClientHost, name: &str) -> Option<Text> {
    host.vanilla_key_mapping(name)
        .and_then(|m| m.translated_key_message(host))
        .ok()
}

/// The key the mod's mapping `id` is bound to.
pub fn mod_binding(host: &dyn ClientHost, owner: &str, id: &str) -> Option<String> {
    let mapping = host.mod_key_mapping(owner, id).ok()?;
    mapping.binding(host).ok().map(|(key, _)| key.to_owned())
}

/// Whether the mod's mapping `id` is held.
pub fn mod_down(host: &dyn ClientHost, owner: &str, id: &str) -> bool {
    host.mod_key_mapping(owner, id)
        .and_then(|m| m.is_down(host))
        .unwrap_or(false)
}

/// `KeyMapping.consumeClick` of the mod's mapping `id`.
pub fn mod_consume_click(host: &mut dyn ClientHost, owner: &str, id: &str) -> bool {
    host.mod_key_mapping(owner, id)
        .and_then(|m| m.consume_click(host))
        .unwrap_or(false)
}
