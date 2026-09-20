use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// A `server.properties` file held line by line, so writing it back only
/// disturbs the keys that changed. Both Java and Bedrock use this format.
pub struct Properties {
    path: PathBuf,
    lines: Vec<Line>,
    crlf: bool,
}

enum Line {
    /// A comment, a blank line, or anything that is not `key=value`.
    Verbatim(String),
    Entry {
        key: String,
        value: String,
    },
}

impl Properties {
    pub async fn load_if_present(path: impl Into<PathBuf>) -> Result<Option<Self>> {
        let path = path.into();
        match tokio::fs::read_to_string(&path).await {
            Ok(text) => Ok(Some(Self::parse(path, &text))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
        }
    }

    fn parse(path: PathBuf, text: &str) -> Self {
        let crlf = text.contains("\r\n");
        let lines = text
            .lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                    return Line::Verbatim(line.to_string());
                }
                match split_entry(line) {
                    Some((key, value)) => Line::Entry { key, value },
                    None => Line::Verbatim(line.to_string()),
                }
            })
            .collect();
        Self { path, lines, crlf }
    }

    /// The value with Java's escapes resolved.
    pub fn get(&self, key: &str) -> Option<String> {
        self.lines.iter().find_map(|line| match line {
            Line::Entry { key: found, value } if found == key => Some(unescape(value)),
            _ => None,
        })
    }

    pub fn flag(&self, key: &str) -> Option<bool> {
        self.get(key).map(|value| value.trim() == "true")
    }

    pub fn number(&self, key: &str) -> Option<i64> {
        self.get(key)?.trim().parse().ok()
    }

    pub fn set(&mut self, key: &str, value: &str) {
        let escaped = escape(value);
        for line in &mut self.lines {
            if let Line::Entry { key: found, value } = line {
                if found == key {
                    *value = escaped;
                    return;
                }
            }
        }
        self.lines.push(Line::Entry {
            key: key.to_string(),
            value: escaped,
        });
    }

    pub async fn save(&self) -> Result<()> {
        let separator = if self.crlf { "\r\n" } else { "\n" };
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Verbatim(text) => out.push_str(text),
                Line::Entry { key, value } => {
                    out.push_str(&escape_key(key));
                    out.push('=');
                    out.push_str(value);
                }
            }
            out.push_str(separator);
        }

        // Write beside the file and rename, so a crash cannot leave it half written.
        let temporary = self.path.with_extension("properties.tmp");
        tokio::fs::write(&temporary, out.as_bytes())
            .await
            .with_context(|| format!("writing {}", temporary.display()))?;
        tokio::fs::rename(&temporary, &self.path)
            .await
            .with_context(|| format!("replacing {}", self.path.display()))?;
        Ok(())
    }
}

/// Splits on the first unescaped `=` or `:`, the way java.util.Properties does.
fn split_entry(line: &str) -> Option<(String, String)> {
    let mut escaped = false;
    for (index, byte) in line.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b'=' | b':' => {
                let key = unescape(line[..index].trim());
                if key.is_empty() {
                    return None;
                }
                return Some((key, line[index + 1..].trim_start().to_string()));
            }
            _ => {}
        }
    }
    None
}

fn unescape(value: &str) -> String {
    if !value.contains('\\') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let digits: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(decoded) => out.push(decoded),
                    None => {
                        out.push_str("\\u");
                        out.push_str(&digits);
                    }
                }
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) > 0x7e => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

fn escape_key(key: &str) -> String {
    escape(key).replace(':', "\\:").replace('=', "\\=")
}

/// `server.properties` inside a server folder.
pub fn path_in(directory: &Path) -> PathBuf {
    directory.join("server.properties")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(properties: &Properties) -> Vec<String> {
        properties
            .lines
            .iter()
            .map(|line| match line {
                Line::Verbatim(text) => text.clone(),
                Line::Entry { key, value } => format!("{key}={value}"),
            })
            .collect()
    }

    #[test]
    fn keeps_comments_and_changes_one_key() {
        let mut properties = Properties::parse(
            PathBuf::from("server.properties"),
            "#Minecraft server properties\nmotd=A Minecraft Server\nenable-rcon=false\n",
        );
        assert_eq!(
            properties.get("motd").as_deref(),
            Some("A Minecraft Server")
        );
        assert_eq!(properties.flag("enable-rcon"), Some(false));

        properties.set("enable-rcon", "true");
        properties.set("rcon.password", "hunter2");

        assert_eq!(
            rendered(&properties),
            vec![
                "#Minecraft server properties",
                "motd=A Minecraft Server",
                "enable-rcon=true",
                "rcon.password=hunter2",
            ]
        );
    }

    #[test]
    fn resolves_java_escapes() {
        let properties = Properties::parse(
            PathBuf::from("server.properties"),
            "motd=Caf\\u00e9 \\u00a7aserver\nlevel-name=a\\:b\n",
        );
        assert_eq!(properties.get("motd").as_deref(), Some("Café §aserver"));
        assert_eq!(properties.get("level-name").as_deref(), Some("a:b"));
    }

    #[test]
    fn reads_a_port_and_ignores_junk_lines() {
        let properties = Properties::parse(
            PathBuf::from("server.properties"),
            "rcon.port=25575\nnot an entry\n\nquery.port = 25565\n",
        );
        assert_eq!(properties.number("rcon.port"), Some(25575));
        assert_eq!(properties.number("query.port"), Some(25565));
        assert_eq!(properties.get("not an entry"), None);
    }
}
