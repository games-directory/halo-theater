//! End-to-end runs of the built program.
//!
//! No whole captured film ships with halo_api, so the film here is synthetic, built the way
//! halo_api's own runtime tests build theirs at the pinned rev: a bare v41 registry, then
//! summary packets laid out from `fixtures/runtime-summaries-v41.json`, the hand-written wire
//! cases copied unchanged from halo_api (`src/theater/fixtures/`).

use serde_json::Value;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CASES: &str = include_str!("fixtures/runtime-summaries-v41.json");

fn run(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_halo-theater"))
        .args(args)
        .output()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn json(output: &Output) -> Value {
    assert!(output.status.success(), "{}", stderr(output));
    serde_json::from_slice(&output.stdout).unwrap()
}

/// A registry chunk holding only the v41 version words, as in halo_api's runtime tests.
fn registry(major_version: u32) -> Vec<u8> {
    [major_version.to_le_bytes(), 27u32.to_le_bytes()].concat()
}

/// One 16-byte packet header (type 9: summary) and its payload.
fn packet(payload: &[u8], timestamp_us: u64) -> Vec<u8> {
    [
        9u16.to_le_bytes().as_slice(),
        &[0, 0],
        &(payload.len() as u32).to_le_bytes(),
        &timestamp_us.to_le_bytes(),
        payload,
    ]
    .concat()
}

fn put(output: &mut [u8], start: usize, data: &[u8]) {
    for (byte, value) in data.iter().enumerate() {
        for bit in 0..8 {
            let at = start + byte * 8 + bit;
            let mask = 1 << (7 - at % 8);
            if value & (1 << (7 - bit)) != 0 {
                output[at / 8] |= mask;
            } else {
                output[at / 8] &= !mask;
            }
        }
    }
}

/// A one-event summary payload for a fixture case, at the offsets halo_api's runtime tests use.
fn summary_payload(input: &Value) -> Vec<u8> {
    let (identity, tail, end) = (37, 14_963, 15_443);
    let mut payload = vec![0; (end + 32usize).div_ceil(8)];
    put(&mut payload, 0, &1u32.to_be_bytes());
    put(&mut payload, identity, &42u64.to_le_bytes());
    put(&mut payload, 101, &[0x2d, 0xc0]);
    for (index, unit) in "Nuzzles".encode_utf16().enumerate() {
        put(&mut payload, tail + index * 16, &unit.to_le_bytes());
    }
    put(
        &mut payload,
        tail + 47 * 8,
        &[input["type"].as_u64().unwrap() as u8],
    );
    put(
        &mut payload,
        tail + 48 * 8,
        &(input["ms"].as_u64().unwrap() as u32).to_be_bytes(),
    );
    put(
        &mut payload,
        tail + 55 * 8,
        &[input["flag"].as_u64().unwrap() as u8],
    );
    put(
        &mut payload,
        tail + 59 * 8,
        &[input["metadata"].as_u64().unwrap() as u8],
    );
    put(&mut payload, end, &[0, 0, 0x2e, 0xe0]);
    match input["corrupt"].as_str() {
        Some("marker") => put(&mut payload, end, &[0, 0, 0, 0]),
        Some("utf16") => put(&mut payload, tail, &0xd800u16.to_le_bytes()),
        Some("reserved") => put(&mut payload, tail + 52 * 8, &[1]),
        Some("xuid") => put(&mut payload, identity, &0u64.to_le_bytes()),
        Some("truncated") => {
            payload.pop();
        }
        _ => {}
    }
    payload
}

fn cases() -> Vec<Value> {
    let fixture: Value = serde_json::from_str(CASES).unwrap();
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["film_version"], 41);
    fixture["cases"].as_array().unwrap().clone()
}

/// One summary chunk with a packet per case.
fn summary_chunk(cases: &[Value]) -> Vec<u8> {
    cases
        .iter()
        .flat_map(|case| packet(&summary_payload(&case["input"]), 999_999_999))
        .collect()
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn decodes_every_summary_wire_case_from_a_chunk_directory() {
    let cases = cases();
    let directory = scratch("chunk-directory");
    std::fs::write(directory.join("chunk-000-type-1.bin"), registry(41)).unwrap();
    std::fs::write(
        directory.join("chunk-001-type-3.bin"),
        summary_chunk(&cases),
    )
    .unwrap();
    std::fs::write(directory.join("notes.txt"), "not a chunk").unwrap();

    let report = json(&run(&[directory.as_os_str()]));

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["tool"]["name"], "halo-theater");
    assert_eq!(report["tool"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["film"]["major_version"], 41);
    assert_eq!(report["film"]["format_version"], 27);
    assert_eq!(report["film"]["build"], Value::Null);
    assert_eq!(report["roster"]["status"], "unavailable");

    // Expected events in film order: by time, ties in packet order.
    let mut expected: Vec<(usize, &Value)> = cases
        .iter()
        .enumerate()
        .filter(|(_, case)| !case["expected"].is_null())
        .map(|(packet, case)| (packet, &case["expected"]))
        .collect();
    expected.sort_by_key(|(_, expected)| expected["time_us"].as_u64().unwrap());
    let events = report["events"].as_array().unwrap();
    assert_eq!(events.len(), expected.len());
    for (event, (packet, expected)) in events.iter().zip(&expected) {
        assert_eq!(event["timestamp_us"], expected["time_us"]);
        assert_eq!(event["kind"], expected["kind"]);
        if expected["kind"] == "medal" {
            assert_eq!(event["medal"]["name"], expected["medal"]);
            assert_eq!(event["medal"]["known"], !expected["medal"].is_null());
        } else {
            assert_eq!(event["medal"], Value::Null);
        }
        assert_eq!(event["actor"]["xuid"], 42);
        assert_eq!(event["actor"]["gamertag"], "Nuzzles");
        assert_eq!(event["actor"]["roster_link"]["status"], "missing");
        assert_eq!(event["derivation"], "guarded_v41_layout");
        assert_eq!(event["provenance"], "derived_summary");
        assert_eq!(event["packet_count_matches"], true);
        assert_eq!(event["source"]["chunk"], 1);
        assert_eq!(event["source"]["packet"], *packet);
        assert_eq!(event["source"]["record"], 0);
    }

    // Each rejected case leaves its packet declaring one event with none found.
    let coverage = &report["coverage"];
    let reports = coverage["summary_packets"].as_array().unwrap();
    assert_eq!(reports.len(), cases.len());
    for (report, case) in reports.iter().zip(&cases) {
        assert_eq!(report["declared_events"], 1);
        assert_eq!(
            report["count_matches"],
            !case["expected"].is_null(),
            "{}",
            case["name"]
        );
    }
    assert_eq!(coverage["summary_counts_match"], false);
    assert_eq!(coverage["chunks"]["registry"], 1);
    assert_eq!(coverage["chunks"]["summary"], 1);
    assert_eq!(coverage["chunks"]["replication"], 0);
    assert_eq!(coverage["transport"]["clear"], 2);
    assert_eq!(coverage["summary"]["packets"], cases.len());
    assert_eq!(coverage["summary"]["decoded"], cases.len());
    assert_eq!(coverage["summary"]["opaque"], 0);
    assert_eq!(coverage["summary"]["unframed_bytes"], 0);
    assert_eq!(coverage["records"]["derived_summary"], expected.len());
}

#[test]
fn reads_a_spectate_manifest_with_compressed_chunks() {
    let cases: Vec<Value> = cases()
        .into_iter()
        .filter(|case| !case["expected"].is_null())
        .collect();
    let directory = scratch("spectate-manifest");
    std::fs::write(directory.join("filmChunk0"), zlib(&registry(41))).unwrap();
    std::fs::write(directory.join("filmChunk1"), zlib(&summary_chunk(&cases))).unwrap();
    let chunk = |index: i32, kind: i32| {
        serde_json::json!({
            "Index": index,
            "ChunkStartTimeOffsetMilliseconds": 0,
            "DurationMilliseconds": 0,
            "ChunkSize": 0,
            "FileRelativePath": format!("/filmChunk{index}"),
            "ChunkType": kind,
        })
    };
    let manifest = serde_json::json!({
        "FilmStatusBond": 1,
        "CustomData": {
            "FilmLength": 0,
            "Chunks": [chunk(0, 1), chunk(1, 3), chunk(2, 2)],
            "HasGameEnded": true,
            "ManifestRefreshSeconds": 0,
            "MatchId": "00000000-0000-0000-0000-000000000000",
            "FilmMajorVersion": 41,
        },
        "BlobStoragePathPrefix": "https://example.invalid/films/",
        "AssetId": "00000000-0000-0000-0000-000000000000",
    });
    let manifest_path = directory.join("spectate.json");
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();

    // filmChunk2 (replication) was never saved: reading it is an error, unless replication is skipped.
    let missing = run(&[manifest_path.as_os_str()]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(stderr(&missing).contains("filmChunk2"));
    assert_eq!(stderr(&missing).lines().count(), 1);

    let report = json(&run(&[
        OsStr::new("--summaries-only"),
        OsStr::new("--pretty"),
        manifest_path.as_os_str(),
    ]));
    assert_eq!(report["events"].as_array().unwrap().len(), cases.len());
    assert_eq!(report["coverage"]["summary_counts_match"], true);
    assert_eq!(report["coverage"]["chunks"]["replication_skipped"], 1);
    assert_eq!(report["coverage"]["transport"]["zlib_complete"], 2);
}

#[test]
fn reads_film_json_in_a_directory() {
    let directory = scratch("film-json");
    std::fs::write(directory.join("registry.bin"), registry(41)).unwrap();
    std::fs::write(directory.join("summary.bin"), summary_chunk(&cases()[..1])).unwrap();
    let manifest = serde_json::json!({"chunks": [
        {"file": "registry.bin", "index": 0, "chunk_type": 1, "start_time_offset_ms": 0},
        {"file": "summary.bin", "index": 1, "chunk_type": 3, "start_time_offset_ms": 0},
    ]});
    std::fs::write(directory.join("film.json"), manifest.to_string()).unwrap();

    let report = json(&run(&[directory.as_os_str()]));
    assert_eq!(report["events"][0]["medal"]["name"], "Splatter");
}

#[test]
fn an_unsupported_film_version_fails_with_one_line() {
    let directory = scratch("unsupported-version");
    std::fs::write(directory.join("chunk-000-type-1.bin"), registry(40)).unwrap();

    let output = run(&[directory.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        stderr(&output),
        "halo-theater: unsupported film version 40; supported: 41\n"
    );
}

#[test]
fn a_film_without_a_registry_first_fails() {
    let directory = scratch("no-registry");
    std::fs::write(
        directory.join("chunk-000-type-3.bin"),
        summary_chunk(&cases()[..1]),
    )
    .unwrap();

    let output = run(&[directory.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "halo-theater: the first film chunk must be a registry chunk\n"
    );
}

#[test]
fn a_directory_without_chunks_or_a_missing_path_fails() {
    let empty = scratch("empty");
    let output = run(&[empty.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("no film.json and no chunk-<index>-type-<type>.bin files"));

    let output = run(&[empty.join("missing").as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).ends_with("no such file or directory\n"));
}

#[test]
fn usage_help_and_version() {
    let help = run(&[OsStr::new("--help")]);
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("Usage: halo-theater")
    );

    let version = run(&[OsStr::new("-V")]);
    assert!(version.status.success());
    assert!(
        String::from_utf8(version.stdout)
            .unwrap()
            .starts_with(&format!("halo-theater {}", env!("CARGO_PKG_VERSION")))
    );

    let usage_errors: [&[&str]; 3] = [&[], &["--json"], &["a", "b"]];
    for args in usage_errors {
        let output = run(&args.iter().map(OsStr::new).collect::<Vec<_>>());
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(stderr(&output).lines().count(), 1);
    }
}
