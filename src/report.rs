//! The JSON this program prints, built only from what halo_api types as an interpretation or a typed feature.
//!
//! Field names follow halo_api's own where one exists. Nothing here gives a raw field a meaning
//! halo_api does not give it: team, weapon, victim and position are absent because halo_api does
//! not link them to a player yet.

use halo_api::theater::TheaterRuntime;
use halo_api::theater::film::{
    ChunkTransport, FilmDataChunk, Packet, PacketBody, PacketRead, PacketStream, RegistryStop,
    SourceRegionKind,
};
use halo_api::theater::runtime::identity::PlayerTableError;
use halo_api::theater::runtime::{
    EventKind, PlayerLink, Provenance, RecordRef, SourceRef, SummaryDerivation, SummaryEvent,
    SummaryPayload,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// Bumped when a field is removed or changes meaning; adding a field does not bump it.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub tool: Tool,
    pub film: FilmInfo,
    pub roster: Roster,
    pub events: Vec<Event>,
    pub coverage: Coverage,
}

#[derive(Serialize)]
pub struct Tool {
    pub name: &'static str,
    pub version: &'static str,
    pub halo_api_rev: &'static str,
}

#[derive(Serialize)]
pub struct FilmInfo {
    /// First registry word; Film::parse accepts 41 only.
    pub major_version: u32,
    /// Second registry word, when the registry header is complete.
    pub format_version: Option<u32>,
    /// halo_api's bootstrap identity read, or null when it found no build anchor.
    pub build: Option<Build>,
}

#[derive(Serialize)]
pub struct Build {
    pub version: String,
    pub build: String,
    pub flavor: String,
    pub build_id: u32,
    pub changelist: u32,
    pub match_start_unix: u32,
}

#[derive(Serialize)]
pub struct Roster {
    /// decoded, not_found, truncated, unknown_build, or unavailable (no bootstrap identity to read it from).
    pub status: &'static str,
    pub players: Vec<RosterPlayer>,
}

#[derive(Serialize)]
pub struct RosterPlayer {
    pub film_index: usize,
    pub xuid: u64,
    pub gamertag: String,
}

#[derive(Serialize)]
pub struct Event {
    /// The summary's recorded milliseconds, times 1,000. Not the packet time or an observed action time.
    pub timestamp_us: u64,
    /// kill, death, medal, mode or unknown.
    pub kind: &'static str,
    /// Present for medal events only.
    pub medal: Option<Medal>,
    pub actor: Actor,
    pub codes: Codes,
    /// halo_api's SummaryDerivation: how the event was found.
    pub derivation: &'static str,
    /// halo_api's Provenance for the event's entry in the runtime index.
    pub provenance: &'static str,
    /// Whether this event's summary packet declared as many events as were found in it (null: count unreadable).
    pub packet_count_matches: Option<bool>,
    pub source: Source,
}

#[derive(Serialize)]
pub struct Medal {
    /// halo_api's English reference name, or null for a code pair it does not know.
    pub name: Option<&'static str>,
    pub known: bool,
}

#[derive(Serialize)]
pub struct Actor {
    pub xuid: u64,
    /// As recorded in the summary; a roster name never replaces it.
    pub gamertag: String,
    pub roster_link: RosterLink,
}

#[derive(Serialize)]
pub struct RosterLink {
    /// unique, missing or ambiguous.
    pub status: &'static str,
    pub film_index: Option<usize>,
    pub gamertag_matches: Option<bool>,
}

#[derive(Serialize)]
pub struct Codes {
    pub type_code: u8,
    pub metadata: u8,
    pub medal_flag: u8,
}

#[derive(Serialize)]
pub struct Source {
    /// Position in the chunk list read (registry = 0), not the manifest index.
    pub chunk: usize,
    pub packet: usize,
    pub record: usize,
}

#[derive(Serialize)]
pub struct Coverage {
    pub chunks: ChunkCounts,
    pub transport: TransportCounts,
    pub registry: RegistryCoverage,
    pub replication: PacketCoverage,
    pub summary: PacketCoverage,
    /// Runtime index entries by halo_api Provenance.
    pub records: RecordCounts,
    pub summary_packets: Vec<SummaryPacketReport>,
    /// True when there is at least one summary packet and every one declared as many events as were found.
    /// halo_api notes that matching counts are evidence of coverage, not proof of a complete grammar.
    pub summary_counts_match: bool,
}

#[derive(Serialize, Default)]
pub struct ChunkCounts {
    pub registry: usize,
    pub replication: usize,
    pub summary: usize,
    pub unknown: usize,
    /// Replication chunks listed but not read (--summaries-only).
    pub replication_skipped: usize,
}

#[derive(Serialize, Default)]
pub struct TransportCounts {
    pub clear: usize,
    pub zlib_complete: usize,
    pub zlib_partial: usize,
    pub zlib_rejected: usize,
}

#[derive(Serialize)]
pub struct RegistryCoverage {
    /// boundary_block (complete), source_boundary or truncated_header.
    pub stop: &'static str,
    pub archetypes: usize,
}

#[derive(Serialize, Default)]
pub struct PacketCoverage {
    pub packets: usize,
    /// A supported body decoder ran; nested records can still be partial.
    pub decoded: usize,
    /// No decoder for the body; counted by halo_api's reason.
    pub opaque: usize,
    pub opaque_reasons: BTreeMap<String, usize>,
    /// Chunk bytes framing could not classify as packets.
    pub unframed_bytes: usize,
    pub bits: BitCoverage,
}

/// Payload bits by halo_api's source coverage: fields, opaque, padding, unparsed.
#[derive(Serialize, Default)]
pub struct BitCoverage {
    pub fields: usize,
    pub opaque: usize,
    pub padding: usize,
    pub unparsed: usize,
    /// Packets whose coverage halo_api could not partition (invalid envelope or conflicting regions).
    pub unmeasured_packets: usize,
}

#[derive(Serialize, Default)]
pub struct RecordCounts {
    pub total: usize,
    pub recorded_read: usize,
    pub partial_read: usize,
    pub packet_envelope: usize,
    pub derived_summary: usize,
}

#[derive(Serialize)]
pub struct SummaryPacketReport {
    pub chunk: usize,
    pub packet: usize,
    pub declared_events: Option<u32>,
    pub candidate_events: usize,
    pub count_matches: Option<bool>,
}

/// Build the report from a loaded runtime.
pub fn build(runtime: &TheaterRuntime, tool: Tool, skipped_replication: usize) -> Report {
    let film = runtime.film();
    let registry = &film.registry;

    let major_version = registry.data.get(..4).map_or(0, |word| {
        u32::from_le_bytes(word.try_into().expect("four bytes"))
    });
    let build = runtime
        .interpretations()
        .identity
        .identity
        .as_ref()
        .map(|identity| Build {
            version: identity.version.clone(),
            build: identity.build.clone(),
            flavor: identity.flavor.clone(),
            build_id: identity.build_id,
            changelist: identity.changelist,
            match_start_unix: identity.match_start_unix,
        });

    let roster = match &runtime.interpretations().player_table {
        None => Roster {
            status: "unavailable",
            players: Vec::new(),
        },
        Some(table) => Roster {
            status: match table.error {
                None => "decoded",
                Some(PlayerTableError::NotFound) => "not_found",
                Some(PlayerTableError::Truncated) => "truncated",
                Some(PlayerTableError::UnknownBuild) => "unknown_build",
            },
            players: table
                .slots
                .iter()
                .map(|slot| RosterPlayer {
                    film_index: slot.film_index,
                    xuid: slot.xuid,
                    gamertag: slot.gamertag.clone(),
                })
                .collect(),
        },
    };

    let reports: Vec<SummaryPacketReport> = runtime
        .summary_reports()
        .iter()
        .map(|report| SummaryPacketReport {
            chunk: report.source.chunk,
            packet: report.source.packet,
            declared_events: report.declared_events,
            candidate_events: report.candidate_events,
            count_matches: report.count_matches(),
        })
        .collect();
    let counts_by_packet: HashMap<(usize, usize), Option<bool>> = reports
        .iter()
        .map(|report| ((report.chunk, report.packet), report.count_matches))
        .collect();

    let mut records = RecordCounts::default();
    let mut summary_provenance = HashMap::new();
    for event in runtime.events() {
        records.total += 1;
        match event.provenance {
            Provenance::RecordedRead => records.recorded_read += 1,
            Provenance::PartialRead => records.partial_read += 1,
            Provenance::PacketEnvelope => records.packet_envelope += 1,
            Provenance::DerivedSummary => records.derived_summary += 1,
        }
        if event.kind == EventKind::Summary {
            summary_provenance.insert(key(event.source), event.provenance);
        }
    }

    let events = runtime
        .summary_events()
        .iter()
        .map(|event| summary_event(event, &summary_provenance, &counts_by_packet))
        .collect();

    let mut chunks = ChunkCounts {
        registry: 1,
        replication_skipped: skipped_replication,
        ..Default::default()
    };
    let mut transport = TransportCounts::default();
    let mut replication = PacketCoverage::default();
    let mut summary = PacketCoverage::default();
    count_transport(&mut transport, registry.transport);
    for chunk in &film.chunks {
        match chunk {
            FilmDataChunk::Replication(chunk) => {
                chunks.replication += 1;
                count_transport(&mut transport, chunk.transport);
                measure(&mut replication, &chunk.body, &chunk.data);
            }
            FilmDataChunk::Summary(chunk) => {
                chunks.summary += 1;
                count_transport(&mut transport, chunk.transport);
                measure(&mut summary, &chunk.body, &chunk.data);
            }
            FilmDataChunk::Unknown(chunk) => {
                chunks.unknown += 1;
                count_transport(&mut transport, chunk.transport);
            }
        }
    }

    let summary_counts_match = !reports.is_empty()
        && reports
            .iter()
            .all(|report| report.count_matches == Some(true));

    Report {
        schema_version: SCHEMA_VERSION,
        tool,
        film: FilmInfo {
            major_version,
            format_version: registry.body.header.map(|header| header[1]),
            build,
        },
        roster,
        events,
        coverage: Coverage {
            chunks,
            transport,
            registry: RegistryCoverage {
                stop: match registry.body.stop {
                    RegistryStop::BoundaryBlock => "boundary_block",
                    RegistryStop::SourceBoundary => "source_boundary",
                    RegistryStop::TruncatedHeader => "truncated_header",
                },
                archetypes: registry.body.registry.archetypes.len(),
            },
            replication,
            summary,
            records,
            summary_packets: reports,
            summary_counts_match,
        },
    }
}

fn summary_event(
    event: &SummaryEvent,
    provenance: &HashMap<(usize, usize, usize), Provenance>,
    counts_by_packet: &HashMap<(usize, usize), Option<bool>>,
) -> Event {
    let (kind, medal) = match event.payload {
        SummaryPayload::Kill => ("kill", None),
        SummaryPayload::Death => ("death", None),
        SummaryPayload::Mode => ("mode", None),
        SummaryPayload::Unknown => ("unknown", None),
        SummaryPayload::Medal(medal) => (
            "medal",
            Some(Medal {
                name: medal.name(),
                known: medal.name().is_some(),
            }),
        ),
    };
    let (status, film_index, gamertag_matches) = match event.actor.roster_link {
        PlayerLink::Unique {
            film_index,
            gamertag_matches,
        } => ("unique", Some(film_index), Some(gamertag_matches)),
        PlayerLink::Missing => ("missing", None, None),
        PlayerLink::Ambiguous => ("ambiguous", None, None),
    };
    let source = key(event.source);

    Event {
        timestamp_us: event.timestamp_us,
        kind,
        medal,
        actor: Actor {
            xuid: event.actor.xuid,
            gamertag: event.actor.gamertag.clone(),
            roster_link: RosterLink {
                status,
                film_index,
                gamertag_matches,
            },
        },
        codes: Codes {
            type_code: event.codes.type_code,
            metadata: event.codes.metadata,
            medal_flag: event.codes.medal_flag,
        },
        derivation: match event.derivation {
            SummaryDerivation::GuardedV41Layout => "guarded_v41_layout",
        },
        provenance: match provenance.get(&source) {
            Some(Provenance::RecordedRead) => "recorded_read",
            Some(Provenance::PartialRead) => "partial_read",
            Some(Provenance::PacketEnvelope) => "packet_envelope",
            Some(Provenance::DerivedSummary) => "derived_summary",
            None => "unindexed",
        },
        packet_count_matches: counts_by_packet
            .get(&(source.0, source.1))
            .copied()
            .flatten(),
        source: Source {
            chunk: source.0,
            packet: source.1,
            record: source.2,
        },
    }
}

/// (chunk, packet, record) for a summary source; record is 0 for anything that is not a summary read.
fn key(source: SourceRef) -> (usize, usize, usize) {
    let record = match source.record {
        RecordRef::Summary(record) => record,
        _ => 0,
    };
    (source.chunk, source.packet, record)
}

fn count_transport(counts: &mut TransportCounts, transport: ChunkTransport) {
    match transport {
        ChunkTransport::Clear => counts.clear += 1,
        ChunkTransport::ZlibComplete => counts.zlib_complete += 1,
        ChunkTransport::ZlibPartial => counts.zlib_partial += 1,
        ChunkTransport::ZlibRejected => counts.zlib_rejected += 1,
    }
}

fn measure<T: PacketBody>(coverage: &mut PacketCoverage, stream: &PacketStream<T>, data: &[u8]) {
    coverage.unframed_bytes += stream
        .opaque
        .iter()
        .map(|range| range.end.saturating_sub(range.start))
        .sum::<usize>();
    for packet in &stream.packets {
        coverage.packets += 1;
        match &packet.body {
            PacketRead::Decoded(_) => coverage.decoded += 1,
            PacketRead::Opaque { reason } => {
                coverage.opaque += 1;
                *coverage
                    .opaque_reasons
                    .entry(reason.to_string())
                    .or_default() += 1;
            }
        }
        measure_bits(&mut coverage.bits, packet, data);
    }
}

fn measure_bits<T: PacketBody>(bits: &mut BitCoverage, packet: &Packet<T>, data: &[u8]) {
    let Some(regions) = packet.source_coverage(data) else {
        bits.unmeasured_packets += 1;
        return;
    };
    for region in regions {
        let width = region.source.end - region.source.start;
        match region.kind {
            SourceRegionKind::Fields => bits.fields += width,
            SourceRegionKind::Opaque => bits.opaque += width,
            SourceRegionKind::Padding => bits.padding += width,
            SourceRegionKind::Unparsed => bits.unparsed += width,
        }
    }
}
