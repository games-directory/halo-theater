//! halo-theater: decode a Halo Infinite Theater film into JSON on stdout.
//!
//! The decoding is halo_api's (github.com/nuzzles/halo_api); this program only reads the film
//! from disk and prints what halo_api types. Exit status: 0 on success, 1 when the film cannot
//! be read or parsed, 2 on a usage error; the error is one line on stderr.

mod input;
mod report;

use halo_api::theater::{Film, TheaterRuntime};
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The halo_api commit this program is built against; the same rev as in Cargo.toml.
const HALO_API_REV: &str = "0aaa05bf2e6b0b905510f51d7473181bde727ec6";

const USAGE: &str = "\
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
";

struct Options {
    film: PathBuf,
    pretty: bool,
    summaries_only: bool,
}

enum Command {
    Run(Options),
    Help,
    Version,
}

fn main() -> ExitCode {
    // A panic in the decoder still exits non-zero (101), with one line on stderr.
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_owned());
        eprintln!("{NAME}: internal error: {}", one_line(&message));
    }));

    let command = match parse_args(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{NAME}: {} (see --help)", one_line(&message));
            return ExitCode::from(2);
        }
    };

    match command {
        Command::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("{NAME} {VERSION} (halo_api {HALO_API_REV})");
            ExitCode::SUCCESS
        }
        Command::Run(options) => match run(&options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{NAME}: {}", one_line(&message));
                ExitCode::from(1)
            }
        },
    }
}

fn parse_args(args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let mut film = None;
    let mut pretty = false;
    let mut summaries_only = false;
    let mut only_paths = false;

    for arg in args {
        match arg.to_str() {
            _ if only_paths => {}
            Some("-h" | "--help") => return Ok(Command::Help),
            Some("-V" | "--version") => return Ok(Command::Version),
            Some("--pretty") => {
                pretty = true;
                continue;
            }
            Some("--summaries-only") => {
                summaries_only = true;
                continue;
            }
            Some("--") => {
                only_paths = true;
                continue;
            }
            Some(flag) if flag.starts_with('-') && flag.len() > 1 => {
                return Err(format!("unknown option {flag}"));
            }
            _ => {}
        }
        if film.is_some() {
            return Err("expected one film".to_owned());
        }
        film = Some(PathBuf::from(arg));
    }

    let film = film.ok_or("missing <FILM>")?;
    Ok(Command::Run(Options {
        film,
        pretty,
        summaries_only,
    }))
}

fn run(options: &Options) -> Result<(), String> {
    let input = input::load(&options.film, options.summaries_only)?;
    let film = Film::parse(input.chunks).map_err(|error| error.to_string())?;
    let runtime = TheaterRuntime::load(film);
    let tool = report::Tool {
        name: NAME,
        version: VERSION,
        halo_api_rev: HALO_API_REV,
    };
    let report = report::build(&runtime, tool, input.skipped_replication);

    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
    let written = if options.pretty {
        serde_json::to_writer_pretty(&mut stdout, &report)
    } else {
        serde_json::to_writer(&mut stdout, &report)
    };
    written.map_err(|error| format!("writing JSON: {error}"))?;
    writeln!(stdout)
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("writing JSON: {error}"))
}

fn one_line(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(OsString::from))
    }

    #[test]
    fn pinned_rev_matches_the_cargo_manifest() {
        assert!(include_str!("../Cargo.toml").contains(&format!("rev = \"{HALO_API_REV}\"")));
    }

    #[test]
    fn parses_flags_in_any_order() {
        let Ok(Command::Run(options)) = parse(&["--summaries-only", "films/one", "--pretty"])
        else {
            panic!()
        };
        assert_eq!(options.film, PathBuf::from("films/one"));
        assert!(options.pretty && options.summaries_only);
        assert!(
            matches!(parse(&["--", "--pretty"]), Ok(Command::Run(options)) if options.film == PathBuf::from("--pretty"))
        );
    }

    #[test]
    fn rejects_bad_usage() {
        assert_eq!(parse(&[]).err().as_deref(), Some("missing <FILM>"));
        assert_eq!(
            parse(&["a", "b"]).err().as_deref(),
            Some("expected one film")
        );
        assert_eq!(
            parse(&["--json"]).err().as_deref(),
            Some("unknown option --json")
        );
    }
}
