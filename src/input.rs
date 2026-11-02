//! Turns a film on disk into the ordered chunks `Film::parse` takes.
//!
//! Three layouts are read, all of them ones halo_api itself produces or reads:
//!
//! - a directory of `chunk-<index>-type-<type>.bin` files, as halo_api's `download_film` example writes them;
//! - a Halo spectate manifest (the `/hi/films/matches/{id}/spectate` response, halo_api's `FilmManifest`),
//!   with each chunk saved beside it under its `FileRelativePath` (e.g. `filmChunk0`);
//! - a `film.json` with a `chunks` list of `file`, `index`, `chunk_type` and `start_time_offset_ms`,
//!   the layout of the captured films halo_api's corpus tests read.
//!
//! Chunk bytes may be zlib-compressed or already inflated; `Film::parse` accepts both.

use halo_api::clients::hi::models::FilmManifest;
use halo_api::theater::film::{ChunkKind, FilmChunk};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The chunks read, in film order, and how many replication chunks were left out.
pub struct Input {
    pub chunks: Vec<FilmChunk>,
    pub skipped_replication: usize,
}

/// One chunk named by a manifest, before its file is read.
struct Listed {
    kind: ChunkKind,
    index: Option<i64>,
    start_ms: Option<i64>,
    file: PathBuf,
}

#[derive(Deserialize)]
struct CorpusManifest {
    chunks: Vec<CorpusChunk>,
}

#[derive(Deserialize)]
struct CorpusChunk {
    file: PathBuf,
    index: Option<i64>,
    chunk_type: i32,
    start_time_offset_ms: Option<i64>,
}

/// Read a film from a chunk directory, a directory holding `film.json`, or a manifest file.
///
/// With `summaries_only`, replication chunks (type 2) are neither read nor required to exist.
pub fn load(path: &Path, summaries_only: bool) -> Result<Input, String> {
    let listed = if path.is_dir() {
        let manifest = path.join("film.json");
        if manifest.is_file() {
            from_manifest(&manifest)?
        } else {
            from_directory(path)?
        }
    } else if path.is_file() {
        from_manifest(path)?
    } else {
        return Err(format!("{}: no such file or directory", path.display()));
    };

    let mut skipped_replication = 0;
    let mut chunks = Vec::with_capacity(listed.len());
    for chunk in listed {
        if summaries_only && chunk.kind == ChunkKind::Replication {
            skipped_replication += 1;
            continue;
        }
        let data = std::fs::read(&chunk.file)
            .map_err(|error| format!("{}: {error}", chunk.file.display()))?;
        chunks.push(FilmChunk {
            kind: chunk.kind,
            index: chunk.index,
            start_ms: chunk.start_ms,
            data,
        });
    }

    Ok(Input {
        chunks,
        skipped_replication,
    })
}

fn from_manifest(path: &Path) -> Result<Vec<Listed>, String> {
    let base = path.parent().unwrap_or(Path::new("."));
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    let invalid = |error: serde_json::Error| format!("{}: {error}", path.display());

    if value.get("CustomData").is_some() {
        let manifest: FilmManifest = serde_json::from_value(value).map_err(invalid)?;
        // Manifest order, as halo_api's client passes the downloaded chunks to Film::parse.
        return Ok(manifest
            .custom_data
            .chunks
            .iter()
            .map(|chunk| Listed {
                kind: kind(chunk.chunk_type),
                index: Some(i64::from(chunk.index)),
                start_ms: Some(chunk.start_time_offset_ms),
                file: base.join(chunk.file_relative_path.trim_start_matches('/')),
            })
            .collect());
    }

    if value.get("chunks").is_some() {
        let manifest: CorpusManifest = serde_json::from_value(value).map_err(invalid)?;
        return Ok(manifest
            .chunks
            .into_iter()
            .map(|chunk| Listed {
                kind: kind(chunk.chunk_type),
                index: chunk.index,
                start_ms: chunk.start_time_offset_ms,
                file: base.join(chunk.file),
            })
            .collect());
    }

    Err(format!(
        "{}: not a film manifest (expected a spectate response with CustomData.Chunks, or a film.json with chunks)",
        path.display()
    ))
}

fn from_directory(path: &Path) -> Result<Vec<Listed>, String> {
    let entries =
        std::fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut listed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", path.display()))?;
        let name = entry.file_name();
        let Some((index, chunk_type)) = name.to_str().and_then(chunk_name) else {
            continue;
        };
        listed.push(Listed {
            kind: kind(chunk_type),
            index: Some(index),
            start_ms: None,
            file: entry.path(),
        });
    }
    if listed.is_empty() {
        return Err(format!(
            "{}: no film.json and no chunk-<index>-type-<type>.bin files",
            path.display()
        ));
    }
    // File names carry order and kind, but no recorded start time.
    listed.sort_by_key(|chunk| chunk.index);
    Ok(listed)
}

/// `chunk-007-type-2.bin` => (7, 2)
fn chunk_name(name: &str) -> Option<(i64, i32)> {
    let (index, chunk_type) = name
        .strip_prefix("chunk-")?
        .strip_suffix(".bin")?
        .split_once("-type-")?;
    Some((index.parse().ok()?, chunk_type.parse().ok()?))
}

fn kind(chunk_type: i32) -> ChunkKind {
    ChunkKind::try_from(chunk_type).unwrap_or(ChunkKind::Unknown(chunk_type))
}

#[cfg(test)]
mod tests {
    use super::chunk_name;

    #[test]
    fn reads_index_and_type_from_chunk_file_names() {
        assert_eq!(chunk_name("chunk-000-type-1.bin"), Some((0, 1)));
        assert_eq!(chunk_name("chunk-12-type-2.bin"), Some((12, 2)));
        assert_eq!(chunk_name("chunk-001-type-3.txt"), None);
        assert_eq!(chunk_name("film.json"), None);
        assert_eq!(chunk_name("chunk-a-type-2.bin"), None);
    }
}
