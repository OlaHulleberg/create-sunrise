//! Saved data accepted by Create's schematic processor and final safe block preparation.

use crate::glue::{block_in_tag, copycat, display::clipboard};
use mod_api::{BlockKey, Compound, Host, Identifier, Tag, Text};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Access {
    Ordinary,
    GameMaster,
}

impl Access {
    pub(super) fn for_player(game_master: Option<bool>) -> Self {
        if game_master == Some(true) {
            Self::GameMaster
        } else {
            Self::Ordinary
        }
    }

    /// Check every entity in a summon tree, not just the one the printer charges materials for.
    pub(super) fn entities(self, host: &dyn Host, nbt: &mut Compound) -> Option<Identifier> {
        self.entity_tree(nbt, &|id| {
            host.registry_entry("minecraft:entity_type", id.as_str())
                .map(|entry| entry.id)
        })
    }

    fn entity_tree(
        self,
        nbt: &mut Compound,
        registered: &impl Fn(&Identifier) -> Option<Identifier>,
    ) -> Option<Identifier> {
        let Tag::String(id) = nbt.get(&"id".into())? else {
            return None;
        };
        let parsed = Identifier::parse(&id.to_string_lossy()).ok()?;
        let kind = registered(&parsed)?;
        // Vanilla 26.3 EntityTypes.OP_ONLY_CUSTOM_DATA. For other namespaces the SDK lacks
        // the type's custom-data permission (sunrise-org/sunrise#6234).
        if self == Self::Ordinary
            && (kind.namespace() != "minecraft"
                || matches!(
                    kind.path(),
                    "falling_block" | "command_block_minecart" | "spawner_minecart"
                ))
        {
            return None;
        }
        nbt.insert("id".into(), Tag::String(kind.as_str().into()));
        if let Some(Tag::List(mut passengers)) = nbt.remove(&"Passengers".into()) {
            passengers.retain_mut(|tag| match tag {
                Tag::Compound(child) => self.entity_tree(child, registered).is_some(),
                _ => false,
            });
            if !passengers.is_empty() {
                nbt.insert("Passengers".into(), Tag::List(passengers));
            }
        }
        Some(kind)
    }
}

/// `BlockHelper.prepareBlockEntityData`: tagged blocks keep their saved fields; a copycat
/// exports only its material and the consumed item without its component patch.
/// Even game-master printing uses this final preparation.
pub(super) fn prepare_block(
    host: &dyn Host,
    block: BlockKey,
    kind: &str,
    data: Compound,
) -> Option<Compound> {
    let data = if block_in_tag(host.tags(), block, "create:safe_nbt") == Some(true) {
        data
    } else {
        copycat::schematic_safe_data(block, data)?
    };
    let sign = block_in_tag(host.tags(), block, "minecraft:all_signs") == Some(true);
    process_block(kind, sign, data, |tag| match host.text_from_nbt(tag) {
        Ok(Some(text)) => has_click(&text),
        Ok(None) => false,
        Err(_) => true,
    })
}

fn has_click(text: &Text) -> bool {
    // Catnip checks a component's style and its appended component tree.
    text.style.click.is_some() || text.children.iter().any(has_click)
}

fn compound(tag: Option<&Tag>) -> Option<&Compound> {
    match tag? {
        Tag::Compound(c) => Some(c),
        _ => None,
    }
}

fn list(tag: Option<&Tag>) -> &[Tag] {
    match tag {
        Some(Tag::List(tags)) => tags,
        _ => &[],
    }
}

/// Catnip runs type-specific processors before the operator-only fallback. `kind` comes from
/// the actual block entity, never from the saved `id` field.
fn process_block(
    kind: &str,
    sign: bool,
    mut data: Compound,
    clicks: impl Fn(&Tag) -> bool,
) -> Option<Compound> {
    match kind {
        "minecraft:lectern" => {
            let components = compound(data.get(&"Book".into()))
                .and_then(|book| compound(book.get(&"components".into())));
            // LecternBlockEntity uses the component, not the item id; Filterable.codec accepts
            // either a component alone or a pair of raw and filtered components.
            if components.is_some_and(|components| {
                components.iter().any(|(name, value)| {
                    Identifier::parse(&name.to_string_lossy())
                        .is_ok_and(|id| id.as_str() == "minecraft:written_book_content")
                        && compound(Some(value)).is_some_and(|written| {
                            list(written.get(&"pages".into())).iter().any(|page| {
                                clicks(page)
                                    || compound(Some(page)).is_some_and(|fields| {
                                        ["raw", "filtered"]
                                            .into_iter()
                                            .any(|key| fields.get(&key.into()).is_some_and(&clicks))
                                    })
                            })
                        })
                })
            }) {
                return None;
            }
        }
        "create:clipboard" => {
            if clipboard::schematic_text_has(&data, &clicks) {
                return None;
            }
        }
        "create:creative_crate" => {
            if let Some(Tag::Compound(filter)) = data.get_mut(&"Filter".into())
                && let Some(Tag::Compound(components)) = filter.get_mut(&"components".into())
            {
                components.retain(|name, _| {
                    Identifier::parse(&name.to_string_lossy()).is_ok_and(|id| {
                        matches!(
                            id.as_str(),
                            "minecraft:enchantments"
                                | "minecraft:potion_contents"
                                | "minecraft:damage"
                                | "minecraft:custom_name"
                        )
                    })
                });
                if components.is_empty() {
                    filter.remove(&"components".into());
                }
            }
        }
        // Catnip's SpawnerBlockEntity exception does not include trial spawners.
        "minecraft:mob_spawner" => (),
        _ if sign => {
            // SignBlockEntity selects the raw or filtered lines for the acting player.
            let click = ["front_text", "back_text"]
                .into_iter()
                .filter_map(|side| compound(data.get(&side.into())))
                .any(|side| {
                    ["messages", "filtered_messages"]
                        .into_iter()
                        .any(|lines| list(side.get(&lines.into())).iter().any(&clicks))
                });
            if click
                || ["front_item", "back_item"]
                    .into_iter()
                    .any(|key| data.contains_key(&key.into()))
            {
                return None;
            }
            // Omitting the flag would let a merge keep the existing sign's authority.
            data.insert("allow_op_features".into(), Tag::Byte(0));
        }
        // Remaining members of vanilla 26.3 BlockEntityTypes.OP_ONLY_CUSTOM_DATA.
        "minecraft:command_block"
        | "minecraft:sign"
        | "minecraft:hanging_sign"
        | "minecraft:trial_spawner" => return None,
        _ => (),
    }
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::text::TextClick;

    fn entity(id: &str, children: Vec<Tag>) -> Compound {
        Compound::from([
            ("id".into(), Tag::String(id.into())),
            ("Passengers".into(), Tag::List(children)),
        ])
    }

    fn registered(id: &Identifier) -> Option<Identifier> {
        matches!(
            id.as_str(),
            "minecraft:armor_stand" | "minecraft:falling_block" | "other:cart"
        )
        .then(|| id.clone())
    }

    #[test]
    fn unprivileged_trees_keep_safe_siblings_and_equipment() {
        let equipment = Tag::compound([(
            "mainhand",
            Tag::compound([("id", Tag::String("minecraft:stick".into()))]),
        )]);
        let mut safe = entity(
            "armor_stand",
            vec![Tag::Compound(entity(":falling_block", vec![]))],
        );
        safe.insert("equipment".into(), equipment.clone());
        let mut root = entity(
            "minecraft:armor_stand",
            vec![
                Tag::Compound(entity(
                    "falling_block",
                    vec![Tag::Compound(entity("armor_stand", vec![]))],
                )),
                Tag::Compound(safe),
                Tag::Compound(entity("other:cart", vec![])),
                Tag::Int(0),
            ],
        );
        for permission in [None, Some(false)] {
            let mut restricted = root.clone();
            assert!(
                Access::for_player(permission)
                    .entity_tree(&mut restricted, &registered)
                    .is_some()
            );
            let [Tag::Compound(safe)] = list(restricted.get(&"Passengers".into())) else {
                panic!("expected only the safe sibling")
            };
            assert_eq!(safe.get(&"equipment".into()), Some(&equipment));
            assert!(!safe.contains_key(&"Passengers".into()));
        }
        assert!(
            Access::for_player(Some(true))
                .entity_tree(&mut root, &registered)
                .is_some()
        );
        assert_eq!(list(root.get(&"Passengers".into())).len(), 3);
        assert!(
            Access::Ordinary
                .entity_tree(&mut entity("falling_block", vec![]), &registered)
                .is_none()
        );
        assert!(
            Access::GameMaster
                .entity_tree(&mut entity("unknown", vec![]), &registered)
                .is_none()
        );
    }

    fn click_fixture() -> Tag {
        Tag::compound([
            ("text", Tag::String("page".into())),
            (
                "click_event",
                Tag::compound([
                    ("action", Tag::String("change_page".into())),
                    ("page", Tag::Int(1)),
                ]),
            ),
        ])
    }

    #[test]
    fn signs_check_both_visible_forms_and_the_item_slots() {
        let click = click_fixture();
        let plain = Tag::String("The literal words click_event".into());
        for messages in ["messages", "filtered_messages"] {
            let data = |line: Tag| {
                Compound::from([(
                    "front_text".into(),
                    Tag::compound([
                        (
                            "messages",
                            Tag::List(if messages == "messages" {
                                vec![line.clone(); 4]
                            } else {
                                vec![plain.clone(); 4]
                            }),
                        ),
                        (
                            "filtered_messages",
                            Tag::List(if messages == "filtered_messages" {
                                vec![line; 4]
                            } else {
                                vec![plain.clone(); 4]
                            }),
                        ),
                    ]),
                )])
            };
            assert!(
                process_block("minecraft:sign", true, data(plain.clone()), |t| t == &click)
                    .is_some()
            );
            assert!(
                process_block("minecraft:sign", true, data(click.clone()), |t| t == &click)
                    .is_none()
            );
        }
        for key in ["front_item", "back_item"] {
            let data = Compound::from([(key.into(), Tag::Compound(Compound::new()))]);
            assert!(process_block("minecraft:sign", true, data, |_| false).is_none());
        }
        let mut child = Text::literal("a page");
        child.style.click = Some(TextClick::ChangePage(1));
        assert!(has_click(&Text::literal("header").append(child)));
        assert!(!has_click(&Text::literal("click_event")));
    }

    #[test]
    fn schematic_signs_never_keep_operator_authority() {
        let text = |line: &str| {
            Tag::compound([("messages", Tag::List(vec![Tag::String(line.into()); 4]))])
        };
        let benign = Compound::from([
            ("front_text".into(), text("Workshop")),
            ("back_text".into(), text("Storage")),
            ("is_waxed".into(), Tag::Byte(1)),
        ]);
        let mut expected = benign.clone();
        expected.insert("allow_op_features".into(), Tag::Byte(0));
        for kind in ["minecraft:sign", "minecraft:hanging_sign"] {
            let mut with_authority = benign.clone();
            with_authority.insert("allow_op_features".into(), Tag::Byte(1));
            for input in [benign.clone(), with_authority] {
                assert_eq!(
                    process_block(kind, true, input, |_| false),
                    Some(expected.clone()),
                    "{kind}"
                );
            }
        }
    }

    #[test]
    fn lectern_checks_written_component_not_item_id() {
        let click = click_fixture();
        let book = |key, page| {
            Compound::from([(
                "Book".into(),
                Tag::compound([
                    ("id", Tag::String("minecraft:writable_book".into())),
                    (
                        "components",
                        Tag::compound([(
                            key,
                            Tag::compound([
                                ("title", Tag::String("test".into())),
                                ("author", Tag::String("test".into())),
                                ("pages", Tag::List(vec![page])),
                            ]),
                        )]),
                    ),
                ]),
            )])
        };
        let writable = book(
            "minecraft:writable_book_content",
            Tag::String("ordinary".into()),
        );
        let ordinary = book(
            "minecraft:written_book_content",
            Tag::String("ordinary".into()),
        );
        for data in [writable, ordinary] {
            assert!(process_block("minecraft:lectern", false, data, |t| t == &click).is_some());
        }
        for page in [
            click.clone(),
            Tag::compound([
                ("raw", Tag::String("plain".into())),
                ("filtered", click.clone()),
            ]),
        ] {
            let written = book("written_book_content", page);
            assert!(process_block("minecraft:lectern", false, written, |t| t == &click).is_none());
        }
    }

    #[test]
    fn registered_processors_and_spawner_exception_precede_fallback() {
        let click = click_fixture();
        let clipboard = |text| {
            Compound::from([(
                "components".into(),
                Tag::compound([(
                    "create:clipboard_content",
                    Tag::compound([(
                        "pages",
                        Tag::List(vec![Tag::List(vec![Tag::compound([("text", text)])])]),
                    )]),
                )]),
            )])
        };
        assert!(
            process_block(
                "create:clipboard",
                false,
                clipboard(Tag::String("normal".into())),
                |t| t == &click
            )
            .is_some()
        );
        assert!(
            process_block("create:clipboard", false, clipboard(click.clone()), |t| t
                == &click)
            .is_none()
        );
        let data = Compound::from([("id".into(), Tag::String("minecraft:mob_spawner".into()))]);
        assert!(process_block("minecraft:command_block", false, data.clone(), |_| false).is_none());
        assert!(process_block("minecraft:trial_spawner", false, data.clone(), |_| false).is_none());
        assert_eq!(
            process_block("minecraft:mob_spawner", false, data.clone(), |_| false),
            Some(data)
        );
        let filter = |fields: &[(&str, Tag)]| {
            Compound::from([(
                "Filter".into(),
                Tag::compound([("components", Tag::compound(fields.iter().cloned()))]),
            )])
        };
        let fields = [
            ("minecraft:custom_data", Tag::Byte(1)),
            ("minecraft:damage", Tag::Int(2)),
        ];
        let clean = filter(&[("minecraft:damage", Tag::Int(2))]);
        assert_eq!(
            process_block("create:creative_crate", false, filter(&fields), |_| false),
            Some(clean)
        );
    }
}
