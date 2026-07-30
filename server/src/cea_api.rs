use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;

pub const DEFAULT_VERSION: &str = "7.7";
const CACHE_NAMESPACE: &str = "cea-language-server/cheat-engine-api";

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct CheatEngineApiConfig {
    pub enabled: bool,
    pub version: String,
}

impl Default for CheatEngineApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            version: DEFAULT_VERSION.into(),
        }
    }
}

struct Snapshot {
    version: &'static str,
    files: &'static [(&'static str, &'static [u8])],
}

const CE_77_FILES: &[(&str, &[u8])] = &[
    (
        "manifest.json",
        include_bytes!("../../cheat-engine-api/7.7/manifest.json"),
    ),
    (
        "core.d.lua",
        include_bytes!("../../cheat-engine-api/7.7/core.d.lua"),
    ),
    (
        "memory.d.lua",
        include_bytes!("../../cheat-engine-api/7.7/memory.d.lua"),
    ),
    (
        "address-list.d.lua",
        include_bytes!("../../cheat-engine-api/7.7/address-list.d.lua"),
    ),
    (
        "mono.d.lua",
        include_bytes!("../../cheat-engine-api/7.7/mono.d.lua"),
    ),
    (
        "ui.d.lua",
        include_bytes!("../../cheat-engine-api/7.7/ui.d.lua"),
    ),
    (
        "structure.d.lua",
        include_bytes!("../../cheat-engine-api/7.7/structure.d.lua"),
    ),
];

const CE_77: Snapshot = Snapshot {
    version: DEFAULT_VERSION,
    files: CE_77_FILES,
};

pub fn supported_versions() -> &'static [&'static str] {
    &[DEFAULT_VERSION]
}

pub fn materialize(config: &CheatEngineApiConfig) -> Result<Option<PathBuf>, String> {
    if !config.enabled {
        return Ok(None);
    }
    let cache_root = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(env::temp_dir);
    materialize_at(config, &cache_root)
}

fn materialize_at(
    config: &CheatEngineApiConfig,
    cache_root: &Path,
) -> Result<Option<PathBuf>, String> {
    if !config.enabled {
        return Ok(None);
    }
    let snapshot = snapshot(&config.version)?;
    validate_snapshot(snapshot)?;
    let hash = snapshot_hash(snapshot);
    let parent = cache_root.join(CACHE_NAMESPACE);
    let target = parent.join(format!("{}-{hash:016x}", snapshot.version));
    if cache_matches(&target, snapshot) {
        return Ok(Some(target));
    }

    fs::create_dir_all(&parent)
        .map_err(|error| format!("failed to create CE API cache directory: {error}"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // A timestamp alone can collide when several installers start in the same
    // clock tick. Reserve the directory by creating it, and retry if another
    // installer happened to choose the same name.
    let mut temporary = parent.join(format!(".{}-{hash:016x}-{nonce}.tmp", snapshot.version));
    let mut attempt = 0_u32;
    loop {
        match fs::create_dir(&temporary) {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                attempt += 1;
                temporary = parent.join(format!(
                    ".{}-{hash:016x}-{nonce}-{attempt}.tmp",
                    snapshot.version
                ));
            }
            Err(error) => {
                return Err(format!(
                    "failed to create temporary CE API directory: {error}"
                ));
            }
        }
    }
    for (name, content) in snapshot.files {
        if let Err(error) = fs::write(temporary.join(name), content) {
            let _ = fs::remove_dir_all(&temporary);
            return Err(format!(
                "failed to extract CE API declaration {name}: {error}"
            ));
        }
    }
    fs::write(temporary.join(".complete"), format!("{hash:016x}\n"))
        .map_err(|error| format!("failed to finalize CE API snapshot: {error}"))?;

    match fs::rename(&temporary, &target) {
        Ok(()) => {}
        Err(_) if cache_matches(&target, snapshot) => {
            let _ = fs::remove_dir_all(&temporary);
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&temporary);
            return Err(format!(
                "failed to install CE API snapshot atomically: {error}"
            ));
        }
    }
    Ok(Some(target))
}

fn snapshot(version: &str) -> Result<&'static Snapshot, String> {
    match version {
        DEFAULT_VERSION => Ok(&CE_77),
        other => Err(format!(
            "unsupported Cheat Engine API version {other:?}; supported versions: {}",
            supported_versions().join(", ")
        )),
    }
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<(), String> {
    let manifest: serde_json::Value = serde_json::from_slice(snapshot.files[0].1)
        .map_err(|error| format!("invalid embedded CE API manifest: {error}"))?;
    if manifest["version"] != snapshot.version {
        return Err("embedded CE API manifest version does not match its snapshot".into());
    }
    for (name, content) in &snapshot.files[1..] {
        let source = std::str::from_utf8(content)
            .map_err(|error| format!("{name} is not valid UTF-8: {error}"))?;
        if !source.starts_with("---@meta\n") {
            return Err(format!("{name} must begin with @meta"));
        }
        if source.contains("require(") || source.contains("setmetatable(") {
            return Err(format!("{name} contains runtime behavior"));
        }
    }
    Ok(())
}

fn cache_matches(path: &Path, snapshot: &Snapshot) -> bool {
    snapshot.files.iter().all(|(name, expected)| {
        fs::read(path.join(name))
            .map(|content| content == *expected)
            .unwrap_or(false)
    })
}

fn snapshot_hash(snapshot: &Snapshot) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for (name, content) in snapshot.files {
        for byte in name.as_bytes().iter().chain(content.iter()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn temporary_directory(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "cea-api-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn validates_the_bundled_snapshot() {
        assert_eq!(supported_versions(), ["7.7"]);
        validate_snapshot(&CE_77).unwrap();
    }

    #[test]
    fn rejects_unsupported_versions() {
        let config = CheatEngineApiConfig {
            version: "8.0".into(),
            ..Default::default()
        };
        assert!(materialize_at(&config, Path::new("/unused"))
            .unwrap_err()
            .contains("supported versions: 7.7"));
    }

    #[test]
    fn disabled_mode_does_not_touch_the_cache() {
        let root = temporary_directory("disabled");
        let config = CheatEngineApiConfig {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(materialize_at(&config, &root).unwrap(), None);
        assert!(!root.exists());
    }

    #[test]
    fn extracts_and_reuses_a_content_addressed_snapshot() {
        let root = temporary_directory("extract");
        let config = CheatEngineApiConfig::default();
        let first = materialize_at(&config, &root).unwrap().unwrap();
        let second = materialize_at(&config, &root).unwrap().unwrap();

        assert_eq!(first, second);
        assert!(first.join("core.d.lua").is_file());
        assert!(first.join("manifest.json").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_installers_reuse_the_winning_snapshot() {
        const INSTALLERS: usize = 16;

        let root = temporary_directory("concurrent");
        let barrier = Arc::new(Barrier::new(INSTALLERS));
        let installers: Vec<_> = (0..INSTALLERS)
            .map(|_| {
                let root = root.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    materialize_at(&CheatEngineApiConfig::default(), &root)
                })
            })
            .collect();

        let paths: Vec<_> = installers
            .into_iter()
            .map(|installer| installer.join().unwrap().unwrap().unwrap())
            .collect();

        assert!(paths.windows(2).all(|pair| pair[0] == pair[1]));
        assert!(cache_matches(&paths[0], &CE_77));
        fs::remove_dir_all(root).unwrap();
    }
}
