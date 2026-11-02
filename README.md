# halo-theater

Decode a Halo Infinite Theater film into JSON.

`halo-theater` is a small command-line program around the Theater film decoder in the Rust crate [halo_api](https://github.com/nuzzles/halo_api). It reads a film you have already downloaded, runs halo_api's parser and runtime over it, and prints what halo_api types as a typed feature or a labelled interpretation: the film's version, the roster, the summary events (kills, deaths, medals and mode events), and counts of what was decoded and what was left opaque. The decoding is all halo_api's; this program only reads the film from disk and writes JSON.

It is built for the Halo Infinite tracker on [games.directory](https://games.directory), which installs a pinned release the same way it installs [rrrocket](https://github.com/nickbabcock/rrrocket) for Rocket League replays.

> **Status.** The decoder is from halo_api's open pull request [#5, "feat: decode Theater films"](https://github.com/nuzzles/halo_api/pull/5), pinned to commit [`0aaa05b`](https://github.com/nuzzles/halo_api/tree/0aaa05bf2e6b0b905510f51d7473181bde727ec6). Only film version 41 is supported.

## What it decodes

- **Film version.** The two registry header words: the major version (41) and the format version. Also halo_api's read of the bootstrap build identity (build string, build ID, changelist, match start time) when it finds one.
- **Roster.** halo_api's bootstrap player table: each slot's film index, XUID and gamertag, and whether the table was read.
- **Summary events.** Every event from halo_api's `summary_events()`: kills, deaths, medals (by name when halo_api knows the code pair, as an unknown pair when it does not) and mode events. Each has its timestamp, the actor's XUID and gamertag as recorded in the summary, how the actor links to the roster (`unique`, `missing` or `ambiguous`), the raw summary codes, halo_api's derivation and provenance labels, and whether its summary packet declared as many events as were found.
- **Coverage.** Chunks read and how each was decompressed; the registry's stop; replication and summary packets decoded or left opaque (with halo_api's reason); bytes framing could not classify; payload bits split into fields, opaque, padding and unparsed by halo_api's source coverage; runtime records by provenance (recorded, partial, envelope, derived); and each summary packet's declared versus found event count. Together these tell a partial decode from a complete one.

## What it does not decode

halo_api is careful to separate what is decoded from what is interpreted, and this program prints nothing halo_api does not type. In particular:

- **Kills and deaths name only the recorded actor.** There is no victim, killer or weapon: halo_api does not invent them from opaque summary bodies.
- **No team.** halo_api decodes a team designator component, but does not link it to a roster player yet.
- **Mode events are a category only.** The objective action, its participants and the score are not mapped.
- **No replay data.** Positions, aim, camera, inputs, projectiles, health and shields are raw fields in halo_api, not typed features; they appear here only as coverage counts.
- **Summary events are guarded interpretations,** found by marker guards and a captured v41 layout rather than a decoded record grammar (`derivation: guarded_v41_layout`). halo_api checks them against 3,667 reference events across five films and against Halo's own match stats for eight players; matching counts are evidence of coverage, not proof of a complete grammar.

See halo_api's [COVERAGE.md](https://github.com/nuzzles/halo_api/blob/0aaa05bf2e6b0b905510f51d7473181bde727ec6/src/theater/docs/COVERAGE.md) for the full picture of what is detected and what remains, and [VALIDATION.md](https://github.com/nuzzles/halo_api/blob/0aaa05bf2e6b0b905510f51d7473181bde727ec6/src/theater/docs/VALIDATION.md) for how it is checked.

## Install

Each [release](https://github.com/games-directory/halo-theater/releases) has a `halo-theater-<version>-<target>.tar.gz` and its `.sha256` for:

| Target | For |
| --- | --- |
| `aarch64-apple-darwin` | Apple silicon Macs |
| `x86_64-apple-darwin` | Intel Macs |
| `x86_64-unknown-linux-musl` | Linux on x86-64, statically linked |

```sh
version=0.1.0 target=aarch64-apple-darwin
base=https://github.com/games-directory/halo-theater/releases/download/v$version
curl -fLO "$base/halo-theater-$version-$target.tar.gz"
curl -fLO "$base/halo-theater-$version-$target.tar.gz.sha256"
shasum -a 256 -c "halo-theater-$version-$target.tar.gz.sha256"
tar -xzf "halo-theater-$version-$target.tar.gz"
"./halo-theater-$version-$target/halo-theater" --version
```

Each archive holds one directory with the `halo-theater` program, `LICENSE`, `NOTICE` and this README.

To build from source you need Rust 1.96, which halo_api requires. `rust-toolchain.toml` pins 1.96.0, so with rustup:

```sh
cargo build --release
```

## Usage

```text
Decode a Halo Infinite Theater film (version 41) into JSON on stdout.

Usage: halo-theater [--pretty] [--summaries-only] <FILM>

<FILM> is one of:
  a directory of chunk-<index>-type-<type>.bin files (halo_api's download_film layout)
  a spectate manifest JSON (the /hi/films/matches/{id}/spectate response) with the
    chunks saved beside it under their FileRelativePath, e.g. filmChunk0
  a film.json listing chunks by file, index, chunk_type and start_time_offset_ms,
    or a directory holding one
Chunk files may be zlib-compressed as downloaded, or already inflated.

Options:
  --pretty          Indent the JSON
  --summaries-only  Skip replication chunks (type 2); the summary events are the same
  -h, --help        Print this help
  -V, --version     Print the version and the halo_api commit it is built on

Exit status: 0 decoded, 1 the film could not be read or parsed, 2 usage error.
```

### Getting a film

halo-theater does not download anything. A film is a manifest plus its chunks:

1. Fetch the spectate manifest, `GET https://discovery-infiniteugc.svc.halowaypoint.com/hi/films/matches/{match_id}/spectate`, with a Spartan token. Save the response as it is, for example as `spectate.json`.
2. Download each entry of `CustomData.Chunks` from `BlobStoragePathPrefix` + `FileRelativePath` with the same token, and save it beside the manifest under its `FileRelativePath` (`filmChunk0`, `filmChunk1`, ...). The chunks stay zlib-compressed as downloaded.
3. Run `halo-theater spectate.json`.

halo_api's [`download_film`](https://github.com/nuzzles/halo_api/blob/0aaa05bf2e6b0b905510f51d7473181bde727ec6/examples/download_film.rs) example does the downloading for you and writes a directory of `chunk-<index>-type-<type>.bin` files, which halo-theater reads as they are.

Chunk types are 1 (the registry, or bootstrap), 2 (the replication stream) and 3 (summaries). `--summaries-only` reads types 1 and 3 and does not need the type 2 chunks at all: the roster comes from the registry and the summary events from the summary chunks, so both are the same, while the replication coverage is empty and `coverage.chunks.replication_skipped` says how many chunks were left out.

### Exit status

| Status | Meaning |
| --- | --- |
| 0 | Decoded; the JSON is on stdout. A partial decode still exits 0; read `coverage`. |
| 1 | The film could not be read or parsed (missing chunk, no registry first, unsupported version, ...). One line on stderr, for example `halo-theater: unsupported film version 40; supported: 41`. |
| 2 | Usage error. One line on stderr. |
| 101 | An internal error in the decoder. One line on stderr. |

## Output

One JSON object on stdout, compact unless you pass `--pretty`. `schema_version` is 1; it goes up when a field is removed or changes meaning, not when a field is added. Names follow halo_api's where it has one.

### Fields

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | integer | Version of this output format. |
| `tool.name`, `tool.version` | string | This program and its version. |
| `tool.halo_api_rev` | string | The halo_api commit it is built on. |
| `film.major_version` | integer | First registry word. Always 41, the only version halo_api parses. |
| `film.format_version` | integer or null | Second registry word; null when the registry header is short. |
| `film.build` | object or null | halo_api's bootstrap identity read: `version`, `build`, `flavor`, `build_id`, `changelist`, `match_start_unix`. Null when it finds no build anchor. |
| `roster.status` | string | `decoded`, `not_found`, `truncated`, `unknown_build`, or `unavailable` (no build identity to read the table from). halo_api never publishes a partial table. |
| `roster.players[]` | array | `film_index`, `xuid`, `gamertag` for each player-table slot, in table order. |
| `events[]` | array | Summary events in film order: by timestamp, then by chunk and packet. |
| `events[].timestamp_us` | integer | The summary's recorded milliseconds times 1,000. Not the packet time, and not an observed action time. |
| `events[].kind` | string | `kill`, `death`, `medal`, `mode` or `unknown`. |
| `events[].medal` | object or null | For medals: `name` (halo_api's English reference name, or null for an unknown code pair) and `known`. Null for other kinds. |
| `events[].actor` | object | `xuid` and `gamertag` as recorded in the summary (a roster name never replaces it), and `roster_link`. |
| `events[].actor.roster_link` | object | `status`: `unique`, `missing` or `ambiguous`. For `unique`, `film_index` and `gamertag_matches` (whether the roster's gamertag is the same); otherwise both null. |
| `events[].codes` | object | The raw summary `type_code`, `metadata` and `medal_flag` halo_api reads the kind and medal from. |
| `events[].derivation` | string | How halo_api found the event: `guarded_v41_layout`. |
| `events[].provenance` | string | The event's provenance in halo_api's runtime index: `derived_summary`. |
| `events[].packet_count_matches` | boolean or null | Whether the event's summary packet declared as many events as were found; null when its count was unreadable. |
| `events[].source` | object | `chunk` (position among the chunks read, registry 0), `packet` and `record`. |
| `coverage.chunks` | object | Chunks read by kind (`registry`, `replication`, `summary`, `unknown`), and `replication_skipped`. |
| `coverage.transport` | object | Chunks by how they were read: `clear`, `zlib_complete`, `zlib_partial`, `zlib_rejected`. |
| `coverage.registry` | object | `stop` (`boundary_block` when the registry ended where its grammar says it should; `source_boundary` or `truncated_header` otherwise) and `archetypes`. |
| `coverage.replication`, `coverage.summary` | object | `packets`, `decoded` (a body decoder ran; nested records can still be partial), `opaque`, `opaque_reasons` (halo_api's reason to count), `unframed_bytes`, and `bits`: payload bits that are `fields`, `opaque`, `padding` or `unparsed`, plus `unmeasured_packets` whose coverage halo_api could not partition. Summary record streams are always opaque in halo_api's canonical model; the events come from its interpretation of them. |
| `coverage.records` | object | Runtime index entries by provenance: `total`, `recorded_read`, `partial_read`, `packet_envelope`, `derived_summary`. |
| `coverage.summary_packets[]` | array | Per summary packet: `chunk`, `packet`, `declared_events`, `candidate_events`, `count_matches`. |
| `coverage.summary_counts_match` | boolean | True when there is at least one summary packet and every one found as many events as it declared. |

### Example

This is the program's output for a synthetic film like the one in the tests, a bare v41 registry and three summary packets, so `build` is null and the roster is unavailable; a real film fills both, and its replication coverage.

```json
{
  "schema_version": 1,
  "tool": {
    "name": "halo-theater",
    "version": "0.1.0",
    "halo_api_rev": "0aaa05bf2e6b0b905510f51d7473181bde727ec6"
  },
  "film": {
    "major_version": 41,
    "format_version": 27,
    "build": null
  },
  "roster": {
    "status": "unavailable",
    "players": []
  },
  "events": [
    {
      "timestamp_us": 61250000,
      "kind": "kill",
      "medal": null,
      "actor": {
        "xuid": 2533274800000001,
        "gamertag": "Spartan",
        "roster_link": {
          "status": "missing",
          "film_index": null,
          "gamertag_matches": null
        }
      },
      "codes": {
        "type_code": 50,
        "metadata": 0,
        "medal_flag": 0
      },
      "derivation": "guarded_v41_layout",
      "provenance": "derived_summary",
      "packet_count_matches": true,
      "source": {
        "chunk": 1,
        "packet": 0,
        "record": 0
      }
    },
    {
      "timestamp_us": 61250000,
      "kind": "medal",
      "medal": {
        "name": "Double Kill",
        "known": true
      },
      "actor": {
        "xuid": 2533274800000001,
        "gamertag": "Spartan",
        "roster_link": {
          "status": "missing",
          "film_index": null,
          "gamertag_matches": null
        }
      },
      "codes": {
        "type_code": 100,
        "metadata": 0,
        "medal_flag": 1
      },
      "derivation": "guarded_v41_layout",
      "provenance": "derived_summary",
      "packet_count_matches": true,
      "source": {
        "chunk": 1,
        "packet": 1,
        "record": 0
      }
    },
    {
      "timestamp_us": 80000000,
      "kind": "medal",
      "medal": {
        "name": null,
        "known": false
      },
      "actor": {
        "xuid": 2533274800000001,
        "gamertag": "Spartan",
        "roster_link": {
          "status": "missing",
          "film_index": null,
          "gamertag_matches": null
        }
      },
      "codes": {
        "type_code": 50,
        "metadata": 255,
        "medal_flag": 1
      },
      "derivation": "guarded_v41_layout",
      "provenance": "derived_summary",
      "packet_count_matches": true,
      "source": {
        "chunk": 1,
        "packet": 2,
        "record": 0
      }
    }
  ],
  "coverage": {
    "chunks": {
      "registry": 1,
      "replication": 0,
      "summary": 1,
      "unknown": 0,
      "replication_skipped": 0
    },
    "transport": {
      "clear": 2,
      "zlib_complete": 0,
      "zlib_partial": 0,
      "zlib_rejected": 0
    },
    "registry": {
      "stop": "source_boundary",
      "archetypes": 0
    },
    "replication": {
      "packets": 0,
      "decoded": 0,
      "opaque": 0,
      "opaque_reasons": {},
      "unframed_bytes": 0,
      "bits": {
        "fields": 0,
        "opaque": 0,
        "padding": 0,
        "unparsed": 0,
        "unmeasured_packets": 0
      }
    },
    "summary": {
      "packets": 3,
      "decoded": 3,
      "opaque": 0,
      "opaque_reasons": {},
      "unframed_bytes": 0,
      "bits": {
        "fields": 96,
        "opaque": 46344,
        "padding": 0,
        "unparsed": 0,
        "unmeasured_packets": 0
      }
    },
    "records": {
      "total": 6,
      "recorded_read": 0,
      "partial_read": 0,
      "packet_envelope": 3,
      "derived_summary": 3
    },
    "summary_packets": [
      {
        "chunk": 1,
        "packet": 0,
        "declared_events": 1,
        "candidate_events": 1,
        "count_matches": true
      },
      {
        "chunk": 1,
        "packet": 1,
        "declared_events": 1,
        "candidate_events": 1,
        "count_matches": true
      },
      {
        "chunk": 1,
        "packet": 2,
        "declared_events": 1,
        "candidate_events": 1,
        "count_matches": true
      }
    ],
    "summary_counts_match": true
  }
}
```

## Development

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

halo_api ships no whole captured film, so the tests build a synthetic one the way halo_api's runtime tests do, from its hand-written summary wire cases (`tests/fixtures/runtime-summaries-v41.json`, copied from halo_api).

To move to a newer halo_api, change the `rev` in `Cargo.toml` and `HALO_API_REV` in `src/main.rs` together (a test checks they match), then run `cargo update -p halo_api`.

Releases: bump `version` in `Cargo.toml`, merge to `main`, then push a `v<version>` tag. The release workflow builds the three targets and publishes the archives and checksums.

## Credits

- [halo_api](https://github.com/nuzzles/halo_api) by Spencer Imbleau ([nuzzles](https://github.com/nuzzles), [simbleau](https://github.com/simbleau)), MIT OR Apache-2.0, and its pull request [#5](https://github.com/nuzzles/halo_api/pull/5) adding the Theater decoder this program is built on.
- Spencer Imbleau's write-up, [Reverse-engineering Halo: Infinite's Theater films](https://spencer.imbleau.com/blog/ai/rust/2026/09/14/halo-infinite-theater-format-astra.html).
- Den Delimarsky's research at den.dev, [Extracting Match Stats From Halo Infinite Film Files](https://den.dev/blog/extracting-stats-film-files-halo-infinite/), which showed how to find and download films and read gamertags, kills and deaths from them.
- Guillaume Sitbon's [LevelUp](https://github.com/JGtm/LevelUp), the reference parser halo_api's decoder is checked against.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE). Halo and Halo Infinite are trademarks of Microsoft Corporation; this project is not affiliated with or endorsed by Microsoft or Halo Studios.
