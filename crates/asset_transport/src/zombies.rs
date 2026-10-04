//! Zombies maps across World at War, Black Ops and Black Ops II: which ones the
//! games root holds, and whether a FastFile reader exists for their envelope.
//! Discovery only; loading a zombies map is the match walk's business.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::discover::{IW4_ZONE_VERSION, game_files};

/// World at War PC zones: `IWffu100`, version 0x183.
const T4_ZONE_VERSION: u32 = 0x183;

const T5_ZONE_VERSION: u32 = 0x1D9;

/// Black Ops II PC zones: `TAff0100`, version 0x93.
const T6_ZONE_VERSION: u32 = 0x93;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ZombiesGame {
    WorldAtWar,
    BlackOps,
    BlackOps2,
}

impl ZombiesGame {
    pub fn label(self) -> &'static str {
        match self {
            Self::WorldAtWar => "World at War",
            Self::BlackOps => "Black Ops",
            Self::BlackOps2 => "Black Ops II",
        }
    }

    /// Whether IW4L has a FastFile reader for this game's zones.
    pub fn has_reader(self) -> bool {
        matches!(self, Self::BlackOps)
    }

    fn zone_version(self) -> u32 {
        match self {
            Self::WorldAtWar => T4_ZONE_VERSION,
            Self::BlackOps => T5_ZONE_VERSION,
            Self::BlackOps2 => T6_ZONE_VERSION,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZombiesMap {
    pub game: ZombiesGame,
    pub zone: &'static str,
    pub title: &'static str,
}

const fn zm(game: ZombiesGame, zone: &'static str, title: &'static str) -> ZombiesMap {
    ZombiesMap { game, zone, title }
}

use ZombiesGame::{BlackOps, BlackOps2, WorldAtWar};

/// Every retail zombies map, by the zone stem its game ships it under. Black
/// Ops' Rezurrection remasters of the four World at War maps are `zombie_cod5_*`.
pub const ZOMBIES_MAPS: &[ZombiesMap] = &[
    zm(WorldAtWar, "nazi_zombie_prototype", "Nacht der Untoten"),
    zm(WorldAtWar, "nazi_zombie_asylum", "Verrückt"),
    zm(WorldAtWar, "nazi_zombie_sumpf", "Shi No Numa"),
    zm(WorldAtWar, "nazi_zombie_factory", "Der Riese"),
    zm(BlackOps, "zombie_cod5_prototype", "Nacht der Untoten"),
    zm(BlackOps, "zombie_cod5_asylum", "Verrückt"),
    zm(BlackOps, "zombie_cod5_sumpf", "Shi No Numa"),
    zm(BlackOps, "zombie_cod5_factory", "Der Riese"),
    zm(BlackOps, "zombie_theater", "Kino der Toten"),
    zm(BlackOps, "zombie_pentagon", "\"Five\""),
    zm(BlackOps, "zombie_cosmodrome", "Ascension"),
    zm(BlackOps, "zombie_coast", "Call of the Dead"),
    zm(BlackOps, "zombie_temple", "Shangri-La"),
    zm(BlackOps, "zombie_moon", "Moon"),
    zm(BlackOps2, "zm_transit", "TranZit"),
    zm(BlackOps2, "zm_transit_dr", "Diner"),
    zm(BlackOps2, "zm_nuked", "Nuketown Zombies"),
    zm(BlackOps2, "zm_highrise", "Die Rise"),
    zm(BlackOps2, "zm_prison", "Mob of the Dead"),
    zm(BlackOps2, "zm_buried", "Buried"),
    zm(BlackOps2, "zm_tomb", "Origins"),
];

pub fn zombies_map(game: ZombiesGame, zone: &str) -> Option<&'static ZombiesMap> {
    ZOMBIES_MAPS
        .iter()
        .find(|map| map.game == game && map.zone.eq_ignore_ascii_case(zone))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZombiesZone {
    /// The envelope matches the game the stem belongs to.
    Found(PathBuf),
    /// A file with the stem exists but its envelope is another game's or unreadable.
    Mismatch { path: PathBuf, detail: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledZombiesMap {
    pub map: &'static ZombiesMap,
    pub zone: ZombiesZone,
}

impl InstalledZombiesMap {
    /// The `game:zone` key `make map` takes, for a map IW4L can read.
    pub fn map_key(&self) -> Option<String> {
        match (&self.zone, self.map.game) {
            (ZombiesZone::Found(_), BlackOps) => Some(format!("t5:{}", self.map.zone)),
            _ => None,
        }
    }
}

/// Envelope magic and version, or why the header is unreadable.
fn read_envelope(path: &Path) -> Result<([u8; 8], u32), String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    let magic = header[0..8].try_into().unwrap();
    Ok((magic, u32::from_le_bytes(header[8..12].try_into().unwrap())))
}

fn envelope_game(magic: &[u8; 8], version: u32) -> Option<ZombiesGame> {
    match (&magic[0..4], version) {
        (b"IWff", T4_ZONE_VERSION) => Some(WorldAtWar),
        (b"IWff", T5_ZONE_VERSION) => Some(BlackOps),
        (b"TAff", T6_ZONE_VERSION) => Some(BlackOps2),
        _ => None,
    }
}

fn describe_envelope(magic: &[u8; 8], version: u32) -> String {
    match envelope_game(magic, version) {
        Some(game) => format!("{} zone", game.label()),
        None if &magic[0..4] == b"IWff" && version == IW4_ZONE_VERSION => "MW2 zone".to_owned(),
        None => format!(
            "envelope {:?} version {version:#x}",
            String::from_utf8_lossy(magic)
        ),
    }
}

/// The catalog's maps present under the games root, catalog order. A stem found
/// with its own game's envelope wins over a same-named file of another game.
pub fn list_zombies_maps(root: &crate::GamesRoot) -> Vec<InstalledZombiesMap> {
    let mut found: BTreeMap<usize, ZombiesZone> = BTreeMap::new();
    for entry in game_files(&root.0) {
        let Ok(path) = entry else {
            continue;
        };
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ff"))
        {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let header = read_envelope(&path);
        for (index, map) in ZOMBIES_MAPS.iter().enumerate() {
            if !map.zone.eq_ignore_ascii_case(stem) {
                continue;
            }
            let zone = match &header {
                Ok((_, version)) if *version == map.game.zone_version() => {
                    ZombiesZone::Found(path.clone())
                }
                Ok((magic, version)) => ZombiesZone::Mismatch {
                    path: path.clone(),
                    detail: describe_envelope(magic, *version),
                },
                Err(error) => ZombiesZone::Mismatch {
                    path: path.clone(),
                    detail: error.clone(),
                },
            };
            match (found.get(&index), &zone) {
                (Some(ZombiesZone::Found(_)), _) => {}
                (Some(_), ZombiesZone::Mismatch { .. }) => {}
                _ => {
                    found.insert(index, zone);
                }
            }
        }
    }
    found
        .into_iter()
        .map(|(index, zone)| InstalledZombiesMap {
            map: &ZOMBIES_MAPS[index],
            zone,
        })
        .collect()
}

/// One line per game with what was found, then one per map that is present.
pub fn zombies_report(root: &crate::GamesRoot) -> Vec<String> {
    let installed = list_zombies_maps(root);
    let mut report = Vec::new();
    for game in [WorldAtWar, BlackOps, BlackOps2] {
        let maps: Vec<_> = installed.iter().filter(|m| m.map.game == game).collect();
        let total = ZOMBIES_MAPS.iter().filter(|m| m.game == game).count();
        let found = maps
            .iter()
            .filter(|m| matches!(m.zone, ZombiesZone::Found(_)))
            .count();
        let reader = if game.has_reader() {
            "reader present"
        } else {
            "no reader yet"
        };
        report.push(format!(
            "zombies {}: {found}/{total} maps found ({reader})",
            game.label()
        ));
        for map in maps {
            let line = match (&map.zone, map.map_key()) {
                (ZombiesZone::Found(_), Some(key)) => {
                    format!("  {} ({key})", map.map.title)
                }
                (ZombiesZone::Found(path), None) => {
                    format!("  {} at {}", map.map.title, path.display())
                }
                (ZombiesZone::Mismatch { path, detail }, _) => format!(
                    "  {}: {} is not a {} zone ({detail})",
                    map.map.title,
                    path.display(),
                    game.label()
                ),
            };
            report.push(line);
        }
    }
    report
}
