use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

/// Why a path the client asked for was refused.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("That path points outside the server folder.")]
    Outside,
    #[error("{0}")]
    Malformed(String),
}

/// Resolves a path the client asked for against one server's folder.
///
/// The check is done twice on purpose: once on the text, to throw out `..` and
/// anything absolute, and again on the real path, because a symbolic link inside
/// the folder can still point anywhere on the disk.
pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf, Refused> {
    if relative.contains('\0') {
        return Err(Refused::Malformed("That is not a path.".into()));
    }

    let mut out = root.to_path_buf();
    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return Err(Refused::Outside),
            // A Windows drive or UNC prefix would otherwise replace the root.
            part if part.contains(':') => return Err(Refused::Outside),
            part => out.push(part),
        }
    }

    let anchor = root.canonicalize().map_err(|_| Refused::Outside)?;
    // A path that does not exist yet is judged by the nearest parent that does.
    let mut probe = out.as_path();
    let real = loop {
        match probe.canonicalize() {
            Ok(found) => break found,
            Err(_) => match probe.parent() {
                Some(parent) => probe = parent,
                None => return Err(Refused::Outside),
            },
        }
    };
    if !real.starts_with(&anchor) {
        return Err(Refused::Outside);
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub name: String,
    /// `file` or `directory`.
    pub kind: &'static str,
    pub size: u64,
    pub modified: Option<String>,
}

/// Directories first, then files, each sorted by name the way a person reads.
pub async fn list(directory: &Path) -> Result<Vec<Entry>> {
    let mut entries = tokio::fs::read_dir(directory)
        .await
        .with_context(|| format!("reading {}", directory.display()))?;

    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let metadata = match entry.metadata().await {
            Ok(metadata) => metadata,
            // A file that vanished between the listing and the stat is not an error.
            Err(_) => continue,
        };
        out.push(Entry {
            name: entry.file_name().to_string_lossy().to_string(),
            kind: if metadata.is_dir() {
                "directory"
            } else {
                "file"
            },
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified: modified_at(&metadata),
        });
    }

    out.sort_by(|left, right| {
        let order = (left.kind == "file").cmp(&(right.kind == "file"));
        order.then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(out)
}

pub fn modified_at(metadata: &std::fs::Metadata) -> Option<String> {
    let time: chrono::DateTime<chrono::Utc> = metadata.modified().ok()?.into();
    Some(time.to_rfc3339())
}

/// Text the panel is willing to show in a browser. Bigger than this, or not
/// text at all, and the operator downloads it instead.
pub const MAX_EDIT_BYTES: u64 = 2 * 1024 * 1024;

/// A null byte says binary more reliably than any extension does.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|byte| *byte == 0)
}

/// Copies or moves a whole tree, used by the files screen and by imports.
pub fn copy_tree(from: &Path, into: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(from).follow_links(false) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(from)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = into.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Unpacks a zip, optionally taking only one folder from inside it.
pub fn extract_zip(archive: &Path, into: &Path, internal: &str) -> Result<()> {
    let file =
        std::fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .with_context(|| format!("reading {}", archive.display()))?;

    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        // enclosed_name refuses absolute paths and anything climbing out with '..'.
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let relative = match internal.is_empty() {
            true => path,
            false => match path.strip_prefix(internal) {
                Ok(rest) => rest.to_path_buf(),
                Err(_) => continue,
            },
        };
        if relative.as_os_str().is_empty() {
            continue;
        }

        let target = into.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&target)
            .with_context(|| format!("writing {}", target.display()))?;
        std::io::copy(&mut entry, &mut out)?;

        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode)).ok();
        }
    }
    Ok(())
}

/// Zips a folder so it can be downloaded in one piece.
pub fn zip_tree(from: &Path, out: &Path) -> Result<()> {
    let file = std::fs::File::create(out).with_context(|| format!("writing {}", out.display()))?;
    let mut writer = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for entry in walkdir::WalkDir::new(from).follow_links(false) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(from)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        // Zip entries always use forward slashes, whatever the host does.
        let name = relative.to_string_lossy().replace('\\', "/");
        if entry.file_type().is_dir() {
            writer.add_directory(name, options)?;
        } else if entry.file_type().is_file() {
            writer.start_file(name, options)?;
            let mut source = std::fs::File::open(entry.path())?;
            std::io::copy(&mut source, &mut writer)?;
        }
    }
    writer.finish()?;
    Ok(())
}

/// Refuses a name that would escape its folder or confuse the filesystem.
pub fn check_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        bail!("Give it a name.");
    }
    if trimmed == "." || trimmed == ".." {
        bail!("That name is not allowed.");
    }
    if name.contains(['/', '\\', '\0']) {
        bail!("A name cannot contain slashes.");
    }
    if Path::new(name).components().count() != 1
        || !matches!(
            Path::new(name).components().next(),
            Some(Component::Normal(_))
        )
    {
        bail!("That name is not allowed.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("crustation-files-{name}"));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("world")).expect("sandbox");
        std::fs::write(root.join("server.properties"), "a=b\n").expect("file");
        root
    }

    #[test]
    fn resolves_a_path_inside_the_folder() {
        let root = sandbox("inside");
        assert_eq!(
            resolve(&root, "server.properties").expect("allowed"),
            root.join("server.properties")
        );
        assert_eq!(resolve(&root, "").expect("allowed"), root);
        assert_eq!(resolve(&root, "/").expect("allowed"), root);
        assert_eq!(
            resolve(&root, "world/level.dat").expect("allowed"),
            root.join("world").join("level.dat")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn refuses_anything_that_climbs_out() {
        let root = sandbox("climb");
        for attempt in [
            "..",
            "../secret",
            "world/../../secret",
            "./../../etc/passwd",
            "..\\windows\\system32",
            "world\\..\\..\\secret",
        ] {
            assert!(
                resolve(&root, attempt).is_err(),
                "{attempt} should be refused"
            );
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn refuses_a_path_anchored_somewhere_else() {
        let root = sandbox("absolute");
        for attempt in ["C:/Windows", "C:\\Windows", r"\\?\C:\x"] {
            assert!(
                resolve(&root, attempt).is_err(),
                "{attempt} should be refused"
            );
        }

        // Leading slashes carry no meaning here: the root is the only anchor, so
        // these land inside it rather than being turned away.
        assert_eq!(
            resolve(&root, "/world").expect("allowed"),
            root.join("world")
        );
        assert_eq!(
            resolve(&root, "//server/share").expect("allowed"),
            root.join("server").join("share")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// The text check cannot see this one: the path looks innocent and every
    /// component is a normal name. Only canonicalising catches it.
    #[test]
    fn refuses_a_link_pointing_out_of_the_folder() {
        let root = sandbox("link");
        let outside = std::env::temp_dir().join("crustation-files-link-target");
        std::fs::create_dir_all(&outside).expect("target");
        std::fs::write(outside.join("secret.txt"), "shh").expect("secret");

        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, root.join("escape")).is_ok();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&outside, root.join("escape")).is_ok();

        if made {
            assert!(resolve(&root, "escape/secret.txt").is_err());
            assert!(resolve(&root, "escape").is_err());
        }

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn allows_a_path_that_does_not_exist_yet() {
        let root = sandbox("new");
        assert_eq!(
            resolve(&root, "world/new/deep.txt").expect("allowed"),
            root.join("world").join("new").join("deep.txt")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn spots_binary_by_its_null_bytes() {
        assert!(!looks_binary(b"plain text\nwith lines\n"));
        assert!(looks_binary(b"PK\x03\x04\x00\x00binary"));
        assert!(!looks_binary(&[]));
    }

    #[test]
    fn refuses_a_name_that_is_really_a_path() {
        assert!(check_name("world").is_ok());
        assert!(check_name("my file.txt").is_ok());
        for bad in ["", "  ", ".", "..", "a/b", "a\\b", "/etc"] {
            assert!(check_name(bad).is_err(), "{bad} should be refused");
        }
    }
}
