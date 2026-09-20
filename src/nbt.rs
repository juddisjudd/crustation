//! Just enough of Bedrock's NBT to read and write `level.dat`.
//!
//! Bedrock writes the same tags Java does but little-endian throughout, and
//! wraps the root compound in an eight byte header: a version, then the length
//! of what follows. Nothing here tries to be a general NBT library; it exists
//! so the panel can turn a world experiment on without asking somebody to make
//! the world in the game first.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(u8, Vec<Value>),
    /// Ordered, so writing a file back keeps a stable shape.
    Compound(BTreeMap<String, Value>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Value {
    fn tag(&self) -> u8 {
        match self {
            Value::Byte(_) => 1,
            Value::Short(_) => 2,
            Value::Int(_) => 3,
            Value::Long(_) => 4,
            Value::Float(_) => 5,
            Value::Double(_) => 6,
            Value::ByteArray(_) => 7,
            Value::String(_) => 8,
            Value::List(..) => 9,
            Value::Compound(_) => 10,
            Value::IntArray(_) => 11,
            Value::LongArray(_) => 12,
        }
    }

    pub fn as_compound_mut(&mut self) -> Option<&mut BTreeMap<String, Value>> {
        match self {
            Value::Compound(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_compound(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Compound(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_byte(&self) -> Option<i8> {
        match self {
            Value::Byte(one) => Some(*one),
            _ => None,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        if self.at + count > self.bytes.len() {
            bail!("the file ends in the middle of a tag");
        }
        let slice = &self.bytes[self.at..self.at + count];
        self.at += count;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into()?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into()?))
    }

    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into()?))
    }

    /// A string is a little-endian length and then that many bytes of UTF-8.
    fn string(&mut self) -> Result<String> {
        let length = i16::from_le_bytes(self.take(2)?.try_into()?);
        if length < 0 {
            bail!("a string cannot have a negative length");
        }
        let bytes = self.take(length as usize)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    fn payload(&mut self, tag: u8) -> Result<Value> {
        Ok(match tag {
            1 => Value::Byte(self.u8()? as i8),
            2 => Value::Short(self.i16()?),
            3 => Value::Int(self.i32()?),
            4 => Value::Long(self.i64()?),
            5 => Value::Float(f32::from_le_bytes(self.take(4)?.try_into()?)),
            6 => Value::Double(f64::from_le_bytes(self.take(8)?.try_into()?)),
            7 => {
                let count = self.i32()?.max(0) as usize;
                Value::ByteArray(self.take(count)?.iter().map(|one| *one as i8).collect())
            }
            8 => Value::String(self.string()?),
            9 => {
                let inner = self.u8()?;
                let count = self.i32()?.max(0) as usize;
                let mut items = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    items.push(self.payload(inner)?);
                }
                Value::List(inner, items)
            }
            10 => {
                let mut map = BTreeMap::new();
                loop {
                    let next = self.u8()?;
                    if next == 0 {
                        break;
                    }
                    let name = self.string()?;
                    map.insert(name, self.payload(next)?);
                }
                Value::Compound(map)
            }
            11 => {
                let count = self.i32()?.max(0) as usize;
                let mut items = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    items.push(self.i32()?);
                }
                Value::IntArray(items)
            }
            12 => {
                let count = self.i32()?.max(0) as usize;
                let mut items = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    items.push(self.i64()?);
                }
                Value::LongArray(items)
            }
            other => bail!("tag {other} is not one this reads"),
        })
    }
}

fn write_string(out: &mut Vec<u8>, text: &str) {
    let bytes = text.as_bytes();
    out.extend_from_slice(&(bytes.len() as i16).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn write_payload(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Byte(one) => out.push(*one as u8),
        Value::Short(one) => out.extend_from_slice(&one.to_le_bytes()),
        Value::Int(one) => out.extend_from_slice(&one.to_le_bytes()),
        Value::Long(one) => out.extend_from_slice(&one.to_le_bytes()),
        Value::Float(one) => out.extend_from_slice(&one.to_le_bytes()),
        Value::Double(one) => out.extend_from_slice(&one.to_le_bytes()),
        Value::ByteArray(items) => {
            out.extend_from_slice(&(items.len() as i32).to_le_bytes());
            out.extend(items.iter().map(|one| *one as u8));
        }
        Value::String(text) => write_string(out, text),
        Value::List(inner, items) => {
            // An empty list still has to name a type, and Byte is what Bedrock
            // writes when it has nothing better to say.
            out.push(if items.is_empty() {
                *inner
            } else {
                items[0].tag()
            });
            out.extend_from_slice(&(items.len() as i32).to_le_bytes());
            for item in items {
                write_payload(out, item);
            }
        }
        Value::Compound(map) => {
            for (name, item) in map {
                out.push(item.tag());
                write_string(out, name);
                write_payload(out, item);
            }
            out.push(0);
        }
        Value::IntArray(items) => {
            out.extend_from_slice(&(items.len() as i32).to_le_bytes());
            for one in items {
                out.extend_from_slice(&one.to_le_bytes());
            }
        }
        Value::LongArray(items) => {
            out.extend_from_slice(&(items.len() as i32).to_le_bytes());
            for one in items {
                out.extend_from_slice(&one.to_le_bytes());
            }
        }
    }
}

/// A `level.dat`, header and all, so it can be written back the way it came.
#[derive(Debug, Clone)]
pub struct LevelDat {
    pub version: i32,
    pub name: String,
    pub root: Value,
}

pub fn parse(bytes: &[u8]) -> Result<LevelDat> {
    if bytes.len() < 12 {
        bail!("that is too short to be a level.dat");
    }
    let version = i32::from_le_bytes(bytes[0..4].try_into()?);
    let length = i32::from_le_bytes(bytes[4..8].try_into()?).max(0) as usize;
    let body = bytes
        .get(8..8 + length.min(bytes.len() - 8))
        .context("the header claims more bytes than the file holds")?;

    let mut reader = Reader { bytes: body, at: 0 };
    let tag = reader.u8()?;
    if tag != 10 {
        bail!("a level.dat starts with a compound, not tag {tag}");
    }
    let name = reader.string()?;
    let root = reader.payload(10)?;
    Ok(LevelDat {
        version,
        name,
        root,
    })
}

pub fn write(level: &LevelDat) -> Vec<u8> {
    let mut body = Vec::new();
    body.push(10);
    write_string(&mut body, &level.name);
    write_payload(&mut body, &level.root);

    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(&level.version.to_le_bytes());
    out.extend_from_slice(&(body.len() as i32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// The experiments the game recognises, by the name it keeps them under. The
/// Beta APIs toggle is still called `gametest` inside the file, which is what
/// the scripting modules are gated on.
pub const BETA_APIS: &str = "gametest";

/// Switches a world experiment on or off.
///
/// Turning one on also sets the two flags the game uses to mark a world as
/// having been played with experiments, which is what makes it keep them.
pub fn set_experiment(level: &mut LevelDat, name: &str, on: bool) -> Result<()> {
    let root = level
        .root
        .as_compound_mut()
        .context("the root of a level.dat is a compound")?;

    let experiments = root
        .entry("experiments".to_string())
        .or_insert_with(|| Value::Compound(BTreeMap::new()));
    let Some(map) = experiments.as_compound_mut() else {
        bail!("experiments is not a compound in this level.dat");
    };

    map.insert(name.to_string(), Value::Byte(i8::from(on)));
    if on {
        map.insert("experiments_ever_used".to_string(), Value::Byte(1));
        map.insert("saved_with_toggled_experiments".to_string(), Value::Byte(1));
    }
    Ok(())
}

/// Whether an experiment is on, for saying so without changing anything.
pub fn experiment_on(level: &LevelDat, name: &str) -> bool {
    level
        .root
        .as_compound()
        .and_then(|root| root.get("experiments"))
        .and_then(Value::as_compound)
        .and_then(|map| map.get(name))
        .and_then(Value::as_byte)
        .is_some_and(|one| one != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LevelDat {
        let mut root = BTreeMap::new();
        root.insert("LevelName".to_string(), Value::String("My World".into()));
        root.insert("GameType".to_string(), Value::Int(0));
        root.insert("commandsEnabled".to_string(), Value::Byte(1));
        root.insert("lastPlayed".to_string(), Value::Long(1_700_000_000));
        root.insert("rainLevel".to_string(), Value::Float(0.0));
        root.insert("Difficulty".to_string(), Value::Int(2));
        LevelDat {
            version: 10,
            name: String::new(),
            root: Value::Compound(root),
        }
    }

    #[test]
    fn a_level_dat_survives_a_round_trip() {
        let before = sample();
        let bytes = write(&before);
        let after = parse(&bytes).expect("parse");
        assert_eq!(after.version, before.version);
        assert_eq!(after.name, before.name);
        assert_eq!(after.root, before.root);
    }

    #[test]
    fn the_header_length_matches_what_follows() {
        let bytes = write(&sample());
        let claimed = i32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(claimed, bytes.len() - 8);
    }

    #[test]
    fn turning_beta_apis_on_marks_the_world_as_having_used_experiments() {
        let mut level = sample();
        assert!(!experiment_on(&level, BETA_APIS));

        set_experiment(&mut level, BETA_APIS, true).expect("set");
        assert!(experiment_on(&level, BETA_APIS));

        let map = level
            .root
            .as_compound()
            .unwrap()
            .get("experiments")
            .unwrap()
            .as_compound()
            .unwrap();
        assert_eq!(map.get("experiments_ever_used").unwrap().as_byte(), Some(1));
        assert_eq!(
            map.get("saved_with_toggled_experiments").unwrap().as_byte(),
            Some(1)
        );
    }

    #[test]
    fn an_experiment_can_be_turned_off_again() {
        let mut level = sample();
        set_experiment(&mut level, BETA_APIS, true).expect("on");
        set_experiment(&mut level, BETA_APIS, false).expect("off");
        assert!(!experiment_on(&level, BETA_APIS));
    }

    #[test]
    fn a_world_that_already_has_experiments_keeps_the_others() {
        let mut level = sample();
        let mut experiments = BTreeMap::new();
        experiments.insert("data_driven_biomes".to_string(), Value::Byte(1));
        level
            .root
            .as_compound_mut()
            .unwrap()
            .insert("experiments".to_string(), Value::Compound(experiments));

        set_experiment(&mut level, BETA_APIS, true).expect("set");
        let after = parse(&write(&level)).expect("round trip");
        assert!(experiment_on(&after, BETA_APIS));
        assert!(experiment_on(&after, "data_driven_biomes"));
    }

    #[test]
    fn every_tag_survives_a_round_trip() {
        let mut root = BTreeMap::new();
        root.insert("b".into(), Value::Byte(-3));
        root.insert("s".into(), Value::Short(-300));
        root.insert("i".into(), Value::Int(-70000));
        root.insert("l".into(), Value::Long(-5_000_000_000));
        root.insert("f".into(), Value::Float(1.5));
        root.insert("d".into(), Value::Double(-2.25));
        root.insert("ba".into(), Value::ByteArray(vec![1, -1, 0]));
        root.insert("str".into(), Value::String("héllo".into()));
        root.insert(
            "list".into(),
            Value::List(3, vec![Value::Int(1), Value::Int(2)]),
        );
        root.insert("empty".into(), Value::List(1, Vec::new()));
        root.insert("ia".into(), Value::IntArray(vec![7, 8]));
        root.insert("la".into(), Value::LongArray(vec![9, 10]));
        let mut inner = BTreeMap::new();
        inner.insert("nested".into(), Value::Byte(1));
        root.insert("compound".into(), Value::Compound(inner));

        let level = LevelDat {
            version: 10,
            name: String::new(),
            root: Value::Compound(root),
        };
        assert_eq!(parse(&write(&level)).expect("parse").root, level.root);
    }

    #[test]
    fn rubbish_is_refused_rather_than_guessed_at() {
        assert!(parse(&[0, 1, 2]).is_err());
        assert!(parse(&[0; 64]).is_err());
    }
}
