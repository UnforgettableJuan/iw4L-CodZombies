use std::fmt::Write as _;
use std::path::Path;

use crate::entities::by_count;
use crate::t5::{Placement, T5Inventory, type_name, walkable};

pub const PREFIX: &str = "inspect:";

pub fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "'"))
}

pub fn placement_name(placement: Placement) -> &'static str {
    match placement {
        Placement::Inline => "inline",
        Placement::Shared => "shared",
        Placement::Null => "null",
    }
}

fn type_walkable(raw: u32) -> &'static str {
    match fastfile_t5::AssetType::from_u32(raw) {
        Some(ty) if walkable(ty) => "yes",
        Some(_) => "no",
        None => "unknown_type",
    }
}

pub fn summary(inventory: &T5Inventory, names: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let h = &inventory.header;
    lines.push(format!(
        "{PREFIX} xfile image_bytes={} size={} external_size={} blocks={}",
        inventory.image_bytes,
        h.size,
        h.external_size,
        h.block_size
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    ));
    lines.push(format!(
        "{PREFIX} table assets={} script_strings={}",
        inventory.assets, inventory.script_strings
    ));
    for (raw, count) in &inventory.types {
        lines.push(format!(
            "{PREFIX} type name={} id={raw:#x} listed={} inline={} shared={} null={} walked={} nested={} walkable={}",
            type_name(*raw),
            count.listed,
            count.inline,
            count.shared,
            count.null,
            count.walked,
            count.nested,
            type_walkable(*raw),
        ));
    }
    match inventory.predicted_stop {
        Some((index, raw)) => lines.push(format!(
            "{PREFIX} predict first_unwalkable index={index} type={}",
            type_name(raw)
        )),
        None => lines.push(format!("{PREFIX} predict first_unwalkable=none")),
    }
    if let Some(stop) = &inventory.stop {
        lines.push(format!(
            "{PREFIX} stop index={} type={} placement={} cursor={:#x} cursor_after={:#x} reason={}",
            stop.index,
            type_name(stop.raw_type),
            placement_name(stop.placement),
            stop.cursor,
            stop.cursor_after,
            quoted(&stop.reason)
        ));
        if !stop.last_named.is_empty() {
            let named: Vec<String> = stop
                .last_named
                .iter()
                .map(|(label, name)| format!("{label}={}", quoted(name)))
                .collect();
            lines.push(format!(
                "{PREFIX} stop_context parsed_before_stop {}",
                named.join(" ")
            ));
        }
        lines.push(format!(
            "{PREFIX} stop_detail unsettled_offsets={} first_unsettled={} next_bytes={}",
            stop.unsettled_offsets,
            quoted(stop.first_unsettled.as_deref().unwrap_or("none")),
            if stop.next_bytes.is_empty() {
                "none"
            } else {
                &stop.next_bytes
            }
        ));
    }
    let w = &inventory.world;
    lines.push(format!(
        "{PREFIX} world clip_map={} gfx_world={} com_world={} map_ents_chars={} light_defs={}",
        quoted(w.clip_map.as_deref().unwrap_or("none")),
        w.gfx_world,
        w.com_world,
        w.map_ents_chars
            .map_or_else(|| String::from("none"), |n| n.to_string()),
        w.light_defs
    ));
    lines.push(format!(
        "{PREFIX} captured raw_files={} localized_strings={} named_types={}",
        inventory.raw_files.len(),
        inventory.localized_strings,
        inventory.names.len()
    ));
    if let Some(entities) = &inventory.entities {
        lines.push(format!(
            "{PREFIX} entities count={} classnames={} targetnames={}",
            entities.entities,
            entities.classnames.len(),
            entities.targetnames.len()
        ));
        for (name, count) in by_count(&entities.classnames) {
            lines.push(format!(
                "{PREFIX} class name={} count={count}",
                quoted(name)
            ));
        }
    }
    if names {
        lines.extend(name_lines(inventory));
    }
    lines
}

fn name_lines(inventory: &T5Inventory) -> Vec<String> {
    let mut lines = Vec::new();
    for (name, len) in &inventory.raw_files {
        lines.push(format!(
            "{PREFIX} name type=rawfile bytes={len} value={}",
            quoted(name)
        ));
    }
    for (ty, names) in &inventory.names {
        for name in names {
            lines.push(format!("{PREFIX} name type={ty} value={}", quoted(name)));
        }
    }
    if let Some(entities) = &inventory.entities {
        for (name, count) in by_count(&entities.targetnames) {
            lines.push(format!(
                "{PREFIX} targetname value={} count={count}",
                quoted(name)
            ));
        }
        for (name, count) in by_count(&entities.keys) {
            lines.push(format!(
                "{PREFIX} entity_key value={} count={count}",
                quoted(name)
            ));
        }
    }
    lines
}

pub fn full_report(
    path: &Path,
    head: &[String],
    inventory: Option<&T5Inventory>,
    tail: &[String],
) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "# iw4l inspect-zone report");
    let _ = writeln!(text, "# file: {}", path.display());
    for line in head {
        let _ = writeln!(text, "{line}");
    }
    if let Some(inventory) = inventory {
        for line in summary(inventory, true) {
            let _ = writeln!(text, "{line}");
        }
    }
    for line in tail {
        let _ = writeln!(text, "{line}");
    }
    text
}
