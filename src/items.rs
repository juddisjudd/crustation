//! The items a server will take for `give`.
//!
//! A Bedrock server running the add-on says what it actually holds, add-ons
//! and all. Nothing else can be asked, so these stand in: every vanilla item
//! of each edition, from PrismarineJS/minecraft-data (MIT), Java 26.1 and
//! Bedrock 1.26.30. Regenerate either with
//!
//! ```text
//! curl -sS https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/<v>/items.json |
//!   jq -r 'map(select(.name != "air") | {id: .name, name: .displayName}) | sort_by(.id) | .[] | tojson'
//! ```
//!
//! wrapped in an array, one entry a line, so a release shows up as a readable
//! diff rather than one changed line.
//!
//! `category` and `icon` are then merged in from mcitemgallery.com's
//! `/metadata/items-index.json`, keyed by `<id>.png`. That gallery is Java
//! only, so a Bedrock-only item has no picture and falls to `other`; a spawn
//! egg is the one case worth sorting by its name instead.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// Where the pictures come from. Fetched once per item and then kept, so this
/// is reached on a cache miss and never for a page the panel has already drawn.
pub const GALLERY: &str = "https://mcitemgallery.com";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Without a namespace, which is how both editions read a bare id.
    pub id: String,
    pub name: String,
    /// The creative tab it sits in, or `other` for anything the gallery has
    /// never seen: Bedrock-only items, Education Edition, and add-on items.
    pub category: String,
    /// The gallery version whose picture of this item is the current one. The
    /// set is incremental, so an item's png lives under the version that last
    /// changed it and nowhere else. Absent means there is no picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

static JAVA: LazyLock<Vec<Item>> = LazyLock::new(|| read(include_str!("items/java.json")));
static BEDROCK: LazyLock<Vec<Item>> = LazyLock::new(|| read(include_str!("items/bedrock.json")));

fn read(text: &str) -> Vec<Item> {
    serde_json::from_str(text).expect("the item list ships with the binary and is checked in CI")
}

/// What the panel can offer without asking the server.
pub fn catalogue(kind: &str) -> &'static [Item] {
    match kind {
        "minecraft_bedrock" => &BEDROCK,
        _ => &JAVA,
    }
}

/// What a server said it holds, given the names the catalogue already knows.
/// The game only ever reports ids, and `minecraft:` on a vanilla one is noise
/// the give command does not need.
pub fn reported(kind: &str, ids: &[String]) -> Vec<Item> {
    let known: std::collections::HashMap<&str, &Item> = catalogue(kind)
        .iter()
        .map(|one| (one.id.as_str(), one))
        .collect();
    let mut named: Vec<Item> = ids
        .iter()
        .map(|id| {
            let bare = id.strip_prefix("minecraft:").unwrap_or(id);
            match known.get(bare) {
                Some(item) => (*item).clone(),
                None => describe(id),
            }
        })
        .collect();
    named.sort_by(|a, b| a.id.cmp(&b.id));
    named.dedup_by(|a, b| a.id == b.id);
    named
}

/// An id the add-on reported, turned into something worth reading in a list.
/// `w:g_netherite_helmet` becomes `G Netherite Helmet`, which is the best
/// anything can do: a pack names its items and never says what to call them.
pub fn describe(id: &str) -> Item {
    let bare = id.split_once(':').map_or(id, |(_, rest)| rest);
    let name = bare
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut letters = word.chars();
            match letters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + letters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    Item {
        id: id.to_string(),
        name,
        category: "other".to_string(),
        icon: None,
    }
}

/// Where the gallery keeps this item's picture. Only ever built from an entry
/// the catalogue itself supplied, so the id is one of ours and not a path.
pub fn icon_url(item: &Item) -> Option<String> {
    item.icon
        .as_ref()
        .map(|version| format!("{GALLERY}/images/{version}/{}.png", item.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_editions_parse_and_are_not_empty() {
        assert!(catalogue("minecraft_java").len() > 1000);
        assert!(catalogue("minecraft_bedrock").len() > 1000);
    }

    #[test]
    fn an_unknown_kind_gets_the_java_list_rather_than_nothing() {
        assert_eq!(
            catalogue("something_else").len(),
            catalogue("minecraft_java").len()
        );
    }

    #[test]
    fn the_lists_hold_items_by_bare_id_and_carry_a_name() {
        let diamond = catalogue("minecraft_java")
            .iter()
            .find(|one| one.id == "diamond")
            .expect("java has diamonds");
        assert_eq!(diamond.name, "Diamond");
        assert!(
            catalogue("minecraft_bedrock")
                .iter()
                .any(|one| one.id == "totem_of_undying")
        );
    }

    #[test]
    fn air_is_not_offered_since_giving_it_is_an_error() {
        assert!(
            !catalogue("minecraft_java")
                .iter()
                .any(|one| one.id == "air")
        );
        assert!(
            !catalogue("minecraft_bedrock")
                .iter()
                .any(|one| one.id == "air")
        );
    }

    #[test]
    fn every_id_is_sorted_and_unique_so_a_picker_can_trust_the_order() {
        for kind in ["minecraft_java", "minecraft_bedrock"] {
            let ids: Vec<&str> = catalogue(kind).iter().map(|one| one.id.as_str()).collect();
            let mut sorted = ids.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(ids, sorted, "{kind} is out of order or holds a repeat");
        }
    }

    #[test]
    fn what_a_server_reports_keeps_the_catalogue_names_and_drops_the_vanilla_namespace() {
        let said = [
            "minecraft:diamond".to_string(),
            "w:g_netherite_helmet".to_string(),
            "minecraft:oak_log".to_string(),
        ];
        let out = reported("minecraft_bedrock", &said);

        assert_eq!(out[0].id, "diamond");
        assert_eq!(
            out[0].name, "Diamond",
            "the catalogue knows what to call it"
        );
        assert_eq!(out[1].id, "oak_log");
        // An add-on's own item keeps its namespace, since that is what `give`
        // has to be handed.
        assert_eq!(out[2].id, "w:g_netherite_helmet");
        assert_eq!(out[2].name, "G Netherite Helmet");
    }

    #[test]
    fn a_server_naming_the_same_item_twice_only_offers_it_once() {
        let said = [
            "minecraft:diamond".to_string(),
            "diamond".to_string(),
            "stone".to_string(),
        ];
        let out = reported("minecraft_java", &said);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "diamond");
        assert_eq!(out[1].id, "stone");
    }

    #[test]
    fn a_server_that_reported_nothing_gets_an_empty_list_rather_than_the_catalogue() {
        assert!(reported("minecraft_bedrock", &[]).is_empty());
    }

    #[test]
    fn a_namespaced_id_from_an_add_on_still_reads_as_words() {
        assert_eq!(describe("w:g_netherite_helmet").name, "G Netherite Helmet");
        assert_eq!(describe("minecraft:oak_log").name, "Oak Log");
        assert_eq!(describe("diamond").name, "Diamond");
        assert_eq!(describe("gv:villager_soldier").id, "gv:villager_soldier");
    }

    #[test]
    fn an_add_on_item_is_filed_under_other_and_has_no_picture() {
        let one = describe("w:g_netherite_helmet");
        assert_eq!(one.category, "other");
        assert_eq!(one.icon, None);
        assert_eq!(icon_url(&one), None);
    }

    #[test]
    fn every_item_names_a_category_and_most_have_a_picture() {
        for kind in ["minecraft_java", "minecraft_bedrock"] {
            let all = catalogue(kind);
            assert!(all.iter().all(|one| !one.category.is_empty()));
            let with = all.iter().filter(|one| one.icon.is_some()).count();
            // The gallery is Java's, so Bedrock is the thinner of the two.
            let floor = if kind == "minecraft_bedrock" {
                1300
            } else {
                1400
            };
            assert!(with > floor, "{kind} only has {with} pictures");
        }
    }

    #[test]
    fn a_picture_is_asked_for_by_the_version_that_last_changed_it() {
        let sword = catalogue("minecraft_java")
            .iter()
            .find(|one| one.id == "diamond_sword")
            .expect("java has a diamond sword");
        assert_eq!(sword.category, "combat");
        assert_eq!(
            icon_url(sword).as_deref(),
            Some("https://mcitemgallery.com/images/1.14.4/diamond_sword.png")
        );
    }

    #[test]
    fn a_bedrock_only_item_is_offered_without_a_picture_rather_than_left_out() {
        let chalkboard = catalogue("minecraft_bedrock")
            .iter()
            .find(|one| one.id == "chalkboard")
            .expect("bedrock has education items");
        assert_eq!(chalkboard.icon, None);
        assert_eq!(chalkboard.category, "other");
    }

    #[test]
    fn a_spawn_egg_the_gallery_never_saw_is_still_filed_with_the_spawn_eggs() {
        let eggs = catalogue("minecraft_bedrock")
            .iter()
            .filter(|one| one.id.ends_with("_spawn_egg"))
            .count();
        assert!(eggs > 80);
        assert!(
            catalogue("minecraft_bedrock")
                .iter()
                .filter(|one| one.id.ends_with("_spawn_egg"))
                .all(|one| one.category == "spawn_eggs")
        );
    }
}
