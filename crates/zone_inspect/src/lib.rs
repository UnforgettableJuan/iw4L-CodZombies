#![forbid(unsafe_code)]

mod entities;
mod pattern;
mod report;
mod t5;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use asset_transport::{GamesRoot, ZoneGame};

pub use entities::{EntityCensus, census};
pub use t5::{
    DECLARED_BLOCKS_CAP, Placement, Stop, T5Inventory, TypeCount, WorldParts, inventory_t5_image,
    type_name, walkable,
};

use report::{PREFIX, quoted};

pub const COMMAND: &str = "inspect-zone";

pub const USAGE: &str = "usage: iw4l inspect-zone <zone|path|pattern>... [--names]
       iw4l inspect-zone --list [pattern]

  zone     a zone name: t5:zombie_theater picks Black Ops; a bare name takes the
           first game that has it (MW2, then Black Ops, then MW3)
  path     a .ff file anywhere on disk; needs no IW4L_GAMES
  pattern  * matches any run of characters, e.g. 't5:zombie_*'
  --names  also print asset names, raw file names, targetnames and entity keys
  --list   list every .ff under IW4L_GAMES with its game and version; a pattern
           without * matches anywhere in the name

Each inspected zone also writes a full report, names included, to
iw4l-artifacts/inspect/. Exit status: 0 every walk completed, 1 a walk stopped
or a zone could not be inventoried, 2 bad arguments or nothing to inspect.";

pub const EXIT_COMPLETE: i32 = 0;
pub const EXIT_STOPPED: i32 = 1;
pub const EXIT_SETUP: i32 = 2;

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    List(Option<String>),
    Inspect { targets: Vec<String>, names: bool },
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut list = false;
    let mut names = false;
    let mut free = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help),
            "--list" => list = true,
            "--names" => names = true,
            flag if flag.starts_with("--") => return Err(format!("unknown flag `{flag}`")),
            _ => free.push(arg.clone()),
        }
    }
    if list {
        if names {
            return Err("--names does not apply to --list".to_owned());
        }
        if free.len() > 1 {
            return Err("--list takes at most one pattern".to_owned());
        }
        return Ok(Command::List(free.pop()));
    }
    if free.is_empty() {
        return Err("nothing to inspect".to_owned());
    }
    Ok(Command::Inspect {
        targets: free,
        names,
    })
}

fn emit(out: &mut dyn Write, line: &str) {
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

#[derive(Default)]
struct Games(Option<Result<GamesRoot, String>>);

impl Games {
    fn get(&mut self) -> Result<&GamesRoot, String> {
        self.0
            .get_or_insert_with(asset_transport::games_root_from_env)
            .as_ref()
            .map_err(Clone::clone)
    }
}

pub fn run(args: &[String], artifacts: Option<&Path>, out: &mut dyn Write) -> i32 {
    let command = match parse(args) {
        Ok(command) => command,
        Err(reason) => {
            emit(out, &format!("{PREFIX} error reason={}", quoted(&reason)));
            for line in USAGE.lines() {
                emit(out, line);
            }
            return EXIT_SETUP;
        }
    };
    let mut games = Games::default();
    match command {
        Command::Help => {
            for line in USAGE.lines() {
                emit(out, line);
            }
            EXIT_COMPLETE
        }
        Command::List(filter) => match games.get() {
            Ok(root) => list(root, filter.as_deref(), out),
            Err(reason) => {
                emit(
                    out,
                    &format!(
                        "{PREFIX} error reason={}",
                        quoted(&format!("--list needs IW4L_GAMES: {reason}"))
                    ),
                );
                EXIT_SETUP
            }
        },
        Command::Inspect { targets, names } => {
            let mut code = EXIT_COMPLETE;
            let mut inspected = 0usize;
            for target in &targets {
                for resolved in resolve(target, &mut games) {
                    match resolved {
                        Ok((label, path)) => {
                            inspected += 1;
                            code = code.max(inspect_file(&label, &path, names, artifacts, out));
                        }
                        Err(reason) => {
                            emit(
                                out,
                                &format!(
                                    "{PREFIX} error target={} reason={}",
                                    quoted(target),
                                    quoted(&reason)
                                ),
                            );
                            code = code.max(EXIT_SETUP);
                        }
                    }
                }
            }
            if inspected == 0 {
                code = EXIT_SETUP;
            }
            code
        }
    }
}

fn resolve(target: &str, games: &mut Games) -> Vec<Result<(String, PathBuf), String>> {
    let as_path = Path::new(target);
    if as_path.is_file() {
        return vec![Ok((target.to_owned(), as_path.to_path_buf()))];
    }
    let root = match games.get() {
        Ok(root) => root,
        Err(reason) => {
            return vec![Err(format!(
                "not a file, and finding a zone by name needs IW4L_GAMES: {reason}"
            ))];
        }
    };
    if pattern::is_pattern(target) {
        let hits = matching_zones(root, target);
        if hits.is_empty() {
            return vec![Err(format!("no .ff under {} matches", root.0.display()))];
        }
        return hits.into_iter().map(Ok).collect();
    }
    match asset_transport::find_zone_file(root, target) {
        Ok(zone) => vec![Ok((target.to_owned(), zone.path))],
        Err(reason) => vec![Err(reason)],
    }
}

fn zone_key(path: &Path) -> (String, Option<ZoneGame>) {
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().to_ascii_lowercase());
    let game = asset_transport::zone_game_for_path(path);
    let prefix = game.map_or("unknown", ZoneGame::prefix);
    (format!("{prefix}:{stem}"), game)
}

fn pattern_filter(filter: &str) -> (Option<ZoneGame>, String) {
    let lowered = filter.trim().to_ascii_lowercase();
    let (game, stem) = asset_transport::split_zone_key(&lowered);
    (game, stem.to_owned())
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().to_ascii_lowercase())
}

fn matching_zones(root: &GamesRoot, target: &str) -> Vec<(String, PathBuf)> {
    let (game, stem_pattern) = pattern_filter(target);
    let mut hits: Vec<(String, PathBuf)> = asset_transport::zone_files(root)
        .filter_map(Result::ok)
        .filter(|path| pattern::matches(&stem_pattern, &stem_of(path)))
        .filter_map(|path| {
            let (key, zone_game) = zone_key(&path);
            (game.is_none() || zone_game == game).then_some((key, path))
        })
        .collect();
    hits.sort();
    hits
}

struct Envelope {
    file_bytes: u64,
    magic: String,
    version: Option<u32>,
}

fn read_envelope(path: &Path) -> Result<Envelope, String> {
    let file_bytes = std::fs::metadata(path)
        .map_err(|error| format!("cannot stat: {error}"))?
        .len();
    let mut head = Vec::with_capacity(12);
    std::fs::File::open(path)
        .map_err(|error| format!("cannot open: {error}"))?
        .take(12)
        .read_to_end(&mut head)
        .map_err(|error| format!("cannot read: {error}"))?;
    let magic_bytes = head.get(..8).unwrap_or(&head);
    let magic = if magic_bytes.iter().all(|b| b.is_ascii_graphic()) {
        String::from_utf8_lossy(magic_bytes).into_owned()
    } else {
        magic_bytes.iter().map(|b| format!("{b:02x}")).collect()
    };
    let version = head
        .get(8..12)
        .and_then(|v| v.try_into().ok())
        .map(u32::from_le_bytes);
    Ok(Envelope {
        file_bytes,
        magic,
        version,
    })
}

fn list(root: &GamesRoot, filter: Option<&str>, out: &mut dyn Write) -> i32 {
    let filter = filter.map(|f| {
        let (game, stem) = pattern_filter(f);
        let stem = if pattern::is_pattern(&stem) {
            stem
        } else {
            format!("*{stem}*")
        };
        (game, stem)
    });
    let mut rows = Vec::new();
    let mut errors = 0usize;
    for entry in asset_transport::zone_files(root) {
        let path = match entry {
            Ok(path) => path,
            Err(reason) => {
                errors += 1;
                emit(out, &format!("{PREFIX} warn reason={}", quoted(&reason)));
                continue;
            }
        };
        let (key, game) = zone_key(&path);
        if let Some((want_game, stem_pattern)) = &filter
            && (!pattern::matches(stem_pattern, &stem_of(&path))
                || (want_game.is_some() && game != *want_game))
        {
            continue;
        }
        rows.push((key, path));
    }
    rows.sort();
    for (key, path) in &rows {
        let line = match read_envelope(path) {
            Ok(envelope) => format!(
                "{PREFIX} zone key={key} magic={} version={} bytes={} path={}",
                envelope.magic,
                envelope
                    .version
                    .map_or_else(|| String::from("none"), |v| format!("{v:#x}")),
                envelope.file_bytes,
                quoted(&path.display().to_string())
            ),
            Err(reason) => format!(
                "{PREFIX} zone key={key} path={} error={}",
                quoted(&path.display().to_string()),
                quoted(&reason)
            ),
        };
        emit(out, &line);
    }
    emit(
        out,
        &format!(
            "{PREFIX} listed zones={} errors={errors} root={}",
            rows.len(),
            quoted(&root.0.display().to_string())
        ),
    );
    EXIT_COMPLETE
}

fn inspect_file(
    label: &str,
    path: &Path,
    names: bool,
    artifacts: Option<&Path>,
    out: &mut dyn Write,
) -> i32 {
    let mut head = Vec::new();
    let game = asset_transport::zone_game_for_path(path);
    match read_envelope(path) {
        Ok(envelope) => head.push(format!(
            "{PREFIX} open target={} path={} file_bytes={} magic={} version={} game={}",
            quoted(label),
            quoted(&path.display().to_string()),
            envelope.file_bytes,
            envelope.magic,
            envelope
                .version
                .map_or_else(|| String::from("none"), |v| format!("{v:#x}")),
            game.map_or("unknown", ZoneGame::prefix),
        )),
        Err(reason) => {
            emit(
                out,
                &format!(
                    "{PREFIX} error target={} path={} reason={}",
                    quoted(label),
                    quoted(&path.display().to_string()),
                    quoted(&reason)
                ),
            );
            return EXIT_SETUP;
        }
    }
    for line in &head {
        emit(out, line);
    }

    let (code, inventory, tail) = inventory_for(label, path, game);
    if let Some(inventory) = &inventory {
        for line in report::summary(inventory, names) {
            emit(out, &line);
        }
    }
    let mut tail = tail;
    let written = artifacts.map(|dir| write_report(dir, path, &head, inventory.as_ref(), &tail));
    match written {
        Some(Ok(report)) => tail.push(format!(
            "{PREFIX} report path={}",
            quoted(&report.display().to_string())
        )),
        Some(Err(reason)) => tail.push(format!(
            "{PREFIX} warn report_unwritten reason={}",
            quoted(&reason)
        )),
        None => {}
    }
    for line in &tail {
        emit(out, line);
    }
    code
}

fn inventory_for(
    label: &str,
    path: &Path,
    game: Option<ZoneGame>,
) -> (i32, Option<T5Inventory>, Vec<String>) {
    let done = |status: &str, extra: String| {
        format!(
            "{PREFIX} done target={} status={status}{extra}",
            quoted(label)
        )
    };
    if game != Some(ZoneGame::T5) {
        let reason = match game {
            Some(game) => format!(
                "the inventory reads Black Ops (t5) zones; this one is {}",
                game.prefix()
            ),
            None => "not a zone version this runtime reads (MW2 0x114, Black Ops 0x1d9, MW3 0x1)"
                .to_owned(),
        };
        return (
            EXIT_STOPPED,
            None,
            vec![done("skipped", format!(" reason={}", quoted(&reason)))],
        );
    }
    let image = match asset_transport::open_zone(path) {
        Ok(image) => image,
        Err(error) => {
            return (
                EXIT_STOPPED,
                None,
                vec![done(
                    "unreadable",
                    format!(" reason={}", quoted(&error.to_string())),
                )],
            );
        }
    };
    match inventory_t5_image(&image.bytes) {
        Ok(inventory) => {
            let status = if inventory.complete() {
                "complete"
            } else {
                "stopped"
            };
            let trailing = inventory
                .trailing_bytes
                .map_or_else(String::new, |n| format!(" trailing_bytes={n}"));
            let line = done(
                status,
                format!(
                    " processed={} assets={}{trailing}",
                    inventory.processed, inventory.assets
                ),
            );
            let code = if inventory.complete() {
                EXIT_COMPLETE
            } else {
                EXIT_STOPPED
            };
            (code, Some(inventory), vec![line])
        }
        Err(reason) => (
            EXIT_STOPPED,
            None,
            vec![done("unreadable", format!(" reason={}", quoted(&reason)))],
        ),
    }
}

fn report_file_name(label: &str) -> String {
    let name: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{}.txt", name.trim_matches('_'))
}

fn write_report(
    artifacts: &Path,
    path: &Path,
    head: &[String],
    inventory: Option<&T5Inventory>,
    tail: &[String],
) -> Result<PathBuf, String> {
    let dir = artifacts.join("inspect");
    std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let file = dir.join(report_file_name(&zone_key(path).0));
    let text = report::full_report(path, head, inventory, tail);
    std::fs::write(&file, text).map_err(|error| format!("{}: {error}", file.display()))?;
    Ok(file)
}

#[cfg(test)]
mod tests;
