use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use asset_transport::T5ZoneMemory;
use fastfile_t5::{
    AssetLinkSink, AssetType, FxEffectDefGeometry, Ptr, XASSET_ENTRY_LEN, XAnimPartsGeometry,
    XFILE_HEADER_LEN, ZoneError, ZoneHeader, ZonePtr, ZoneStream,
};

use crate::entities::{EntityCensus, census};

pub const DECLARED_BLOCKS_CAP: u64 = 6 << 30;

const POOL_IDS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Inline,
    Shared,
    Null,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeCount {
    pub listed: usize,
    pub inline: usize,
    pub shared: usize,
    pub null: usize,
    pub walked: usize,
    pub nested: usize,
    pub bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stop {
    pub index: usize,
    pub raw_type: u32,
    pub placement: Placement,
    pub cursor: usize,
    pub cursor_after: usize,
    pub reason: String,
    pub last_named: Vec<(&'static str, String)>,
    pub unsettled_offsets: usize,
    pub first_unsettled: Option<String>,
    pub next_bytes: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldParts {
    pub clip_map: Option<String>,
    pub gfx_world: bool,
    pub com_world: bool,
    pub map_ents_chars: Option<usize>,
    pub light_defs: usize,
}

#[derive(Clone, Debug)]
pub struct T5Inventory {
    pub header: ZoneHeader,
    pub image_bytes: usize,
    pub script_strings: usize,
    pub assets: usize,
    pub types: BTreeMap<u32, TypeCount>,
    pub predicted_stop: Option<(usize, u32)>,
    pub processed: usize,
    pub stop: Option<Stop>,
    pub trailing_bytes: Option<usize>,
    pub world: WorldParts,
    pub entities: Option<EntityCensus>,
    pub names: BTreeMap<&'static str, BTreeSet<String>>,
    pub raw_files: BTreeMap<String, usize>,
    pub localized_strings: usize,
}

impl T5Inventory {
    pub fn complete(&self) -> bool {
        self.stop.is_none()
    }
}

pub fn type_name(raw: u32) -> String {
    AssetType::from_u32(raw).map_or_else(|| format!("unknown_{raw:#x}"), |ty| ty.name().to_owned())
}

pub fn walkable(ty: AssetType) -> bool {
    static TABLE: OnceLock<[bool; POOL_IDS]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        std::array::from_fn(|raw| {
            u32::try_from(raw)
                .ok()
                .and_then(AssetType::from_u32)
                .is_some_and(probe_walk)
        })
    });
    table.get(ty as usize).copied().unwrap_or(false)
}

fn probe_walk(ty: AssetType) -> bool {
    let image = [0u8; XFILE_HEADER_LEN];
    let Ok(header) = fastfile_t5::parse_zone_header(&image) else {
        return false;
    };
    let mut memory = T5ZoneMemory::for_header(&header);
    let Ok(mut s) = memory.stream(&image) else {
        return false;
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        fastfile_t5::load_asset_body(&mut s, ty)
    }));
    !matches!(outcome, Ok(Err(ZoneError::NoAssetLoader(_))))
}

pub fn inventory_t5_image(image: &[u8]) -> Result<T5Inventory, String> {
    let header =
        fastfile_t5::parse_zone_header(image).map_err(|error| format!("zone header: {error}"))?;
    let declared: u64 = header.block_size.iter().map(|&b| u64::from(b)).sum();
    if declared > DECLARED_BLOCKS_CAP {
        return Err(format!(
            "zone header declares {declared} bytes of blocks, over the {DECLARED_BLOCKS_CAP} byte cap; not allocating"
        ));
    }
    let mut memory = T5ZoneMemory::for_header(&header);
    let mut s = memory
        .stream(image)
        .map_err(|error| format!("zone stream: {error}"))?;
    let table =
        fastfile_t5::open_asset_table(&mut s).map_err(|error| format!("asset list: {error}"))?;

    let mut inventory = T5Inventory {
        header,
        image_bytes: image.len(),
        script_strings: table.strings.count(),
        assets: table.count(),
        types: BTreeMap::new(),
        predicted_stop: None,
        processed: 0,
        stop: None,
        trailing_bytes: None,
        world: WorldParts::default(),
        entities: None,
        names: BTreeMap::new(),
        raw_files: BTreeMap::new(),
        localized_strings: 0,
    };

    let mut entries = Vec::with_capacity(table.count());
    if let Some(array) = table.array() {
        for index in 0..table.count() {
            let at = index * XASSET_ENTRY_LEN;
            let raw = s
                .u32_at(array, at)
                .map_err(|error| format!("asset list entry {index}: {error}"))?;
            let placement = match s
                .ptr_at(array, at + 4)
                .map_err(|error| format!("asset list entry {index}: {error}"))?
            {
                ZonePtr::Null => Placement::Null,
                ZonePtr::Offset(_) => Placement::Shared,
                ZonePtr::Following | ZonePtr::Insert => Placement::Inline,
            };
            let count = inventory.types.entry(raw).or_default();
            count.listed += 1;
            match placement {
                Placement::Inline => count.inline += 1,
                Placement::Shared => count.shared += 1,
                Placement::Null => count.null += 1,
            }
            if inventory.predicted_stop.is_none()
                && placement == Placement::Inline
                && !AssetType::from_u32(raw).is_some_and(walkable)
            {
                inventory.predicted_stop = Some((index, raw));
            }
            entries.push((raw, placement));
        }
    }

    let mut sink = InventorySink::default();
    for (index, &(raw, placement)) in entries.iter().enumerate() {
        let cursor = s.cursor();
        let walked = match (AssetType::from_u32(raw), table.slot(index)) {
            (Some(ty), Some(slot)) => catch_unwind(AssertUnwindSafe(|| {
                fastfile_t5::load_asset_at_observed(&mut s, ty, slot, &mut sink)
            }))
            .map_err(|panic| format!("panic: {}", panic_text(panic.as_ref())))
            .and_then(|result| result.map_err(|error| error.to_string())),
            (None, _) => Err(ZoneError::UnknownAssetType(raw).to_string()),
            (Some(_), None) => Err(ZoneError::NoBlockPushed.to_string()),
        };
        match walked {
            Ok(body) => {
                inventory.processed += 1;
                if body {
                    let count = inventory.types.entry(raw).or_default();
                    count.walked += 1;
                    count.bytes += s.cursor().saturating_sub(cursor);
                }
            }
            Err(reason) => {
                inventory.stop = Some(stop_at(&s, image, index, raw, placement, cursor, reason));
                break;
            }
        }
    }
    if inventory.stop.is_none() {
        match s.pop() {
            Ok(()) => inventory.trailing_bytes = Some(s.remaining()),
            Err(error) => {
                let cursor = s.cursor();
                inventory.stop = Some(stop_at(
                    &s,
                    image,
                    entries.len(),
                    u32::MAX,
                    Placement::Inline,
                    cursor,
                    format!("closing the asset list: {error}"),
                ));
            }
        }
    }

    for (raw, count) in &mut inventory.types {
        let loaded = sink.loaded.get(raw).copied().unwrap_or(0);
        count.nested = loaded.saturating_sub(count.walked);
    }
    inventory.world = WorldParts {
        clip_map: s
            .clip_map()
            .map(|g| name_at(&s, g.name).unwrap_or_else(|| String::from("(unnamed)"))),
        gfx_world: s.gfx_world().is_some(),
        com_world: s.com_world().is_some(),
        map_ents_chars: s.map_ents().map(|g| g.entity_chars),
        light_defs: s.light_defs().len(),
    };
    inventory.entities = s.map_ents().and_then(|g| {
        let text = s.slice_at(g.entity_string?, 0, g.entity_chars).ok()?;
        Some(census(&String::from_utf8_lossy(text)))
    });
    inventory.names = sink.names;
    inventory.raw_files = sink.raw_files;
    inventory.localized_strings = sink.localized;
    Ok(inventory)
}

fn stop_at(
    s: &ZoneStream<'_>,
    image: &[u8],
    index: usize,
    raw_type: u32,
    placement: Placement,
    cursor: usize,
    reason: String,
) -> Stop {
    let last_named = [
        ("last_xmodel", s.latest_xmodel().and_then(|g| g.name)),
        ("last_material", s.latest_material().and_then(|g| g.name)),
        ("last_image", s.latest_image().and_then(|g| g.name)),
        (
            "last_techset",
            s.latest_technique_set().and_then(|g| g.name),
        ),
    ]
    .into_iter()
    .filter_map(|(label, name)| Some((label, name_at(s, name)?)))
    .collect();
    let next = image.get(cursor..).unwrap_or(&[]);
    Stop {
        index,
        raw_type,
        placement,
        cursor,
        cursor_after: s.cursor(),
        reason,
        last_named,
        unsettled_offsets: s.unsettled_offsets(),
        first_unsettled: s
            .first_unsettled()
            .map(|(ptr, a, b)| format!("block={} offset={:#x} {a} {b}", ptr.block, ptr.offset)),
        next_bytes: next.iter().take(32).map(|b| format!("{b:02x}")).collect(),
    }
}

fn name_at(s: &ZoneStream<'_>, name: Option<Ptr>) -> Option<String> {
    let text = s.cstr(name?).ok()?;
    (!text.is_empty()).then(|| text.to_owned())
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| String::from("non-text panic"))
}

#[derive(Default)]
struct InventorySink {
    loaded: BTreeMap<u32, usize>,
    names: BTreeMap<&'static str, BTreeSet<String>>,
    raw_files: BTreeMap<String, usize>,
    localized: usize,
}

impl InventorySink {
    fn note(&mut self, ty: AssetType, name: Option<String>) {
        if let Some(name) = name {
            self.names.entry(ty.name()).or_default().insert(name);
        }
    }
}

impl AssetLinkSink for InventorySink {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        _slot: Ptr,
        _insert_slot: Option<Ptr>,
    ) -> fastfile_t5::Result<()> {
        *self.loaded.entry(ty as u32).or_insert(0) += 1;
        let name = match ty {
            AssetType::XModel => s.latest_xmodel().and_then(|g| g.name),
            AssetType::Material => s.latest_material().and_then(|g| g.name),
            AssetType::Image => s.latest_image().and_then(|g| g.name),
            AssetType::TechniqueSet => s.latest_technique_set().and_then(|g| g.name),
            AssetType::Weapon => s.weapon().and_then(|g| g.name),
            AssetType::ClipMapSp | AssetType::ClipMapMp => s.clip_map().and_then(|g| g.name),
            _ => None,
        };
        self.note(ty, name_at(s, name));
        Ok(())
    }

    fn alias(&mut self, _ty: AssetType, _slot: Ptr, _target: Ptr) -> fastfile_t5::Result<()> {
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        _zlib_compressed: bool,
    ) -> fastfile_t5::Result<()> {
        let len = data.strip_suffix(&[0]).map_or(data.len(), <[u8]>::len);
        self.raw_files.insert(name.to_owned(), len);
        Ok(())
    }

    fn capture_localize(&mut self, _name: &str, _value: &[u8]) -> fastfile_t5::Result<()> {
        self.localized += 1;
        Ok(())
    }

    fn capture_string_table(&mut self, s: &ZoneStream<'_>, header: Ptr) -> fastfile_t5::Result<()> {
        if let Ok(ZonePtr::Offset(name)) = s.ptr_at(header, 0) {
            self.note(AssetType::StringTable, name_at(s, Some(name)));
        }
        Ok(())
    }

    fn capture_xanim(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: XAnimPartsGeometry,
    ) -> fastfile_t5::Result<()> {
        self.note(AssetType::XAnimParts, name_at(s, geometry.name));
        Ok(())
    }

    fn capture_fx(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: FxEffectDefGeometry,
    ) -> fastfile_t5::Result<()> {
        self.note(AssetType::Fx, name_at(s, geometry.name));
        Ok(())
    }
}
