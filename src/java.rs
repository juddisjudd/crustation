use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Runtime {
    pub path: String,
    pub version: String,
    /// Major version, so the interface can say "Java 21".
    pub major: Option<u32>,
}

/// Looks for Java where distributions and the Docker image put it, plus JAVA_HOME
/// and whatever is on PATH.
pub async fn discover() -> Vec<Runtime> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(home) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join("bin").join(exe("java")));
    }
    if let Some(found) = which("java") {
        candidates.push(found);
    }

    for root in [
        "/usr/lib/jvm",
        "/usr/java",
        "/opt/java",
        "C:/Program Files/Java",
        "C:/Program Files/Eclipse Adoptium",
        "C:/Program Files/Microsoft/jdk",
    ] {
        let root = Path::new(root);
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let binary = entry.path().join("bin").join(exe("java"));
            if binary.exists() {
                candidates.push(binary);
            }
        }
    }

    let mut runtimes: Vec<Runtime> = Vec::new();
    for path in candidates {
        let key = path.to_string_lossy().to_string();
        if runtimes.iter().any(|runtime| runtime.path == key) {
            continue;
        }
        if let Some(version) = version_of(&path).await {
            let major = parse_major(&version);
            runtimes.push(Runtime {
                path: key,
                version,
                major,
            });
        }
    }
    runtimes.sort_by_key(|runtime| std::cmp::Reverse(runtime.major));
    runtimes
}

async fn version_of(path: &Path) -> Option<String> {
    let output = tokio::process::Command::new(path)
        .arg("-version")
        .output()
        .await
        .ok()?;
    // java -version writes to stderr.
    let text = String::from_utf8_lossy(&output.stderr);
    text.lines().next().map(|line| line.trim().to_string())
}

fn parse_major(version: &str) -> Option<u32> {
    let quoted = version.split('"').nth(1)?;
    let first = quoted.split(['.', '_', '-']).next()?;
    match first.parse::<u32>() {
        // 1.8.0_402 style
        Ok(1) => quoted.split('.').nth(1)?.parse().ok(),
        Ok(major) => Some(major),
        Err(_) => None,
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(exe(name)))
        .find(|candidate| candidate.is_file())
}
