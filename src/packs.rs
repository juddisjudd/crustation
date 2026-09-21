//! Add-ons, as the game ships them.
//!
//! A `.mcpack` is a zip holding one pack, a `.mcaddon` is a zip holding several
//! (sometimes as `.mcpack` files inside it), and plenty of people distribute
//! either as a plain `.zip`. Every one of them is identified by a
//! `manifest.json`, and where it belongs on the server is decided by what that
//! manifest says its modules are. Java's equivalent is a `pack.mcmeta`.

use std::collections::BTreeMap;
use std::io::{BufRead, Read};
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
    /// The header's name as written, key or not, for `display_name` to read.
    #[serde(skip)]
    pub title: String,
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

    let title = header
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    // `pack.name` is a key looked up in the pack's own language file, not a
    // name, so it is no better than the file it arrived in.
    let name = Some(title.as_str())
        .filter(|found| !found.starts_with("pack.") && !found.trim().is_empty())
        .unwrap_or_default()
        .to_string();

    Some(Manifest {
        uuid,
        name,
        title,
        version: header.get("version").map(version_of).unwrap_or_default(),
        sort,
    })
}

/// What to call a pack on screen: the header's name, or what the pack's own
/// language file says when the header holds a key, without the game's `§`
/// codes. The folder when neither gives anything. `texts` is where to look a
/// key up; without it a key counts as no name.
pub fn display_name(title: &str, texts: Option<&Path>, folder: &str) -> String {
    let named = if title.starts_with("pack.") {
        texts
            .and_then(|pack| localized(pack, title))
            .unwrap_or_default()
    } else {
        title.to_string()
    };
    let shown = without_formatting(&named);
    if shown.is_empty() {
        folder.to_string()
    } else {
        shown
    }
}

/// Takes out the `§` codes the game reads as colour and style, which mean
/// nothing as text.
pub fn without_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(one) = chars.next() {
        if one == '§' {
            chars.next();
        } else {
            out.push(one);
        }
    }
    out.trim().to_string()
}

/// Looks a key up in `texts/en_US.lang`, or the first language the pack ships
/// when it has no English. Read a line at a time, since `pack.name` sits near
/// the top and some language files run to megabytes.
fn localized(pack: &Path, key: &str) -> Option<String> {
    let texts = pack.join("texts");
    let english = texts.join("en_US.lang");
    let file = if english.is_file() {
        english
    } else {
        let mut others: Vec<PathBuf> = std::fs::read_dir(&texts)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|one| one == "lang"))
            .collect();
        others.sort();
        others.into_iter().next()?
    };
    let reader = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    reader
        .lines()
        .map_while(std::result::Result::ok)
        .find_map(|line| lang_value(&line, key))
}

/// The value on one `.lang` line, when that line sets `key`. A line starting
/// `#` is a comment, and a tab followed by `#` ends a value early.
fn lang_value(line: &str, key: &str) -> Option<String> {
    let line = line.trim_start_matches('\u{feff}');
    if line.starts_with('#') {
        return None;
    }
    let (found, value) = line.split_once('=')?;
    if found.trim() != key {
        return None;
    }
    let value = value.split_once("\t#").map_or(value, |(kept, _)| kept);
    Some(value.trim().to_string())
}

/// Something a pack can be told to do, found by reading it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    /// One of "function", "scriptevent", "command", or "setting" for what only
    /// a player can change from inside the game.
    pub kind: &'static str,
    /// A short name, fit for a button.
    pub label: String,
    /// What to run, without a leading slash. A setting has none.
    pub command: Option<String>,
    /// What the pack says about it, where it says anything.
    pub detail: Option<String>,
}

/// How many to offer before the list stops being a help.
const MOST_SUGGESTIONS: usize = 40;

/// What a pack looks like it answers to: the functions it ships, the script
/// event ids its scripts watch for, the slash commands it registers, and the
/// settings its manifest declares.
///
/// Starting points for a note, not a promise. A script can do anything; this
/// reads the shapes packs usually take, and one that hides its own name behind
/// a variable is read only as far as it can be.
pub fn suggestions(pack: &Path) -> Vec<Suggestion> {
    let mut out = functions(pack);
    let scripts = script_text(pack);
    out.extend(script_events(&scripts));
    out.extend(slash_commands(&scripts));
    out.extend(settings(pack));
    out.dedup();
    out.truncate(MOST_SUGGESTIONS);
    out
}

/// An identifier as a person would read it: `forced_reset_chests` becomes
/// `forced reset chests`.
fn readable(name: &str) -> String {
    let plain = name.rsplit(':').next().unwrap_or(name);
    let spaced: String = plain
        .chars()
        .map(|one| if one == '_' { ' ' } else { one })
        .collect();
    spaced.trim().chars().take(48).collect()
}

/// Every file under `functions/`, except the ones `tick.json` already runs on
/// its own.
fn functions(pack: &Path) -> Vec<Suggestion> {
    let root = pack.join("functions");
    let ticking = ticking(&root);
    let mut names: Vec<String> = walkdir::WalkDir::new(&root)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let rest = entry.path().strip_prefix(&root).ok()?;
            let parts: Vec<&str> = rest
                .components()
                .filter_map(|part| part.as_os_str().to_str())
                .collect();
            // The game names a function with a slash, whichever platform wrote it.
            let joined = parts.join("/");
            Some(joined.strip_suffix(".mcfunction")?.to_string())
        })
        .filter(|name| !ticking.contains(name))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| Suggestion {
            kind: "function",
            label: readable(name.rsplit('/').next().unwrap_or(&name)),
            command: Some(format!("function {name}")),
            detail: None,
        })
        .collect()
}

/// The functions a pack runs itself every tick, which nobody needs a button for.
fn ticking(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join("tick.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|parsed| parsed.get("values").cloned())
        .and_then(|values| serde_json::from_value::<Vec<String>>(values).ok())
        .unwrap_or_default()
}

/// Every script in the pack, read as one. A bundler puts a pack's whole mind
/// in one file, so there is rarely more than one to read.
fn script_text(pack: &Path) -> String {
    const MOST: usize = 8 * 1024 * 1024;
    let mut out = String::new();
    for entry in walkdir::WalkDir::new(pack.join("scripts"))
        .into_iter()
        .flatten()
    {
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|one| one != "js") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            out.push_str(&text);
            out.push('\n');
        }
        if out.len() > MOST {
            break;
        }
    }
    out
}

/// True for the `namespace:name` shape an id has to have. Vanilla's own
/// namespace is not a pack's to answer for.
fn namespaced(value: &str) -> bool {
    let Some((namespace, name)) = value.split_once(':') else {
        return false;
    };
    let usable = |part: &str| {
        !part.is_empty()
            && part.len() < 64
            && part
                .chars()
                .all(|one| one.is_ascii_alphanumeric() || one == '_' || one == '-' || one == '.')
    };
    namespace != "minecraft" && usable(namespace) && usable(name)
}

/// The string starting at `at`, if a quote is what starts there.
fn literal_at(text: &str, at: usize) -> Option<String> {
    let rest = text.get(at..)?;
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = rest.get(quote.len_utf8()..)?;
    let end = body.find(quote)?;
    Some(body[..end].to_string())
}

/// Where the value after a key and its colon begins, when that key is the
/// nearest one there is.
fn value_after(window: &str, key: &str) -> Option<usize> {
    let at = window.find(key)? + key.len();
    let rest = window.get(at..)?;
    let colon = rest.find(':')?;
    // A key and its colon sit together; anything between is a different key.
    if !rest[..colon].trim_matches(['"', '\'', ' ']).is_empty() {
        return None;
    }
    let after = rest.get(colon + 1..)?;
    Some(at + colon + 1 + (after.len() - after.trim_start().len()))
}

/// The script event ids a script watches for. Read only where the script says
/// it listens at all, since an `.id` is a common enough thing to compare.
fn script_events(text: &str) -> Vec<Suggestion> {
    if !text.contains("scriptEventReceive") {
        return Vec::new();
    }

    let mut ids: Vec<String> = Vec::new();
    let mut push = |value: String| {
        if namespaced(&value) && !ids.contains(&value) {
            ids.push(value);
        }
    };

    // An id compared where it is used, and the constant standing in for one.
    for (at, _) in text.match_indices(".id") {
        let Some(rest) = text.get(at + 3..) else {
            continue;
        };
        let Some(equals) = rest.find("==") else {
            continue;
        };
        if equals > 4 || !rest[..equals].trim().is_empty() {
            continue;
        }
        let after = &rest[equals..];
        let start = after.len() - after.trim_start_matches(['=', ' ']).len();
        match literal_at(after, start) {
            Some(found) => push(found),
            None => {
                let name: String = after[start..]
                    .chars()
                    .take_while(|one| one.is_alphanumeric() || *one == '_' || *one == '$')
                    .collect();
                if let Some(found) = declared(text, &name) {
                    push(found);
                }
            }
        }
    }

    // A switch over the id is the other way a script sorts them.
    for (at, _) in text.match_indices("case ") {
        if let Some(found) = literal_at(text, at + 5) {
            push(found);
        }
    }

    ids.into_iter()
        .map(|id| Suggestion {
            kind: "scriptevent",
            label: readable(&id),
            command: Some(format!("scriptevent {id}")),
            detail: None,
        })
        .collect()
}

/// What a constant holds, for a script that names its ids rather than writing
/// them where they are used.
fn declared(text: &str, name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    for (at, _) in text.match_indices(name) {
        let before = text[..at].chars().next_back();
        if before.is_some_and(|one| one.is_alphanumeric() || one == '_' || one == '$') {
            continue;
        }
        let Some(rest) = text.get(at + name.len()..) else {
            continue;
        };
        let Some(after) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        if after.starts_with('=') {
            continue;
        }
        if let Some(found) = literal_at(after.trim_start(), 0) {
            return Some(found);
        }
    }
    None
}

/// The slash commands a script registers. A pack that loops over its own
/// aliases hides the names behind a variable, so the strings just before the
/// call are read too.
fn slash_commands(text: &str) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();

    for (at, _) in text.match_indices("registerCommand") {
        let window = &text[at..text.len().min(at + 400)];
        let detail = value_after(window, "description")
            .and_then(|start| literal_at(window, start))
            .filter(|one| !one.is_empty());

        let mut names: Vec<String> = Vec::new();
        if let Some(found) = value_after(window, "name").and_then(|start| literal_at(window, start))
            && namespaced(&found)
        {
            names.push(found);
        }
        if names.is_empty() {
            let back = &text[at.saturating_sub(300)..at];
            let mut from = 0;
            while let Some(next) = back[from..].find(['"', '\'']) {
                let quote = from + next;
                match literal_at(back, quote) {
                    Some(found) => {
                        from = quote + found.len() + 2;
                        if namespaced(&found) && !names.contains(&found) {
                            names.push(found);
                        }
                    }
                    None => from = quote + 1,
                }
            }
        }

        if names.is_empty() {
            // Worth saying a pack has one even when its name cannot be read.
            if let Some(detail) = detail.clone() {
                out.push(Suggestion {
                    kind: "command",
                    label: readable(&detail),
                    command: None,
                    detail: Some(detail),
                });
            }
            continue;
        }
        for name in names {
            out.push(Suggestion {
                kind: "command",
                label: readable(&name),
                command: Some(name),
                detail: detail.clone(),
            });
        }
    }

    out.dedup();
    out
}

/// The settings a manifest declares, which are the world's to change rather
/// than anything a console can run.
fn settings(pack: &Path) -> Vec<Suggestion> {
    let Ok(text) = std::fs::read_to_string(pack.join("manifest.json")) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    parsed
        .get("settings")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter(|row| row.get("type").and_then(serde_json::Value::as_str) != Some("label"))
                .filter_map(|row| row.get("text").and_then(serde_json::Value::as_str))
                .map(|shown| {
                    let named = match shown.contains(' ') {
                        true => shown.to_string(),
                        false => localized(pack, shown).unwrap_or_else(|| shown.to_string()),
                    };
                    Suggestion {
                        kind: "setting",
                        label: without_formatting(&named),
                        command: None,
                        detail: None,
                    }
                })
                .filter(|one| !one.label.is_empty())
                .collect()
        })
        .unwrap_or_default()
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

/// The folder a pack's files should go in, under its kind's directory.
///
/// Replacing what is there is right when it holds the same pack, since that is
/// an update. It is wrong when it holds a different one: two packs that happen
/// to call themselves the same thing would take turns deleting each other, and
/// because both are still written into the world's list the server would start
/// up complaining that a configured pack was not found. A second pack with a
/// name already taken gets a folder of its own, marked with its own id.
fn destination(
    server: &Path,
    sort: Sort,
    level: &str,
    folder: &str,
    uuid: &str,
    placed: &[Installed],
) -> String {
    let mut candidate = folder.to_string();
    let mut attempt = 0;

    loop {
        let at = relative(sort.folder(level).join(&candidate));
        let holder = placed
            .iter()
            .find(|one| one.path == at)
            .map(|one| one.uuid.clone())
            .unwrap_or_else(|| pack_at(&server.join(sort.folder(level)).join(&candidate)));

        match holder {
            Some(found) if found != uuid => {
                attempt += 1;
                let short: String = uuid
                    .chars()
                    .filter(char::is_ascii_alphanumeric)
                    .take(8)
                    .collect();
                candidate = match attempt {
                    1 => format!("{folder}-{short}"),
                    more => format!("{folder}-{short}-{more}"),
                };
            }
            _ => return candidate,
        }
    }
}

/// Which pack a folder already holds, if it holds one the panel can read.
fn pack_at(folder: &Path) -> Option<String> {
    let text = std::fs::read_to_string(folder.join("manifest.json")).ok()?;
    read_manifest(&text).map(|one| one.uuid)
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
    /// Whether this is one the server came with rather than one anybody chose.
    pub stock: bool,
}

/// Whether a pack folder is one Bedrock ships with itself.
///
/// A dedicated server unpacks dozens of them — chemistry once per game version,
/// the vanilla pack, the editor, the script libraries — and nobody installed
/// any of them, so a list that shows them buries the one pack somebody actually
/// added. Judged by the folder name, which is Mojang's to choose and stable:
/// the version suffix comes off, and what is left is matched against the names
/// the server ships under.
pub fn is_stock(folder: &str) -> bool {
    const EXACT: [&str; 5] = [
        "vanilla",
        "chemistry",
        "editor",
        "server_library",
        "server_ui_library",
    ];
    const STARTS: [&str; 4] = ["vanilla_", "chemistry_", "experimental_", "editor_"];

    let lower = folder.to_ascii_lowercase();
    // `chemistry_1.20.50` is the 1.20.50 copy of `chemistry`, not its own pack.
    let stem = lower
        .rsplit_once('_')
        .filter(|(_, tail)| {
            !tail.is_empty()
                && tail.chars().all(|one| one.is_ascii_digit() || one == '.')
                && tail.contains(|one: char| one.is_ascii_digit())
        })
        .map(|(head, _)| head)
        .unwrap_or(lower.as_str());

    EXACT.contains(&stem) || STARTS.iter().any(|one| stem.starts_with(one))
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
        let folder = destination(
            server,
            manifest.sort,
            level,
            &folder_name(&manifest.name, fallback),
            &manifest.uuid,
            found,
        );
        let into = server.join(manifest.sort.folder(level)).join(&folder);

        // Replace rather than merge, so an update does not leave the old files
        // of a pack that dropped them. `destination` has already made sure this
        // folder is either free or the same pack's.
        std::fs::remove_dir_all(&into).ok();
        std::fs::create_dir_all(&into)?;
        crate::files::extract_zip(archive, &into, root.to_str().unwrap_or_default())?;

        let activated = activate
            && switch_on(server, level, &manifest)
                .map_err(|error| tracing::warn!(%error, "could not switch the pack on"))
                .is_ok();

        found.push(Installed {
            name: display_name(&manifest.title, Some(&into), &folder),
            sort: manifest.sort,
            path: relative(manifest.sort.folder(level).join(&folder)),
            uuid: Some(manifest.uuid),
            version: manifest.version,
            activated,
            // Nothing arriving through here came with the server.
            stock: false,
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
            stock: false,
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
            stock: false,
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
            let stock = bedrock && is_stock(&name);
            let path = entry.path();
            out.push(Installed {
                // Every stock pack would read "Vanilla Resource Pack"; the
                // folder says which one it is.
                name: display_name(
                    manifest.as_ref().map_or("", |one| one.title.as_str()),
                    (!stock).then_some(path.as_path()),
                    &name,
                ),
                sort: *sort,
                path: relative(sort.folder(level).join(&name)),
                activated: match &uuid {
                    Some(id) => live.contains(id),
                    None => !bedrock,
                },
                uuid,
                version: manifest.map(|one| one.version).unwrap_or_default(),
                stock,
            });
        }
    }

    // What somebody added first, then the several dozen the server came with.
    out.sort_by(|left, right| {
        left.stock
            .cmp(&right.stock)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    out
}

/// A pack a world is told to load that is not on the disk.
///
/// This is what the server means when it says, once, at startup, that a
/// configured pack was not found and was ignored. The id is all it gives you,
/// and the panel is the only thing holding enough to make sense of it.
#[derive(Debug, Clone, Serialize)]
pub struct Missing {
    pub uuid: String,
    pub version: Vec<i64>,
    /// Which of the two lists names it.
    pub sort: Sort,
}

/// Packs a world names that are not installed. Read from the world's own lists,
/// so it says exactly what the server will complain about on its next start.
pub fn missing(server: &Path, level: &str) -> Vec<Missing> {
    let here: Vec<String> = [Sort::Behaviour, Sort::Resource]
        .iter()
        .flat_map(|sort| {
            std::fs::read_dir(server.join(sort.folder(level)))
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|entry| pack_at(&entry.path()))
        })
        .collect();

    let world = server.join("worlds").join(level);
    let mut out = Vec::new();

    for sort in [Sort::Behaviour, Sort::Resource] {
        let Some(file) = sort.world_list() else {
            continue;
        };
        let Ok(rows) = read_world_list(&world.join(file)) else {
            continue;
        };
        for row in rows {
            let Some(uuid) = row.get("pack_id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if here.iter().any(|one| one == uuid) {
                continue;
            }
            out.push(Missing {
                uuid: uuid.to_string(),
                version: row.get("version").map(version_of).unwrap_or_default(),
                sort,
            });
        }
    }
    out
}

/// Takes a pack out of a world's list, for one the world names but that is not
/// there. The pack's own folder, if it has one, is left alone.
pub fn forget(server: &Path, level: &str, uuid: &str) -> Result<bool> {
    let world = server.join("worlds").join(level);
    let mut dropped = false;

    for sort in [Sort::Behaviour, Sort::Resource] {
        let Some(file) = sort.world_list() else {
            continue;
        };
        let path = world.join(file);
        let rows = read_world_list(&path)?;
        let kept: Vec<serde_json::Value> = rows
            .into_iter()
            .filter(|row| row.get("pack_id").and_then(serde_json::Value::as_str) != Some(uuid))
            .collect();

        // Only rewrite a list that actually changed, so nothing is touched for
        // an id that was never in it.
        if kept.len() != read_world_list(&path)?.len() {
            write_world_list(&path, &kept)?;
            dropped = true;
        }
    }
    Ok(dropped)
}

/// What came of removing a pack.
#[derive(Debug, Clone, Serialize)]
pub struct Removed {
    pub uuid: Option<String>,
    /// The worlds it was taken out of, by name.
    pub worlds: Vec<String>,
    /// Worlds whose pack list could not be read, so they still name it. The
    /// caller is told rather than left to find out from the server's log.
    pub skipped: Vec<String>,
}

/// Removes an installed pack: its id from every world that names it, and then
/// its folder.
///
/// Every world, not only the one being played. A world left naming a pack whose
/// files are gone is the "configured pack was not found" the server grumbles
/// about at startup, and leaving that behind while tidying up would be a poor
/// trade.
///
/// The lists are done first on purpose. If the folder will not delete, the worst
/// left behind is a pack that is present and switched off; the other order
/// leaves a world naming a pack that is not there, which is the thing this is
/// trying to avoid. For the same reason a world whose list cannot be read is
/// reported and stepped over rather than stopping the removal: one unreadable
/// file should not make a pack impossible to get rid of.
pub fn uninstall(server: &Path, at: &Path, bedrock: bool) -> Result<Removed> {
    // Read who it is before the folder goes, since the manifest goes with it.
    let uuid = pack_at(at);
    let mut out = Removed {
        uuid: uuid.clone(),
        worlds: Vec::new(),
        skipped: Vec::new(),
    };

    // Only Bedrock keeps a list of what a world loads; Java reads its datapacks
    // folder, which removing the folder settles by itself.
    if let Some(uuid) = uuid.filter(|_| bedrock) {
        for world in worlds(server, true) {
            match forget(server, &world.folder, &uuid) {
                Ok(true) => out.worlds.push(world.name),
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(%error, world = %world.folder, "could not tidy a world's pack list");
                    out.skipped.push(world.name);
                }
            }
        }
    }

    std::fs::remove_dir_all(at).with_context(|| format!("removing {}", at.display()))?;
    Ok(out)
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

    fn pack_for(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("crustation-scan-{label}"));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("functions").join("setup")).expect("functions");
        std::fs::create_dir_all(root.join("scripts")).expect("scripts");
        root
    }

    #[test]
    fn functions_are_offered_except_the_ones_that_run_themselves() {
        let pack = pack_for("functions");
        let at = pack.join("functions");
        std::fs::write(at.join("forced_reset_truck.mcfunction"), "say hi").expect("one");
        std::fs::write(at.join("every_tick.mcfunction"), "say tick").expect("two");
        std::fs::write(at.join("setup").join("start.mcfunction"), "say start").expect("three");
        std::fs::write(at.join("tick.json"), r#"{"values":["every_tick"]}"#).expect("tick");

        let found = suggestions(&pack);
        let commands: Vec<&str> = found
            .iter()
            .filter_map(|one| one.command.as_deref())
            .collect();
        assert_eq!(
            commands,
            ["function forced_reset_truck", "function setup/start"]
        );
        assert_eq!(found[0].label, "forced reset truck");
        std::fs::remove_dir_all(&pack).ok();
    }

    #[test]
    fn script_event_ids_are_read_through_a_constant_or_a_switch() {
        let pack = pack_for("events");
        std::fs::write(
            pack.join("scripts").join("main.js"),
            r#"
            const SWITCH_ID = "pj:magnet_switch";
            system.afterEvents.scriptEventReceive.subscribe((event) => {
              if (event.id === SWITCH_ID) { flip(); }
              if (event.id === "pj:magnet_status") { report(); }
              switch (event.id) { case "pj:magnet_reset": reset(); }
              if (block.id === "minecraft:stone") { return; }
            });
            "#,
        )
        .expect("script");

        let found = suggestions(&pack);
        let ids: Vec<&str> = found
            .iter()
            .filter(|one| one.kind == "scriptevent")
            .filter_map(|one| one.command.as_deref())
            .collect();
        assert_eq!(
            ids,
            [
                "scriptevent pj:magnet_switch",
                "scriptevent pj:magnet_status",
                "scriptevent pj:magnet_reset"
            ]
        );
        std::fs::remove_dir_all(&pack).ok();
    }

    #[test]
    fn a_script_that_never_listens_offers_no_events() {
        let pack = pack_for("quiet");
        std::fs::write(
            pack.join("scripts").join("main.js"),
            r#"if (item.id === "pj:wand") { wave(); }"#,
        )
        .expect("script");
        assert!(suggestions(&pack).is_empty());
        std::fs::remove_dir_all(&pack).ok();
    }

    #[test]
    fn a_slash_command_is_read_even_when_its_names_are_a_list() {
        let pack = pack_for("commands");
        std::fs::write(
            pack.join("scripts").join("main.js"),
            r#"
            for (let t of ["ztp:health-bars","ztp:hb"]) registry.registerCommand({name:t,description:"Open Health Bars settings"}, run);
            registry.registerCommand({ name: "pj:magnet", description: "Magnet settings" }, run);
            "#,
        )
        .expect("script");

        let read = suggestions(&pack);
        let found: Vec<(&str, Option<&str>)> = read
            .iter()
            .filter(|one| one.kind == "command")
            .map(|one| (one.label.as_str(), one.command.as_deref()))
            .collect();
        assert_eq!(
            found,
            [
                ("health-bars", Some("ztp:health-bars")),
                ("hb", Some("ztp:hb")),
                ("magnet", Some("pj:magnet")),
            ]
        );
        assert_eq!(read[0].detail.as_deref(), Some("Open Health Bars settings"));
        std::fs::remove_dir_all(&pack).ok();
    }

    #[test]
    fn the_settings_a_manifest_declares_are_named_but_not_runnable() {
        let pack = pack_for("settings");
        std::fs::write(
            pack.join("manifest.json"),
            r#"{"settings":[{"type":"label","text":"Configure"},{"type":"toggle","text":"Enable item magnet"}]}"#,
        )
        .expect("manifest");

        let found = suggestions(&pack);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, "setting");
        assert_eq!(found[0].label, "Enable item magnet");
        assert!(found[0].command.is_none());
        std::fs::remove_dir_all(&pack).ok();
    }

    #[test]
    fn colour_codes_come_out_of_a_name() {
        assert_eq!(without_formatting("§aOP Toolsmith"), "OP Toolsmith");
        assert_eq!(
            without_formatting("§dMultiplayer Waypoint System §r"),
            "Multiplayer Waypoint System"
        );
        assert_eq!(without_formatting("trailing§"), "trailing");
        assert_eq!(without_formatting("Plain"), "Plain");
    }

    #[test]
    fn a_lang_line_gives_up_only_the_key_asked_for() {
        assert_eq!(
            lang_value("pack.name=Health Bars", "pack.name").as_deref(),
            Some("Health Bars")
        );
        assert_eq!(
            lang_value("pack.name=Health Bars\t## the title", "pack.name").as_deref(),
            Some("Health Bars")
        );
        assert_eq!(
            lang_value("\u{feff}pack.name=First", "pack.name").as_deref(),
            Some("First")
        );
        assert!(lang_value("## pack.name=commented", "pack.name").is_none());
        assert!(lang_value("pack.description=Other", "pack.name").is_none());
    }

    fn pack_with_lang(label: &str, lang: Option<(&str, &str)>) -> PathBuf {
        let root = std::env::temp_dir().join(format!("crustation-packs-{label}"));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("texts")).expect("texts");
        if let Some((file, body)) = lang {
            std::fs::write(root.join("texts").join(file), body).expect("lang");
        }
        root
    }

    #[test]
    fn a_key_is_looked_up_and_its_colour_taken_out() {
        let pack = pack_with_lang(
            "keyed",
            Some((
                "en_US.lang",
                "pack.name=§aHealth Bars§r\r\npack.description=x\r\n",
            )),
        );
        assert_eq!(display_name("pack.name", Some(&pack), "bp"), "Health Bars");
        std::fs::remove_dir_all(&pack).ok();
    }

    #[test]
    fn a_pack_with_no_english_is_read_in_the_language_it_has() {
        let pack = pack_with_lang("french", Some(("fr_FR.lang", "pack.name=Barres de vie\n")));
        assert_eq!(
            display_name("pack.name", Some(&pack), "bp"),
            "Barres de vie"
        );
        std::fs::remove_dir_all(&pack).ok();
    }

    #[test]
    fn a_key_nobody_answers_falls_back_to_the_folder() {
        let pack = pack_with_lang("unanswered", None);
        assert_eq!(display_name("pack.name", Some(&pack), "bp"), "bp");
        assert_eq!(
            display_name("pack.name", None, "vanilla_1.21.0"),
            "vanilla_1.21.0"
        );
        assert_eq!(display_name("", None, "folder"), "folder");
        std::fs::remove_dir_all(&pack).ok();
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

    fn placed(path: &str, uuid: &str) -> Installed {
        Installed {
            name: path.to_string(),
            sort: Sort::Behaviour,
            path: path.to_string(),
            uuid: Some(uuid.to_string()),
            version: Vec::new(),
            activated: true,
            stock: false,
        }
    }

    #[test]
    fn the_same_pack_again_goes_back_where_it_was() {
        let root = sandbox("update");
        let already = [placed("behavior_packs/Shiny", "pack-a")];
        assert_eq!(
            destination(&root, Sort::Behaviour, "w", "Shiny", "pack-a", &already),
            "Shiny"
        );
    }

    #[test]
    fn a_second_pack_of_the_same_name_does_not_land_on_the_first() {
        let root = sandbox("collide");
        let already = [placed("behavior_packs/Shiny", "pack-a")];
        let folder = destination(&root, Sort::Behaviour, "w", "Shiny", "pack-b", &already);

        assert_ne!(folder, "Shiny", "pack-b must not take pack-a's folder");
        assert!(folder.starts_with("Shiny-"), "kept the name it asked for");
    }

    #[test]
    fn a_third_pack_of_the_same_name_gets_its_own_folder_too() {
        let root = sandbox("collide-thrice");
        let already = [
            placed("behavior_packs/Shiny", "pack-a"),
            placed("behavior_packs/Shiny-packb", "pack-b"),
        ];
        let folder = destination(&root, Sort::Behaviour, "w", "Shiny", "pack-c", &already);
        assert!(!already.iter().any(|one| one.path.ends_with(&folder)));
    }

    #[test]
    fn a_folder_on_disk_holding_another_pack_is_not_taken_either() {
        let root = sandbox("on-disk");
        let at = root.join("behavior_packs").join("Shiny");
        std::fs::create_dir_all(&at).expect("folder");
        std::fs::write(
            at.join("manifest.json"),
            BEHAVIOUR.replace("aaaa-1", "already-here"),
        )
        .expect("manifest");

        let folder = destination(&root, Sort::Behaviour, "w", "Shiny", "newcomer", &[]);
        assert_ne!(folder, "Shiny", "the pack already there must survive");
    }

    #[test]
    fn a_world_naming_a_pack_that_is_not_there_says_so() {
        let root = sandbox("orphans");
        // One pack installed and listed, one listed with nothing behind it.
        let at = root.join("behavior_packs").join("Here");
        std::fs::create_dir_all(&at).expect("folder");
        std::fs::write(
            at.join("manifest.json"),
            BEHAVIOUR.replace("aaaa-1", "is-here"),
        )
        .expect("manifest");

        let world = root.join("worlds").join("w");
        std::fs::create_dir_all(&world).expect("world");
        std::fs::write(
            world.join("world_behavior_packs.json"),
            r#"[{"pack_id":"is-here","version":[1,0,0]},
                {"pack_id":"long-gone","version":[1,1,29]}]"#,
        )
        .expect("list");

        let found = missing(&root, "w");
        assert_eq!(found.len(), 1, "only the one with no pack behind it");
        assert_eq!(found[0].uuid, "long-gone");
        assert_eq!(found[0].version, vec![1, 1, 29]);
    }

    #[test]
    fn forgetting_an_entry_leaves_the_others_alone() {
        let root = sandbox("forget");
        let world = root.join("worlds").join("w");
        std::fs::create_dir_all(&world).expect("world");
        let list = world.join("world_behavior_packs.json");
        std::fs::write(
            &list,
            r#"[{"pack_id":"keep-me","version":[1,0,0]},
                {"pack_id":"long-gone","version":[1,0,0]}]"#,
        )
        .expect("list");

        assert!(forget(&root, "w", "long-gone").expect("forget"));
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&list).expect("read")).expect("parse");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["pack_id"], "keep-me");
    }

    #[test]
    fn forgetting_something_that_was_never_listed_changes_nothing() {
        let root = sandbox("forget-nothing");
        let world = root.join("worlds").join("w");
        std::fs::create_dir_all(&world).expect("world");
        std::fs::write(
            world.join("world_behavior_packs.json"),
            r#"[{"pack_id":"keep-me","version":[1,0,0]}]"#,
        )
        .expect("list");

        assert!(!forget(&root, "w", "never-there").expect("forget"));
    }

    #[test]
    fn removing_a_pack_takes_it_out_of_every_world_that_named_it() {
        let root = sandbox("uninstall");
        let at = root.join("behavior_packs").join("Shiny");
        std::fs::create_dir_all(&at).expect("folder");
        std::fs::write(
            at.join("manifest.json"),
            BEHAVIOUR.replace("aaaa-1", "going-away"),
        )
        .expect("manifest");

        // Two worlds load it, and one of them loads something else too.
        for (name, rows) in [
            (
                "Bedrock level",
                r#"[{"pack_id":"going-away","version":[1,0,0]}]"#,
            ),
            (
                "Second",
                r#"[{"pack_id":"going-away","version":[1,0,0]},
                    {"pack_id":"stays","version":[2,0,0]}]"#,
            ),
        ] {
            let world = root.join("worlds").join(name);
            world_at(&world, None);
            std::fs::write(world.join("world_behavior_packs.json"), rows).expect("list");
        }

        let gone = uninstall(&root, &at, true).expect("uninstall");
        assert_eq!(gone.uuid.as_deref(), Some("going-away"));
        assert_eq!(gone.worlds.len(), 2, "both worlds named it");
        assert!(gone.skipped.is_empty());
        assert!(!at.exists(), "the folder is gone");

        let left = std::fs::read_to_string(
            root.join("worlds")
                .join("Second")
                .join("world_behavior_packs.json"),
        )
        .expect("read");
        assert!(!left.contains("going-away"));
        assert!(left.contains("stays"), "the other pack is untouched");
    }

    #[test]
    fn a_world_with_an_unreadable_list_does_not_stop_the_removal() {
        let root = sandbox("uninstall-broken");
        let at = root.join("behavior_packs").join("Shiny");
        std::fs::create_dir_all(&at).expect("folder");
        std::fs::write(
            at.join("manifest.json"),
            BEHAVIOUR.replace("aaaa-1", "going-away"),
        )
        .expect("manifest");

        let broken = root.join("worlds").join("Broken");
        world_at(&broken, None);
        std::fs::write(broken.join("world_behavior_packs.json"), "{ truncated").expect("bad list");

        let fine = root.join("worlds").join("Fine");
        world_at(&fine, None);
        std::fs::write(
            fine.join("world_behavior_packs.json"),
            r#"[{"pack_id":"going-away","version":[1,0,0]}]"#,
        )
        .expect("list");

        let gone = uninstall(&root, &at, true).expect("the pack still goes");
        assert!(!at.exists(), "the files are removed either way");
        assert_eq!(gone.worlds, vec!["Fine"], "the readable world was tidied");
        assert_eq!(
            gone.skipped,
            vec!["Broken"],
            "and the other one is reported"
        );
        assert_eq!(
            std::fs::read_to_string(broken.join("world_behavior_packs.json")).expect("read"),
            "{ truncated",
            "an unreadable list is never written over"
        );
    }

    #[test]
    fn removing_a_pack_no_world_names_still_removes_the_files() {
        let root = sandbox("uninstall-unused");
        let at = root.join("behavior_packs").join("Lonely");
        std::fs::create_dir_all(&at).expect("folder");
        std::fs::write(at.join("manifest.json"), BEHAVIOUR).expect("manifest");

        let gone = uninstall(&root, &at, true).expect("uninstall");
        assert!(gone.worlds.is_empty());
        assert!(!at.exists());
    }

    #[test]
    fn the_packs_a_bedrock_server_unpacks_for_itself_are_stock() {
        for folder in [
            "vanilla",
            "vanilla_1.21.0",
            "vanilla_music",
            "chemistry",
            "chemistry_1.20.50",
            "chemistry_1.26.40",
            "editor",
            "server_library",
            "server_ui_library",
            "experimental_gametest",
            "VANILLA",
        ] {
            assert!(is_stock(folder), "{folder} ships with the server");
        }
    }

    #[test]
    fn a_pack_somebody_installed_is_not_stock() {
        for folder in [
            "Shiny-Behaviours",
            "my-vanilla-tweaks",
            "chemistry-plus",
            "server",
            "library",
            "Second-Add-on",
            "editors-choice",
        ] {
            assert!(!is_stock(folder), "{folder} is somebody's own");
        }
    }

    #[test]
    fn a_version_suffix_comes_off_but_a_word_does_not() {
        // `chemistry_1.20.50` is that version of chemistry.
        assert!(is_stock("chemistry_1.20.50"));
        // `server_library` is its own name, not `server` version `library`.
        assert!(is_stock("server_library"));
        assert!(!is_stock("shiny_1.0.0"));
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
