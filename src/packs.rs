//! Add-ons, as the game ships them.
//!
//! A `.mcpack` is a zip holding one pack, a `.mcaddon` is a zip holding several
//! (sometimes as `.mcpack` files inside it), and plenty of people distribute
//! either as a plain `.zip`. Every one of them is identified by a
//! `manifest.json`, and where it belongs on the server is decided by what that
//! manifest says its modules are. Java's equivalent is a `pack.mcmeta`.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

/// What a pack is for, which is what decides where it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    Behaviour,
    Resource,
    WorldTemplate,
    Skin,
    /// Java's own kind, which lives inside the world rather than beside it.
    Datapack,
    /// A whole world: a .mcworld on Bedrock, and a plain zip of the folder on
    /// Java, which has no extension of its own for it.
    World,
}

impl Sort {
    /// The folder the server reads this kind of pack from.
    pub fn folder(self, level: &str) -> PathBuf {
        match self {
            Sort::Behaviour => PathBuf::from("behavior_packs"),
            Sort::Resource => PathBuf::from("resource_packs"),
            Sort::WorldTemplate => PathBuf::from("world_templates"),
            Sort::Skin => PathBuf::from("skin_packs"),
            Sort::Datapack => Path::new(level).join("datapacks"),
            // Bedrock keeps its worlds together; Java keeps each beside the jar.
            Sort::World => PathBuf::from("worlds"),
        }
    }

    /// The file inside a world that says this kind of pack is switched on.
    /// Only the two Bedrock reads at world load have one.
    fn world_list(self) -> Option<&'static str> {
        match self {
            Sort::Behaviour => Some("world_behavior_packs.json"),
            Sort::Resource => Some("world_resource_packs.json"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    pub uuid: String,
    pub name: String,
    pub version: Vec<i64>,
    pub sort: Sort,
}

/// A version is `[1, 0, 0]` in a modern manifest and `"1.0.0"` in an old one.
fn version_of(value: &serde_json::Value) -> Vec<i64> {
    if let Some(list) = value.as_array() {
        return list.iter().filter_map(serde_json::Value::as_i64).collect();
    }
    if let Some(text) = value.as_str() {
        return text
            .split('.')
            .filter_map(|part| part.parse().ok())
            .collect();
    }
    Vec::new()
}

fn sort_of(module_type: &str) -> Option<Sort> {
    match module_type {
        "data" | "script" | "client_data" | "javascript" => Some(Sort::Behaviour),
        "resources" => Some(Sort::Resource),
        "world_template" => Some(Sort::WorldTemplate),
        "skin_pack" => Some(Sort::Skin),
        _ => None,
    }
}

/// Reads a Bedrock `manifest.json`. A pack whose modules say nothing the panel
/// recognises is not something it can place, so it is passed over rather than
/// guessed at.
pub fn read_manifest(text: &str) -> Option<Manifest> {
    let parsed: serde_json::Value = serde_json::from_str(text).ok()?;
    let header = parsed.get("header")?;
    let uuid = header.get("uuid")?.as_str()?.to_string();

    let sort = parsed
        .get("modules")?
        .as_array()?
        .iter()
        .filter_map(|module| module.get("type")?.as_str())
        .find_map(sort_of)?;

    // `pack.name` is a key looked up in the pack's own language file, not a
    // name, so it is no better than the file it arrived in.
    let name = header
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|found| !found.starts_with("pack.") && !found.trim().is_empty())
        .unwrap_or_default()
        .to_string();

    Some(Manifest {
        uuid,
        name,
        version: header.get("version").map(version_of).unwrap_or_default(),
        sort,
    })
}

/// Reads a Java `pack.mcmeta`, which says only that this is a pack at all.
pub fn is_datapack(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|parsed| parsed.get("pack").cloned())
        .is_some()
}

/// A folder name safe to write, derived from what the pack calls itself.
pub fn folder_name(name: &str, fallback: &str) -> String {
    let source = if name.trim().is_empty() {
        fallback
    } else {
        name
    };
    let cleaned: String = source
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "pack".to_string()
    } else {
        trimmed.chars().take(64).collect()
    }
}

/// One pack the panel put somewhere.
#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub name: String,
    pub sort: Sort,
    /// Where it landed, relative to the server folder.
    pub path: String,
    pub uuid: Option<String>,
    pub version: Vec<i64>,
    /// Whether the world was told to load it.
    pub activated: bool,
}

/// Unpacks an add-on into the folders the server reads, and tells the world to
/// load what it found.
///
/// `scratch` is somewhere to put a `.mcpack` nested inside a `.mcaddon` while it
/// is opened in turn; it must be outside the server folder so a half-done
/// install never shows up in the file browser.
pub fn install(
    archive: &Path,
    server: &Path,
    scratch: &Path,
    level: &str,
    bedrock: bool,
    activate: bool,
) -> Result<Vec<Installed>> {
    let mut found = Vec::new();
    unpack_into(
        archive, server, scratch, level, bedrock, activate, &mut found, 0,
    )?;
    if found.is_empty() {
        bail!(
            "Nothing in that file is something the panel can place. A Bedrock pack has a \
             manifest.json, a Java datapack has a pack.mcmeta, and a world has a level.dat."
        );
    }
    Ok(found)
}

#[allow(clippy::too_many_arguments)]
fn unpack_into(
    archive: &Path,
    server: &Path,
    scratch: &Path,
    level: &str,
    bedrock: bool,
    activate: bool,
    found: &mut Vec<Installed>,
    depth: u8,
) -> Result<()> {
    // A pack inside a pack inside a pack is somebody playing games, not a real
    // add-on, so the recursion stops well before anything can run away.
    if depth > 3 {
        return Ok(());
    }

    let file =
        std::fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .with_context(|| format!("{} is not a zip the panel can read", archive.display()))?;

    // Every pack root in this archive: the folder holding a manifest.json, or
    // the archive itself when the manifest sits at the top.
    let mut roots: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut nested: Vec<PathBuf> = Vec::new();
    let mut datapacks: Vec<PathBuf> = Vec::new();
    let mut worlds: Vec<PathBuf> = Vec::new();
    let mut world_names: BTreeMap<PathBuf, String> = BTreeMap::new();

    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|one| one.to_str()).unwrap_or("");
        let parent = path.parent().unwrap_or(Path::new("")).to_path_buf();

        match name {
            "manifest.json" if bedrock => {
                let mut text = String::new();
                if entry.read_to_string(&mut text).is_ok() {
                    roots.insert(parent, text);
                }
            }
            "level.dat" => worlds.push(parent),
            "levelname.txt" => {
                let mut text = String::new();
                if entry.read_to_string(&mut text).is_ok() {
                    world_names.insert(parent, text.trim().to_string());
                }
            }
            "pack.mcmeta" if !bedrock => {
                let mut text = String::new();
                if entry.read_to_string(&mut text).is_ok() && is_datapack(&text) {
                    datapacks.push(parent);
                }
            }
            _ => {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".mcpack") || lower.ends_with(".mcaddon") {
                    nested.push(path.to_path_buf());
                }
            }
        }
    }

    // A manifest below another pack's root belongs to that pack, not beside it.
    let tops: Vec<PathBuf> = roots
        .keys()
        .filter(|one| {
            !roots
                .keys()
                .any(|other| other != *one && one.starts_with(other))
        })
        .cloned()
        .collect();

    for root in tops {
        let text = roots.get(&root).cloned().unwrap_or_default();
        let Some(manifest) = read_manifest(&text) else {
            continue;
        };
        let fallback = root
            .file_name()
            .and_then(|one| one.to_str())
            .unwrap_or_else(|| stem(archive));
        let folder = folder_name(&manifest.name, fallback);
        let into = server.join(manifest.sort.folder(level)).join(&folder);

        // Replace rather than merge, so an update does not leave the old files
        // of a pack that dropped them.
        std::fs::remove_dir_all(&into).ok();
        std::fs::create_dir_all(&into)?;
        crate::files::extract_zip(archive, &into, root.to_str().unwrap_or_default())?;

        let activated = activate
            && switch_on(server, level, &manifest)
                .map_err(|error| tracing::warn!(%error, "could not switch the pack on"))
                .is_ok();

        found.push(Installed {
            name: if manifest.name.is_empty() {
                folder.clone()
            } else {
                manifest.name.clone()
            },
            sort: manifest.sort,
            path: relative(manifest.sort.folder(level).join(&folder)),
            uuid: Some(manifest.uuid),
            version: manifest.version,
            activated,
        });
    }

    // A level.dat below another world's root is that world's, not a second one.
    let tops: Vec<PathBuf> = worlds
        .iter()
        .filter(|one| {
            !worlds
                .iter()
                .any(|other| other != *one && one.starts_with(other))
        })
        .cloned()
        .collect();

    for root in tops {
        // What the world calls itself, which is what the player will look for.
        let named = world_names
            .get(&root)
            .cloned()
            .filter(|one| !one.is_empty())
            .or_else(|| {
                root.file_name()
                    .and_then(|one| one.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| stem(archive).to_string());
        let folder = folder_name(&named, stem(archive));
        // Bedrock gathers its worlds under worlds/; Java keeps each at the top.
        let at = match bedrock {
            true => server.join("worlds").join(&folder),
            false => server.join(&folder),
        };
        std::fs::remove_dir_all(&at).ok();
        std::fs::create_dir_all(&at)?;
        crate::files::extract_zip(archive, &at, root.to_str().unwrap_or_default())?;
        found.push(Installed {
            name: named,
            sort: Sort::World,
            path: match bedrock {
                true => relative(PathBuf::from("worlds").join(&folder)),
                false => folder.clone(),
            },
            uuid: None,
            version: Vec::new(),
            // Nothing is loaded until level-name points at it, which is the
            // caller's to decide.
            activated: false,
        });
    }

    for root in datapacks {
        let fallback = root
            .file_name()
            .and_then(|one| one.to_str())
            .unwrap_or_else(|| stem(archive));
        let folder = folder_name("", fallback);
        let into = server.join(Sort::Datapack.folder(level)).join(&folder);
        std::fs::remove_dir_all(&into).ok();
        std::fs::create_dir_all(&into)?;
        crate::files::extract_zip(archive, &into, root.to_str().unwrap_or_default())?;
        found.push(Installed {
            name: folder.clone(),
            sort: Sort::Datapack,
            path: relative(Sort::Datapack.folder(level).join(&folder)),
            uuid: None,
            version: Vec::new(),
            // Java loads what is in the folder; there is no list to add it to.
            activated: true,
        });
    }

    // A .mcaddon is often just a bag of .mcpack files, so open those too.
    if !nested.is_empty() {
        std::fs::create_dir_all(scratch)?;
        for inside in nested {
            let Some(name) = inside.file_name().and_then(|one| one.to_str()) else {
                continue;
            };
            let temporary = scratch.join(format!("{}-{name}", uuid::Uuid::new_v4()));
            let mut entry = match zip.by_name(&inside.to_string_lossy()) {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let mut out = std::fs::File::create(&temporary)?;
            std::io::copy(&mut entry, &mut out)?;
            drop(out);
            let result = unpack_into(
                &temporary,
                server,
                scratch,
                level,
                bedrock,
                activate,
                found,
                depth + 1,
            );
            std::fs::remove_file(&temporary).ok();
            result?;
        }
    }

    Ok(())
}

fn stem(archive: &Path) -> &str {
    archive
        .file_stem()
        .and_then(|one| one.to_str())
        .unwrap_or("pack")
}

/// Paths the interface shows use forward slashes whichever host this runs on.
fn relative(path: PathBuf) -> String {
    path.components()
        .filter_map(|part| part.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

/// Adds the pack to the list the world loads, or updates the version if it is
/// already there. Bedrock ignores a pack that is only sitting in the folder.
fn switch_on(server: &Path, level: &str, manifest: &Manifest) -> Result<()> {
    let Some(file) = manifest.sort.world_list() else {
        return Ok(());
    };
    let world = server.join("worlds").join(level);
    if !world.is_dir() {
        bail!("there is no world at {} yet", world.display());
    }
    let path = world.join(file);
    let mut rows = read_world_list(&path)?;

    let version = serde_json::json!(manifest.version);
    let already = rows.iter_mut().find(|row| {
        row.get("pack_id").and_then(serde_json::Value::as_str) == Some(manifest.uuid.as_str())
    });
    match already {
        Some(row) => row["version"] = version,
        None => rows.push(serde_json::json!({
            "pack_id": manifest.uuid,
            "version": version,
        })),
    }

    write_world_list(&path, &rows)
}

/// Reads one of a world's pack lists so a new pack can be added to it.
///
/// A file that is not there is an empty list, which is what a fresh world has.
/// A file that is there but cannot be read as a list is an error, because the
/// only other thing to do with it is write an empty list over the top, and that
/// would silently switch off every pack the world already loads.
pub fn read_world_list(path: &Path) -> Result<Vec<serde_json::Value>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", path.display()));
        }
    };
    // A world the game has never loaded can leave the file empty.
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&text).with_context(|| {
        format!(
            "{} is not a list this panel can add to. Fix or remove it, then install again; \
             nothing was changed.",
            path.display()
        )
    })
}

/// Writes a pack list beside the old one and renames over it, so a world is
/// never left reading a half-written file.
pub fn write_world_list(path: &Path, rows: &[serde_json::Value]) -> Result<()> {
    let text = serde_json::to_string_pretty(rows)?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text).with_context(|| format!("writing {}", temporary.display()))?;
    std::fs::rename(&temporary, path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// What is installed, read back out of the folders rather than from a register
/// the panel keeps, so packs somebody dropped in by hand are listed too.
pub fn installed(server: &Path, level: &str, bedrock: bool) -> Vec<Installed> {
    let kinds: &[Sort] = match bedrock {
        true => &[Sort::Behaviour, Sort::Resource],
        false => &[Sort::Datapack],
    };
    let live = active_ids(server, level);
    let mut out = Vec::new();

    for sort in kinds {
        let folder = server.join(sort.folder(level));
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let manifest = std::fs::read_to_string(entry.path().join("manifest.json"))
                .ok()
                .as_deref()
                .and_then(read_manifest);
            let uuid = manifest.as_ref().map(|one| one.uuid.clone());
            out.push(Installed {
                name: manifest
                    .as_ref()
                    .map(|one| one.name.clone())
                    .filter(|one| !one.is_empty())
                    .unwrap_or_else(|| name.clone()),
                sort: *sort,
                path: relative(sort.folder(level).join(&name)),
                activated: match &uuid {
                    Some(id) => live.contains(id),
                    None => !bedrock,
                },
                uuid,
                version: manifest.map(|one| one.version).unwrap_or_default(),
            });
        }
    }

    out.sort_by_key(|one| one.name.to_lowercase());
    out
}

/// One world on the server, as the picker offers it.
#[derive(Debug, Clone, Serialize)]
pub struct World {
    /// What the world calls itself, which is what the player looks for.
    pub name: String,
    /// The folder, which is what `level-name` and the pack lists key on.
    pub folder: String,
    /// Where it sits, relative to the server folder.
    pub path: String,
}

/// Every world the server keeps. Bedrock gathers them under `worlds/`; Java
/// keeps each beside the jar, so the top level is what is read there. A world is
/// a folder holding a `level.dat`, which is the only thing both editions agree
/// on.
pub fn worlds(server: &Path, bedrock: bool) -> Vec<World> {
    let root = match bedrock {
        true => server.join("worlds"),
        false => server.to_path_buf(),
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut out: Vec<World> = entries
        .flatten()
        .filter(|entry| entry.path().join("level.dat").is_file())
        .map(|entry| {
            let folder = entry.file_name().to_string_lossy().to_string();
            let named = std::fs::read_to_string(entry.path().join("levelname.txt"))
                .ok()
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| folder.clone());
            World {
                name: named,
                path: match bedrock {
                    true => relative(PathBuf::from("worlds").join(&folder)),
                    false => folder.clone(),
                },
                folder,
            }
        })
        .collect();

    out.sort_by_key(|one| one.name.to_lowercase());
    out
}

fn active_ids(server: &Path, level: &str) -> Vec<String> {
    let world = server.join("worlds").join(level);
    ["world_behavior_packs.json", "world_resource_packs.json"]
        .iter()
        .filter_map(|file| std::fs::read_to_string(world.join(file)).ok())
        .filter_map(|text| serde_json::from_str::<Vec<serde_json::Value>>(&text).ok())
        .flatten()
        .filter_map(|row| {
            row.get("pack_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEHAVIOUR: &str = r#"{
        "format_version": 2,
        "header": { "name": "My Pack", "uuid": "aaaa-1", "version": [1, 2, 3] },
        "modules": [{ "type": "data", "uuid": "bbbb-1", "version": [1, 2, 3] }]
    }"#;

    #[test]
    fn a_data_module_makes_it_a_behaviour_pack() {
        let found = read_manifest(BEHAVIOUR).expect("read");
        assert_eq!(found.sort, Sort::Behaviour);
        assert_eq!(found.name, "My Pack");
        assert_eq!(found.version, vec![1, 2, 3]);
        assert_eq!(found.uuid, "aaaa-1");
    }

    #[test]
    fn a_resources_module_makes_it_a_resource_pack() {
        let text = BEHAVIOUR.replace("\"type\": \"data\"", "\"type\": \"resources\"");
        assert_eq!(read_manifest(&text).expect("read").sort, Sort::Resource);
    }

    #[test]
    fn a_script_module_is_still_a_behaviour_pack() {
        let text = BEHAVIOUR.replace("\"type\": \"data\"", "\"type\": \"script\"");
        assert_eq!(read_manifest(&text).expect("read").sort, Sort::Behaviour);
    }

    #[test]
    fn a_name_that_is_only_a_lookup_key_is_no_name_at_all() {
        let text = BEHAVIOUR.replace("\"My Pack\"", "\"pack.name\"");
        assert!(read_manifest(&text).expect("read").name.is_empty());
    }

    #[test]
    fn an_old_manifest_spells_its_version_out() {
        let text = BEHAVIOUR.replace("[1, 2, 3]", "\"1.2.3\"");
        assert_eq!(read_manifest(&text).expect("read").version, vec![1, 2, 3]);
    }

    #[test]
    fn a_manifest_with_no_module_the_panel_knows_is_passed_over() {
        let text = BEHAVIOUR.replace("\"type\": \"data\"", "\"type\": \"something_else\"");
        assert!(read_manifest(&text).is_none());
    }

    #[test]
    fn rubbish_is_not_a_manifest() {
        assert!(read_manifest("not json").is_none());
        assert!(read_manifest("{}").is_none());
    }

    #[test]
    fn a_pack_mcmeta_names_a_datapack() {
        assert!(is_datapack(
            r#"{"pack":{"pack_format":48,"description":"x"}}"#
        ));
        assert!(!is_datapack(r#"{"header":{}}"#));
    }

    #[test]
    fn a_folder_name_keeps_only_what_is_safe_to_write() {
        assert_eq!(folder_name("My Pack!", "x"), "My-Pack");
        assert_eq!(folder_name("", "from-file"), "from-file");
        assert_eq!(folder_name("../../etc", "x"), "etc");
        assert_eq!(folder_name("///", "///"), "pack");
    }

    #[test]
    fn a_world_goes_under_worlds_on_bedrock() {
        assert_eq!(Sort::World.folder("anything"), PathBuf::from("worlds"));
    }

    fn sandbox(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("crustation-worlds-{name}"));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).expect("sandbox");
        root
    }

    fn world_at(at: &Path, named: Option<&str>) {
        std::fs::create_dir_all(at).expect("world");
        std::fs::write(at.join("level.dat"), [0u8; 8]).expect("level.dat");
        if let Some(named) = named {
            std::fs::write(at.join("levelname.txt"), named).expect("levelname.txt");
        }
    }

    #[test]
    fn bedrock_worlds_are_the_folders_under_worlds() {
        let root = sandbox("bedrock");
        world_at(&root.join("worlds").join("Bedrock level"), Some("My World"));
        world_at(&root.join("worlds").join("second"), None);
        // No level.dat, so not a world.
        std::fs::create_dir_all(root.join("worlds").join("notes")).expect("folder");
        // A Java-shaped world at the top is not where Bedrock keeps them.
        world_at(&root.join("elsewhere"), None);

        let found = worlds(&root, true);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "My World");
        assert_eq!(found[0].folder, "Bedrock level");
        assert_eq!(found[0].path, "worlds/Bedrock level");
        assert_eq!(found[1].name, "second");
    }

    #[test]
    fn java_worlds_sit_beside_the_jar() {
        let root = sandbox("java");
        world_at(&root.join("world"), None);
        world_at(&root.join("creative"), None);
        std::fs::create_dir_all(root.join("plugins")).expect("folder");

        let found = worlds(&root, false);
        assert_eq!(
            found
                .iter()
                .map(|one| one.folder.as_str())
                .collect::<Vec<_>>(),
            vec!["creative", "world"]
        );
        assert_eq!(found[0].path, "creative");
    }

    const OTHERS: &str = r#"[
        { "pack_id": "somebody-elses-pack", "version": [2, 0, 0] },
        { "pack_id": "one-more", "version": [1, 1, 1] }
    ]"#;

    fn manifest_of(uuid: &str, version: &str) -> Manifest {
        read_manifest(
            &BEHAVIOUR
                .replace("aaaa-1", uuid)
                .replace("[1, 2, 3]", version),
        )
        .expect("manifest")
    }

    /// The world a pack would be switched on for, with `world_behavior_packs.json`
    /// already holding whatever `existing` says.
    fn world_with(name: &str, existing: Option<&str>) -> (PathBuf, PathBuf) {
        let root = sandbox(name);
        let world = root.join("worlds").join("Bedrock level");
        std::fs::create_dir_all(&world).expect("world");
        if let Some(text) = existing {
            std::fs::write(world.join("world_behavior_packs.json"), text).expect("list");
        }
        let list = world.join("world_behavior_packs.json");
        (root, list)
    }

    #[test]
    fn switching_a_pack_on_leaves_the_ones_already_there() {
        let (root, list) = world_with("append", Some(OTHERS));
        switch_on(
            &root,
            "Bedrock level",
            &manifest_of("new-pack", "[3, 0, 0]"),
        )
        .expect("switch on");

        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&list).expect("read")).expect("parse");
        let ids: Vec<&str> = rows
            .iter()
            .map(|row| row["pack_id"].as_str().expect("id"))
            .collect();
        assert_eq!(ids, vec!["somebody-elses-pack", "one-more", "new-pack"]);
    }

    #[test]
    fn installing_the_same_pack_again_updates_its_row_rather_than_adding_a_second() {
        let (root, list) = world_with("update", Some(OTHERS));
        switch_on(
            &root,
            "Bedrock level",
            &manifest_of("one-more", "[9, 9, 9]"),
        )
        .expect("switch on");

        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&list).expect("read")).expect("parse");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["version"], serde_json::json!([9, 9, 9]));
    }

    #[test]
    fn a_world_with_no_list_yet_gets_one() {
        let (root, list) = world_with("fresh", None);
        switch_on(&root, "Bedrock level", &manifest_of("first", "[1, 0, 0]")).expect("switch on");

        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&list).expect("read")).expect("parse");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["pack_id"], "first");
    }

    #[test]
    fn a_list_the_panel_cannot_read_is_left_alone_rather_than_written_over() {
        let (root, list) = world_with("broken", Some("{ this is not a pack list"));
        let refused = switch_on(
            &root,
            "Bedrock level",
            &manifest_of("new-pack", "[1, 0, 0]"),
        );

        assert!(
            refused.is_err(),
            "a list it cannot read must not be replaced"
        );
        assert_eq!(
            std::fs::read_to_string(&list).expect("read"),
            "{ this is not a pack list"
        );
    }

    #[test]
    fn a_list_the_game_left_empty_is_an_empty_list() {
        let (root, list) = world_with("blank", Some("   \n"));
        switch_on(&root, "Bedrock level", &manifest_of("first", "[1, 0, 0]")).expect("switch on");

        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&list).expect("read")).expect("parse");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn a_server_with_nowhere_to_look_has_no_worlds() {
        let root = sandbox("empty");
        assert!(worlds(&root, true).is_empty());
    }

    #[test]
    fn each_kind_knows_where_it_belongs() {
        assert_eq!(
            Sort::Behaviour.folder("world"),
            PathBuf::from("behavior_packs")
        );
        assert_eq!(
            Sort::Resource.folder("world"),
            PathBuf::from("resource_packs")
        );
        assert_eq!(
            Sort::Datapack.folder("myworld"),
            Path::new("myworld").join("datapacks")
        );
    }
}
