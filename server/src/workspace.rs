use std::{collections::HashMap, fs, path::Path};

use tower_lsp::lsp_types::{Url, WorkspaceFolder};

use crate::document::Document;

const MAX_FILES_PER_FOLDER: usize = 10_000;
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;
const MAX_DEPTH: usize = 32;
const EXCLUDED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
];

pub fn discover(folders: &[WorkspaceFolder]) -> HashMap<Url, Document> {
    let mut documents = HashMap::new();
    for folder in folders {
        let Ok(root) = folder.uri.to_file_path() else {
            continue;
        };
        let mut remaining = MAX_FILES_PER_FOLDER;
        discover_directory(&root, 0, &mut remaining, &mut documents);
    }
    documents
}

pub fn load(uri: &Url) -> Option<Document> {
    let path = uri.to_file_path().ok()?;
    if !is_cea_file(&path) || fs::metadata(&path).ok()?.len() > MAX_FILE_SIZE {
        return None;
    }
    Document::parse(fs::read_to_string(path).ok()?).ok()
}

pub fn belongs_to(uri: &Url, folders: &[WorkspaceFolder]) -> bool {
    let Ok(path) = uri.to_file_path() else {
        return false;
    };
    folders.iter().any(|folder| {
        folder
            .uri
            .to_file_path()
            .is_ok_and(|root| path.starts_with(root))
    })
}

fn discover_directory(
    directory: &Path,
    depth: usize,
    remaining: &mut usize,
    documents: &mut HashMap<Url, Document>,
) {
    if depth > MAX_DEPTH || *remaining == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if *remaining == 0 {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if !excluded(&path) {
                discover_directory(&path, depth + 1, remaining, documents);
            }
        } else if file_type.is_file() && is_cea_file(&path) {
            *remaining -= 1;
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() > MAX_FILE_SIZE {
                continue;
            }
            let (Ok(uri), Ok(source)) = (Url::from_file_path(&path), fs::read_to_string(&path))
            else {
                continue;
            };
            if let Ok(document) = Document::parse(source) {
                documents.insert(uri, document);
            }
        }
    }
}

fn excluded(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| EXCLUDED_DIRECTORIES.contains(&name))
}

fn is_cea_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cea"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn fixture_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "cea-workspace-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn discovers_cea_files_but_skips_excluded_and_non_cea_files() {
        let root = fixture_root();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("scripts/open.cea"), "[ENABLE]\n[DISABLE]\n").unwrap();
        fs::write(root.join("scripts/readme.txt"), "ignored").unwrap();
        fs::write(root.join("target/generated.cea"), "[ENABLE]\n[DISABLE]\n").unwrap();
        let folder = WorkspaceFolder {
            uri: Url::from_directory_path(&root).unwrap(),
            name: "fixture".into(),
        };

        let documents = discover(&[folder]);

        assert_eq!(documents.len(), 1);
        assert!(documents
            .keys()
            .any(|uri| uri.path().ends_with("/scripts/open.cea")));
        fs::remove_dir_all(root).unwrap();
    }
}
