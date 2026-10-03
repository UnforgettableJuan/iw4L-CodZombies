use std::io::Write as _;
use std::path::PathBuf;

use fastfile_t5::AssetType;

use super::*;

const FOLLOWING: u32 = u32::MAX;
const BLOCK_BYTES: u32 = 256;
const SHARED_IN_VIRTUAL_BLOCK: u32 = (4 << 29) + 1;

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn cstr(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn xfile(body: &[u8]) -> Vec<u8> {
    xfile_with_blocks(body, BLOCK_BYTES)
}

fn xfile_with_blocks(body: &[u8], block_bytes: u32) -> Vec<u8> {
    let mut image = words(&[u32::try_from(body.len()).unwrap(), 0]);
    image.extend(words(&[block_bytes; 7]));
    image.extend_from_slice(body);
    image
}

fn asset_list(entries: &[(AssetTypeId, u32)]) -> Vec<u8> {
    let mut body = words(&[0, 0, u32::try_from(entries.len()).unwrap(), FOLLOWING]);
    for (ty, pointer) in entries {
        body.extend(words(&[ty.0, *pointer]));
    }
    body
}

struct AssetTypeId(u32);

fn id(ty: AssetType) -> AssetTypeId {
    AssetTypeId(ty as u32)
}

fn localize(value: &str, name: &str) -> Vec<u8> {
    let mut body = words(&[FOLLOWING, FOLLOWING]);
    body.extend(cstr(value));
    body.extend(cstr(name));
    body
}

fn raw_file(name: &str, data: &str) -> Vec<u8> {
    let mut body = words(&[FOLLOWING, u32::try_from(data.len()).unwrap(), FOLLOWING]);
    body.extend(cstr(name));
    body.extend(cstr(data));
    body
}

fn complete_zone() -> Vec<u8> {
    let mut body = asset_list(&[
        (id(AssetType::Localize), FOLLOWING),
        (id(AssetType::RawFile), FOLLOWING),
        (id(AssetType::RawFile), 0),
        (id(AssetType::Localize), SHARED_IN_VIRTUAL_BLOCK),
    ]);
    body.extend(localize("Hello", "ZOMBIE_GREETING"));
    body.extend(raw_file("maps/zombie_test.gsc", "main(){}"));
    xfile(&body)
}

fn stopping_zone() -> Vec<u8> {
    let mut body = asset_list(&[
        (id(AssetType::Localize), FOLLOWING),
        (id(AssetType::UiMap), FOLLOWING),
        (id(AssetType::RawFile), FOLLOWING),
    ]);
    body.extend(localize("Hello", "ZOMBIE_GREETING"));
    body.extend(words(&[FOLLOWING; 8]));
    xfile(&body)
}

fn fastfile(image: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(image).unwrap();
    let mut file = b"IWffu100".to_vec();
    file.extend(fastfile_t5::ZONE_VERSION_PC.to_le_bytes());
    file.extend(encoder.finish().unwrap());
    file
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zone_inspect_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_to_string(args: &[&str], artifacts: Option<&Path>) -> (i32, String) {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let mut out = Vec::new();
    let code = run(&args, artifacts, &mut out);
    (code, String::from_utf8(out).unwrap())
}

#[test]
fn parses_commands() {
    let s = |v: &[&str]| v.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
    assert_eq!(parse(&s(&["--help"])), Ok(Command::Help));
    assert_eq!(parse(&s(&["--list"])), Ok(Command::List(None)));
    assert_eq!(
        parse(&s(&["--list", "zombie"])),
        Ok(Command::List(Some("zombie".to_owned())))
    );
    assert_eq!(
        parse(&s(&["t5:zombie_theater", "--names", "x.ff"])),
        Ok(Command::Inspect {
            targets: s(&["t5:zombie_theater", "x.ff"]),
            names: true
        })
    );
    assert!(parse(&s(&[])).is_err());
    assert!(parse(&s(&["--list", "a", "b"])).is_err());
    assert!(parse(&s(&["--list", "--names"])).is_err());
    assert!(parse(&s(&["--bogus", "x"])).is_err());
}

#[test]
fn walkable_agrees_with_the_parser() {
    let unwalkable: Vec<&str> = (0..=0x2a)
        .filter_map(AssetType::from_u32)
        .filter(|ty| !walkable(*ty))
        .map(AssetType::name)
        .collect();
    assert_eq!(
        unwalkable,
        [
            "ui_map",
            "menu",
            "weapondef",
            "weaponvariant",
            "aitype",
            "mptype",
            "mpbody",
            "mphead",
            "character",
            "xmodelalias",
            "packindex",
        ]
    );
    assert!(walkable(AssetType::RawFile));
    assert!(walkable(AssetType::GameWorldSp));
    assert!(walkable(AssetType::ClipMapSp));
}

#[test]
fn inventories_a_zone_that_walks_to_the_end() {
    let inventory = inventory_t5_image(&complete_zone()).unwrap();
    assert!(inventory.complete(), "{:?}", inventory.stop);
    assert_eq!(inventory.assets, 4);
    assert_eq!(inventory.processed, 4);
    assert_eq!(inventory.predicted_stop, None);
    assert_eq!(inventory.trailing_bytes, Some(0));

    let localize = &inventory.types[&(AssetType::Localize as u32)];
    assert_eq!(
        (
            localize.listed,
            localize.inline,
            localize.shared,
            localize.walked
        ),
        (2, 1, 1, 1)
    );
    let raw = &inventory.types[&(AssetType::RawFile as u32)];
    assert_eq!((raw.listed, raw.inline, raw.null, raw.walked), (2, 1, 1, 1));
    assert!(raw.bytes > 0);

    assert_eq!(inventory.localized_strings, 1);
    assert_eq!(
        inventory.raw_files.get("maps/zombie_test.gsc"),
        Some(&"main(){}".len())
    );
}

#[test]
fn stops_at_the_first_inline_type_without_a_loader() {
    let inventory = inventory_t5_image(&stopping_zone()).unwrap();
    assert_eq!(inventory.predicted_stop, Some((1, AssetType::UiMap as u32)));
    let stop = inventory.stop.as_ref().expect("walk must stop");
    assert_eq!(stop.index, 1);
    assert_eq!(stop.raw_type, AssetType::UiMap as u32);
    assert_eq!(stop.placement, Placement::Inline);
    assert!(stop.reason.contains("ui_map"), "{}", stop.reason);
    assert!(!stop.next_bytes.is_empty());
    assert_eq!(inventory.processed, 1);
    assert_eq!(inventory.types[&(AssetType::RawFile as u32)].walked, 0);
}

#[test]
fn a_techset_that_fails_midway_is_not_reported_as_parsed() {
    let mut body = asset_list(&[
        (id(AssetType::Localize), FOLLOWING),
        (id(AssetType::TechniqueSet), FOLLOWING),
    ]);
    body.extend(localize("Hello", "ZOMBIE_GREETING"));
    body.extend(words(&[FOLLOWING, 0, FOLLOWING]));
    body.extend(words(&[0; 129]));
    body.extend(cstr("ts_failing"));
    body.extend(words(&[0]));
    let inventory = inventory_t5_image(&xfile_with_blocks(&body, 1 << 16)).unwrap();
    let stop = inventory.stop.expect("walk must stop");
    assert_eq!(stop.raw_type, AssetType::TechniqueSet as u32);
    assert_eq!(stop.started_techset.as_deref(), Some("ts_failing"));
    assert!(
        stop.last_named.iter().all(|(_, name)| name != "ts_failing"),
        "{:?}",
        stop.last_named
    );
}

#[test]
fn stops_on_an_unknown_pool_id() {
    let mut body = asset_list(&[(AssetTypeId(0x50), FOLLOWING)]);
    body.extend(words(&[0; 4]));
    let inventory = inventory_t5_image(&xfile(&body)).unwrap();
    assert_eq!(inventory.predicted_stop, Some((0, 0x50)));
    let stop = inventory.stop.expect("walk must stop");
    assert!(
        stop.reason.contains("unknown asset pool id"),
        "{}",
        stop.reason
    );
    assert_eq!(type_name(0x50), "unknown_0x50");
}

#[test]
fn refuses_absurd_block_sizes() {
    let mut image = words(&[0, 0]);
    image.extend(words(&[u32::MAX; 7]));
    let error = inventory_t5_image(&image).unwrap_err();
    assert!(error.contains("not allocating"), "{error}");
}

#[test]
fn runs_end_to_end_on_a_fastfile_path() {
    let dir = scratch("end_to_end");
    let zone = dir.join("zombie_test.ff");
    std::fs::write(&zone, fastfile(&complete_zone())).unwrap();
    let artifacts = dir.join("artifacts");

    let (code, out) = run_to_string(&[zone.to_str().unwrap()], Some(&artifacts));
    assert_eq!(code, EXIT_COMPLETE, "{out}");
    assert!(
        out.contains("magic=IWffu100 version=0x1d9 game=t5"),
        "{out}"
    );
    assert!(out.contains("inspect: type name=localize"), "{out}");
    assert!(
        out.contains("status=complete processed=4 assets=4"),
        "{out}"
    );
    assert!(!out.contains("maps/zombie_test.gsc"), "{out}");

    let report = std::fs::read_to_string(artifacts.join("inspect/t5_zombie_test.txt")).unwrap();
    assert!(
        report.contains("value=\"maps/zombie_test.gsc\""),
        "{report}"
    );

    let (_, named) = run_to_string(&[zone.to_str().unwrap(), "--names"], None);
    assert!(
        named.contains("type=rawfile bytes=8 value=\"maps/zombie_test.gsc\""),
        "{named}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reports_a_stopped_walk_with_exit_one() {
    let dir = scratch("stopped");
    let zone = dir.join("zombie_stop.ff");
    std::fs::write(&zone, fastfile(&stopping_zone())).unwrap();
    let (code, out) = run_to_string(&[zone.to_str().unwrap()], None);
    assert_eq!(code, EXIT_STOPPED, "{out}");
    assert!(
        out.contains("inspect: stop index=1 type=ui_map placement=inline"),
        "{out}"
    );
    assert!(
        out.contains("predict first_unwalkable index=1 type=ui_map"),
        "{out}"
    );
    assert!(out.contains("status=stopped processed=1 assets=3"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn names_the_version_of_a_zone_it_cannot_read() {
    let dir = scratch("foreign");
    let zone = dir.join("nazi_zombie_prototype.ff");
    let mut bytes = b"IWffu100".to_vec();
    bytes.extend(0x183u32.to_le_bytes());
    std::fs::write(&zone, bytes).unwrap();
    let (code, out) = run_to_string(&[zone.to_str().unwrap()], None);
    assert_eq!(code, EXIT_STOPPED, "{out}");
    assert!(out.contains("version=0x183 game=unknown"), "{out}");
    assert!(out.contains("status=skipped"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejects_bad_arguments_with_usage() {
    let (code, out) = run_to_string(&["--bogus"], None);
    assert_eq!(code, EXIT_SETUP);
    assert!(out.contains("unknown flag"), "{out}");
    assert!(out.contains("usage: iw4l inspect-zone"), "{out}");
    let (code, out) = run_to_string(&["--help"], None);
    assert_eq!(code, EXIT_COMPLETE);
    assert!(out.starts_with("usage: iw4l inspect-zone"), "{out}");
}

fn header(version: u32) -> Vec<u8> {
    let mut bytes = b"IWffu100".to_vec();
    bytes.extend(version.to_le_bytes());
    bytes
}

fn games_tree(name: &str) -> PathBuf {
    let dir = scratch(name);
    let zone_dir = dir.join("Black Ops").join("zone").join("Common");
    std::fs::create_dir_all(&zone_dir).unwrap();
    std::fs::write(zone_dir.join("zombie_theater.ff"), header(0x1d9)).unwrap();
    std::fs::write(zone_dir.join("zombie_moon.ff"), header(0x1d9)).unwrap();
    std::fs::write(zone_dir.join("mp_nuked.ff"), header(0x1d9)).unwrap();
    std::fs::write(zone_dir.join("kowloon.ff"), header(0x1d9)).unwrap();
    std::fs::write(zone_dir.join("mp_kowloon.ff"), header(0x1d9)).unwrap();
    std::fs::write(dir.join("mp_rust.ff"), header(0x114)).unwrap();
    std::fs::write(zone_dir.join("readme.txt"), b"not a zone").unwrap();
    dir
}

fn resolved_keys(roots: &[GamesRoot], target: &str) -> Vec<String> {
    let mut roots = Roots(Some(Ok(roots.to_vec())));
    resolve(target, &mut roots)
        .into_iter()
        .map(|r| r.map_or_else(|e| format!("error: {e}"), |(key, _)| key))
        .collect()
}

fn listed(roots: &[GamesRoot], filter: Option<&str>) -> String {
    let mut out = Vec::new();
    assert_eq!(list(roots, filter, &mut out), EXIT_COMPLETE);
    String::from_utf8(out).unwrap()
}

#[test]
fn lists_zones_under_a_games_root() {
    let dir = games_tree("listing");
    let roots = [GamesRoot(dir.clone())];

    let out = listed(&roots, Some("zombie"));
    assert!(out.contains("key=t5:zombie_moon"), "{out}");
    assert!(out.contains("key=t5:zombie_theater"), "{out}");
    assert!(!out.contains("mp_nuked"), "{out}");
    assert!(out.contains("listed zones=2"), "{out}");

    let all = listed(&roots, None);
    assert!(all.contains("listed zones=6"), "{all}");
    assert!(
        all.contains("key=iw4:mp_rust magic=IWffu100 version=0x114"),
        "{all}"
    );

    let t5 = listed(&roots, Some("t5:"));
    assert!(t5.contains("listed zones=5"), "{t5}");
    assert!(!t5.contains("mp_rust"), "{t5}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resolves_names_and_patterns_across_roots() {
    let dir = games_tree("resolving");
    let nested = GamesRoot(dir.join("Black Ops"));
    let roots = [GamesRoot(dir.clone()), nested];

    assert_eq!(
        resolved_keys(&roots, "t5:*"),
        [
            "t5:kowloon",
            "t5:mp_kowloon",
            "t5:mp_nuked",
            "t5:zombie_moon",
            "t5:zombie_theater"
        ]
    );
    assert_eq!(resolved_keys(&roots, "t5:kowloon"), ["t5:kowloon"]);
    assert_eq!(
        resolved_keys(&roots, "ZOMBIE_THEATER"),
        ["t5:zombie_theater"]
    );
    assert_eq!(resolved_keys(&roots, "t5:nuked"), ["t5:nuked"]);
    assert!(resolved_keys(&roots, "iw4:zombie_*")[0].starts_with("error: no .ff matches"));
    assert!(resolved_keys(&roots, "t5:zombie_nowhere")[0].starts_with("error: no .ff matches"));
    assert_eq!(
        resolved_keys(&roots, "missing/zombie_theater.ff"),
        ["error: no such file: missing/zombie_theater.ff"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn report_names_are_file_safe_and_unique() {
    let mut used = HashSet::new();
    assert_eq!(
        report_file_name("t5:zombie_theater", &mut used),
        "t5_zombie_theater.txt"
    );
    assert_eq!(
        report_file_name("t5:zombie_theater", &mut used),
        "t5_zombie_theater-2.txt"
    );
    assert_eq!(
        report_file_name("unknown:a b/c", &mut used),
        "unknown_a_b_c.txt"
    );
}
