//! Reader for OpenStreetMap PBF extracts.
//!
//! Reads in two passes over the data blocks, each spread over threads:
//! the first keeps the ways and relations a caller's filters accept, the
//! second fetches coordinates (and, optionally, tags) of only the nodes those
//! ways reference. Memory therefore scales with the kept network, not with
//! the extract.
//!
//! Coordinates are kept as fixed-point integers in units of 1e-7 degrees,
//! OSM's own precision.

use super::protobuf::{packed_delta_sint64, packed_u64, Fields, Packed};
use std::borrow::Cow;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// An error while reading a PBF file.
#[derive(Debug)]
pub enum PbfError {
    Io(std::io::Error),
    Format(String),
    /// An error in the file at `path`.
    InFile {
        path: std::path::PathBuf,
        error: Box<PbfError>,
    },
}

impl fmt::Display for PbfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PbfError::Io(e) => write!(f, "cannot read the PBF file: {e}"),
            PbfError::Format(m) => write!(f, "malformed PBF file: {m}"),
            PbfError::InFile { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for PbfError {}

impl From<std::io::Error> for PbfError {
    fn from(e: std::io::Error) -> Self {
        PbfError::Io(e)
    }
}

impl From<super::protobuf::DecodeError> for PbfError {
    fn from(e: super::protobuf::DecodeError) -> Self {
        PbfError::Format(e.0)
    }
}

type Result<T> = std::result::Result<T, PbfError>;

fn format_err<T>(msg: impl Into<String>) -> Result<T> {
    Err(PbfError::Format(msg.into()))
}

/// Tags of an element, as `(key, value)` pairs in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags(pub Vec<(String, String)>);

impl Tags {
    /// Value of `key`, if present.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn from_refs(tags: &[(&str, &str)]) -> Self {
        Tags(
            tags.iter()
                .map(|&(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }
}

/// Value of `key` in a borrowed tag list, as handed to the filters.
pub fn tag<'a>(tags: &[(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    tags.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// An OSM way: its id, tags and node ids in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Way {
    pub id: i64,
    pub tags: Tags,
    pub nodes: Vec<i64>,
}

/// Kind of a relation member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Node,
    Way,
    Relation,
}

/// One member of a relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub kind: MemberKind,
    pub id: i64,
    pub role: String,
}

/// An OSM relation: its id, tags and members in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub id: i64,
    pub tags: Tags,
    pub members: Vec<Member>,
}

/// A borrowed tag filter.
pub type TagFilter<'f> = &'f (dyn Fn(&[(&str, &str)]) -> bool + Sync);

/// What to keep from the file.
pub struct ReadOptions<'f> {
    /// Ways to keep; their nodes are the only ones whose coordinates are read.
    pub way_filter: TagFilter<'f>,
    /// Relations to keep (e.g. turn restrictions). `None` keeps none.
    pub relation_filter: Option<TagFilter<'f>>,
    /// Tags of referenced nodes to keep (e.g. barriers). `None` keeps none.
    pub node_tag_filter: Option<TagFilter<'f>>,
    /// Nodes to report on their own, wherever they are (e.g. park-and-ride
    /// points); `None` reports none.
    pub poi_filter: Option<TagFilter<'f>>,
    /// Ways whose node references are counted (see [`Extract::ref_counts`]);
    /// `None` counts nothing.
    pub ref_count_filter: Option<TagFilter<'f>>,
    /// Worker threads; 0 means all available cores.
    pub n_threads: usize,
}

/// The part of an extract a caller asked for.
#[derive(Debug, Clone, Default)]
pub struct Extract {
    pub ways: Vec<Way>,
    pub relations: Vec<Relation>,
    /// Ids of every node referenced by a kept way, sorted and unique.
    pub node_ids: Vec<i64>,
    /// Latitude of `node_ids[i]` in 1e-7 degrees; `i32::MIN` when the file
    /// does not contain the node (a way clipped at the extract boundary).
    pub lat: Vec<i32>,
    /// Longitude of `node_ids[i]` in 1e-7 degrees; `i32::MIN` when missing.
    pub lon: Vec<i32>,
    /// Tags of referenced nodes accepted by `node_tag_filter`, by node id.
    pub node_tags: Vec<(i64, Tags)>,
    /// How many times `node_ids[i]` is referenced by the ways accepted by
    /// `ref_count_filter` (kept or not), repeats within a way included;
    /// empty when there is no such filter. A count of 2 or more marks an
    /// intersection.
    pub ref_counts: Vec<u32>,
    /// Nodes accepted by `poi_filter`: id, latitude and longitude (1e-7
    /// degrees) and tags, sorted by id.
    pub pois: Vec<(i64, i32, i32, Tags)>,
    /// Node ids that are members of a kept relation and exist in the file,
    /// sorted.
    pub member_nodes_present: Vec<i64>,
    /// Way ids that are members of a kept relation and exist in the file
    /// (kept by `way_filter` or not), sorted.
    pub member_ways_present: Vec<i64>,
}

impl Extract {
    /// Index of `id` in `node_ids`.
    pub fn node_index(&self, id: i64) -> Option<usize> {
        self.node_ids.binary_search(&id).ok()
    }
}

/// Read an extract from a `.osm.pbf` file. Errors name the file.
pub fn read_pbf_file(path: impl AsRef<std::path::Path>, opts: &ReadOptions) -> Result<Extract> {
    let path = path.as_ref();
    std::fs::read(path)
        .map_err(PbfError::Io)
        .and_then(|bytes| read_pbf(&bytes, opts))
        .map_err(|error| PbfError::InFile {
            path: path.to_path_buf(),
            error: Box::new(error),
        })
}

/// Read an extract from the bytes of a `.osm.pbf` file.
pub fn read_pbf(bytes: &[u8], opts: &ReadOptions) -> Result<Extract> {
    let blobs = split_blobs(bytes)?;
    let n_threads = resolve_threads(opts.n_threads);

    // Pass 1: ways and relations.
    let parts = par_map(&blobs, n_threads, |blob| {
        let block = inflate(blob)?;
        let mut ways = Vec::new();
        let mut relations = Vec::new();
        decode_block(&block, &mut |item| match item {
            Item::Way { id, tags, refs } => {
                if (opts.way_filter)(tags) && refs.len() >= 2 {
                    ways.push(Way {
                        id,
                        tags: Tags::from_refs(tags),
                        nodes: refs,
                    });
                }
            }
            Item::Relation { id, tags, members } => {
                if let Some(f) = opts.relation_filter {
                    if f(tags) {
                        relations.push(Relation {
                            id,
                            tags: Tags::from_refs(tags),
                            members,
                        });
                    }
                }
            }
            Item::Node { .. } => {}
        })?;
        Ok((ways, relations))
    })?;
    let mut ex = Extract::default();
    for (w, r) in parts {
        ex.ways.extend(w);
        ex.relations.extend(r);
    }

    // Pass 2: coordinates of the referenced nodes.
    let mut ids: Vec<i64> = ex
        .ways
        .iter()
        .flat_map(|w| w.nodes.iter().copied())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let ids_ref = &ids;
    let counts: Vec<AtomicU32> = match opts.ref_count_filter {
        Some(_) => (0..ids.len()).map(|_| AtomicU32::new(0)).collect(),
        None => Vec::new(),
    };
    let counts_ref = &counts;
    let member_ids = |kind: MemberKind| {
        let mut v: Vec<i64> = ex
            .relations
            .iter()
            .flat_map(|r| r.members.iter())
            .filter(|m| m.kind == kind)
            .map(|m| m.id)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let member_nodes = member_ids(MemberKind::Node);
    let member_ways = member_ids(MemberKind::Way);
    let node_seen: Vec<AtomicBool> = member_nodes
        .iter()
        .map(|_| AtomicBool::new(false))
        .collect();
    let way_seen: Vec<AtomicBool> = member_ways.iter().map(|_| AtomicBool::new(false)).collect();
    let (member_nodes_ref, member_ways_ref) = (&member_nodes, &member_ways);
    let (node_seen_ref, way_seen_ref) = (&node_seen, &way_seen);
    let found = par_map(&blobs, n_threads, |blob| {
        let block = inflate(blob)?;
        let mut coords: Vec<(u32, i32, i32)> = Vec::new();
        let mut tagged: Vec<(i64, Tags)> = Vec::new();
        let mut pois: Vec<(i64, i32, i32, Tags)> = Vec::new();
        decode_block(&block, &mut |item| match item {
            Item::Node { id, lat, lon, tags } => {
                if let Some(f) = opts.poi_filter {
                    if !tags.is_empty() && f(tags) {
                        pois.push((id, lat, lon, Tags::from_refs(tags)));
                    }
                }
                if let Ok(i) = member_nodes_ref.binary_search(&id) {
                    node_seen_ref[i].store(true, Ordering::Relaxed);
                }
                if let Ok(i) = ids_ref.binary_search(&id) {
                    coords.push((i as u32, lat, lon));
                    if let Some(f) = opts.node_tag_filter {
                        if !tags.is_empty() && f(tags) {
                            tagged.push((id, Tags::from_refs(tags)));
                        }
                    }
                }
            }
            Item::Way { id, tags, refs } => {
                if let Ok(i) = member_ways_ref.binary_search(&id) {
                    way_seen_ref[i].store(true, Ordering::Relaxed);
                }
                if let Some(f) = opts.ref_count_filter {
                    if f(tags) {
                        for r in refs {
                            if let Ok(i) = ids_ref.binary_search(&r) {
                                counts_ref[i].fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
            }
            Item::Relation { .. } => {}
        })?;
        Ok((coords, tagged, pois))
    })?;
    ex.lat = vec![i32::MIN; ids.len()];
    ex.lon = vec![i32::MIN; ids.len()];
    for (coords, tagged, pois) in found {
        ex.pois.extend(pois);
        for (i, lat, lon) in coords {
            ex.lat[i as usize] = lat;
            ex.lon[i as usize] = lon;
        }
        ex.node_tags.extend(tagged);
    }
    ex.node_tags.sort_by_key(|(id, _)| *id);
    ex.pois.sort_by_key(|p| p.0);
    ex.ref_counts = counts.into_iter().map(AtomicU32::into_inner).collect();
    let present = |ids: &[i64], seen: Vec<AtomicBool>| -> Vec<i64> {
        ids.iter()
            .zip(seen)
            .filter(|(_, s)| s.load(Ordering::Relaxed))
            .map(|(&id, _)| id)
            .collect()
    };
    ex.member_nodes_present = present(&member_nodes, node_seen);
    ex.member_ways_present = present(&member_ways, way_seen);
    ex.node_ids = ids;
    Ok(ex)
}

fn resolve_threads(n: usize) -> usize {
    if n > 0 {
        n
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    }
}

/// Split the file into the payloads of its `OSMData` blobs, checking the
/// header's required features on the way.
fn split_blobs(bytes: &[u8]) -> Result<Vec<&[u8]>> {
    // `bytes[pos..pos + n]` when it exists; sizes come from the file.
    let span = |pos: usize, n: usize| pos.checked_add(n).and_then(|end| bytes.get(pos..end));
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let Some(len_bytes) = span(pos, 4) else {
            return format_err("truncated blob header length");
        };
        let header_len = u32::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
        if header_len > MAX_HEADER_SIZE {
            return format_err("blob header larger than 64 KiB");
        }
        pos += 4;
        let Some(header) = span(pos, header_len) else {
            return format_err("truncated blob header");
        };
        pos += header_len;
        let mut kind: &[u8] = b"";
        let mut data_size = None;
        for f in Fields::new(header) {
            match f? {
                (1, v) => kind = v.as_bytes()?,
                (3, v) => data_size = Some(v.as_u64()?),
                _ => {}
            }
        }
        let Some(data_size) = data_size else {
            return format_err("blob header without datasize");
        };
        let blob = usize::try_from(data_size).ok().and_then(|n| span(pos, n));
        let Some(blob) = blob else {
            return format_err(format!(
                "blob of {data_size} bytes runs past the end of the file"
            ));
        };
        pos += blob.len();
        match kind {
            b"OSMHeader" => check_header(&inflate(blob)?)?,
            b"OSMData" => out.push(blob),
            _ => {} // unknown blob types are skipped, as the format allows
        }
    }
    Ok(out)
}

fn check_header(block: &[u8]) -> Result<()> {
    for f in Fields::new(block) {
        if let (4, v) = f? {
            let feature = String::from_utf8_lossy(v.as_bytes()?);
            if !matches!(feature.as_ref(), "OsmSchema-V0.6" | "DenseNodes") {
                return format_err(format!("unsupported required feature '{feature}'"));
            }
        }
    }
    Ok(())
}

/// Largest uncompressed block the PBF specification allows.
const MAX_BLOCK_SIZE: usize = 32 << 20;
/// Largest blob header the PBF specification allows.
const MAX_HEADER_SIZE: usize = 64 << 10;

/// Decompress a `Blob` message into the block it carries. Sizes are bounded
/// by the specification, so a hostile file cannot ask for a huge allocation.
fn inflate(blob: &[u8]) -> Result<Cow<'_, [u8]>> {
    let mut raw_size = None;
    for f in Fields::new(blob) {
        match f? {
            (1, v) => {
                let raw = v.as_bytes()?;
                if raw.len() > MAX_BLOCK_SIZE {
                    return format_err("raw blob larger than 32 MiB");
                }
                return Ok(Cow::Borrowed(raw));
            }
            (2, v) => {
                let n = v.as_u64()?;
                if n > MAX_BLOCK_SIZE as u64 {
                    return format_err(format!("blob raw_size {n} exceeds 32 MiB"));
                }
                raw_size = Some(n as usize);
            }
            (3, v) => {
                let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
                    v.as_bytes()?,
                    raw_size.unwrap_or(MAX_BLOCK_SIZE),
                )
                .map_err(|e| PbfError::Format(format!("zlib: {e:?}")))?;
                if raw_size.is_some_and(|n| n != out.len()) {
                    return format_err("decompressed blob size differs from raw_size");
                }
                return Ok(Cow::Owned(out));
            }
            (4 | 6 | 7, _) => {
                return format_err("only raw and zlib blobs are supported (found lzma/lz4/zstd)")
            }
            _ => {}
        }
    }
    format_err("blob without data")
}

/// One element of a primitive block, with tags borrowed from its string table.
enum Item<'s, 't> {
    Node {
        id: i64,
        lat: i32,
        lon: i32,
        tags: &'t [(&'s str, &'s str)],
    },
    Way {
        id: i64,
        tags: &'t [(&'s str, &'s str)],
        refs: Vec<i64>,
    },
    Relation {
        id: i64,
        tags: &'t [(&'s str, &'s str)],
        members: Vec<Member>,
    },
}

struct BlockParams {
    granularity: i64,
    lat_offset: i64,
    lon_offset: i64,
}

impl BlockParams {
    /// Fixed-point 1e-7 degrees from a raw coordinate.
    #[inline]
    fn lat(&self, raw: i64) -> Result<i32> {
        coordinate(self.lat_offset, self.granularity, raw)
    }

    #[inline]
    fn lon(&self, raw: i64) -> Result<i32> {
        coordinate(self.lon_offset, self.granularity, raw)
    }
}

/// `offset + granularity * raw` nanodegrees in 1e-7 degrees; an error when
/// the file's values overflow or the result does not fit in 1e-7 degrees.
#[inline]
fn coordinate(offset: i64, granularity: i64, raw: i64) -> Result<i32> {
    granularity
        .checked_mul(raw)
        .and_then(|d| d.checked_add(offset))
        .and_then(to_e7)
        .ok_or_else(|| {
            PbfError::Format(format!(
                "coordinate out of range (offset {offset}, granularity {granularity}, value {raw})"
            ))
        })
}

/// Nanodegrees to 1e-7 degrees, rounded half away from zero.
#[inline]
fn to_e7(nano: i64) -> Option<i32> {
    let q = if nano >= 0 {
        nano.checked_add(50)? / 100
    } else {
        nano.checked_sub(50)? / 100
    };
    i32::try_from(q).ok()
}

fn decode_block(block: &[u8], visit: &mut dyn FnMut(Item)) -> Result<()> {
    let mut strings: Vec<Cow<str>> = Vec::new();
    let mut groups: Vec<&[u8]> = Vec::new();
    let mut params = BlockParams {
        granularity: 100,
        lat_offset: 0,
        lon_offset: 0,
    };
    for f in Fields::new(block) {
        match f? {
            (1, v) => {
                for s in Fields::new(v.as_bytes()?) {
                    if let (1, sv) = s? {
                        strings.push(String::from_utf8_lossy(sv.as_bytes()?));
                    }
                }
            }
            (2, v) => groups.push(v.as_bytes()?),
            (17, v) => params.granularity = v.as_i64()?,
            (19, v) => params.lat_offset = v.as_i64()?,
            (20, v) => params.lon_offset = v.as_i64()?,
            _ => {}
        }
    }
    if params.granularity <= 0 {
        return format_err(format!(
            "block granularity {} is not positive",
            params.granularity
        ));
    }
    let st = |i: u64| -> Result<&str> {
        strings
            .get(i as usize)
            .map(|s| s.as_ref())
            .ok_or_else(|| PbfError::Format(format!("string index {i} out of range")))
    };
    let mut tags: Vec<(&str, &str)> = Vec::new();
    for group in groups {
        for f in Fields::new(group) {
            let (field, v) = f?;
            let msg = match v {
                super::protobuf::Value::Bytes(b) => b,
                _ => continue,
            };
            match field {
                1 => decode_node(msg, &params, &st, &mut tags, visit)?,
                2 => decode_dense(msg, &params, &st, &mut tags, visit)?,
                3 => decode_way(msg, &st, &mut tags, visit)?,
                4 => decode_relation(msg, &st, &mut tags, visit)?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn keys_vals<'s>(
    keys: &[u64],
    vals: &[u64],
    st: &dyn Fn(u64) -> Result<&'s str>,
    tags: &mut Vec<(&'s str, &'s str)>,
) -> Result<()> {
    if keys.len() != vals.len() {
        return format_err("keys and vals of different lengths");
    }
    tags.clear();
    for (&k, &v) in keys.iter().zip(vals) {
        tags.push((st(k)?, st(v)?));
    }
    Ok(())
}

fn decode_node<'s>(
    msg: &[u8],
    p: &BlockParams,
    st: &dyn Fn(u64) -> Result<&'s str>,
    tags: &mut Vec<(&'s str, &'s str)>,
    visit: &mut dyn FnMut(Item),
) -> Result<()> {
    let (mut id, mut lat, mut lon) = (0, 0, 0);
    let (mut keys, mut vals) = (Vec::new(), Vec::new());
    for f in Fields::new(msg) {
        match f? {
            (1, v) => id = v.as_sint64()?,
            (2, v) => keys = packed_u64(v.as_bytes()?)?,
            (3, v) => vals = packed_u64(v.as_bytes()?)?,
            (8, v) => lat = v.as_sint64()?,
            (9, v) => lon = v.as_sint64()?,
            _ => {}
        }
    }
    keys_vals(&keys, &vals, st, tags)?;
    visit(Item::Node {
        id,
        lat: p.lat(lat)?,
        lon: p.lon(lon)?,
        tags,
    });
    Ok(())
}

fn decode_dense<'s>(
    msg: &[u8],
    p: &BlockParams,
    st: &dyn Fn(u64) -> Result<&'s str>,
    tags: &mut Vec<(&'s str, &'s str)>,
    visit: &mut dyn FnMut(Item),
) -> Result<()> {
    let (mut ids, mut lats, mut lons): (&[u8], &[u8], &[u8]) = (&[], &[], &[]);
    let mut kv: &[u8] = &[];
    for f in Fields::new(msg) {
        match f? {
            (1, v) => ids = v.as_bytes()?,
            (8, v) => lats = v.as_bytes()?,
            (9, v) => lons = v.as_bytes()?,
            (10, v) => kv = v.as_bytes()?,
            _ => {}
        }
    }
    let ids = packed_delta_sint64(ids)?;
    let lats = packed_delta_sint64(lats)?;
    let lons = packed_delta_sint64(lons)?;
    if lats.len() != ids.len() || lons.len() != ids.len() {
        return format_err("dense nodes with mismatched id/lat/lon lengths");
    }
    // keys_vals: for each node, (key, val)* then a 0; empty when no node has tags.
    let has_tags = !kv.is_empty();
    let mut kv = Packed::new(kv);
    for i in 0..ids.len() {
        tags.clear();
        if has_tags {
            loop {
                match kv.next() {
                    None | Some(Ok(0)) => break,
                    Some(Ok(k)) => {
                        let v = match kv.next() {
                            Some(r) => r?,
                            None => return format_err("dense keys_vals ends inside a pair"),
                        };
                        tags.push((st(k)?, st(v)?));
                    }
                    Some(Err(e)) => return Err(e.into()),
                }
            }
        }
        visit(Item::Node {
            id: ids[i],
            lat: p.lat(lats[i])?,
            lon: p.lon(lons[i])?,
            tags,
        });
    }
    Ok(())
}

fn decode_way<'s>(
    msg: &[u8],
    st: &dyn Fn(u64) -> Result<&'s str>,
    tags: &mut Vec<(&'s str, &'s str)>,
    visit: &mut dyn FnMut(Item),
) -> Result<()> {
    let mut id = 0;
    let (mut keys, mut vals, mut refs) = (Vec::new(), Vec::new(), Vec::new());
    for f in Fields::new(msg) {
        match f? {
            (1, v) => id = v.as_i64()?,
            (2, v) => keys = packed_u64(v.as_bytes()?)?,
            (3, v) => vals = packed_u64(v.as_bytes()?)?,
            (8, v) => refs = packed_delta_sint64(v.as_bytes()?)?,
            _ => {}
        }
    }
    keys_vals(&keys, &vals, st, tags)?;
    visit(Item::Way { id, tags, refs });
    Ok(())
}

fn decode_relation<'s>(
    msg: &[u8],
    st: &dyn Fn(u64) -> Result<&'s str>,
    tags: &mut Vec<(&'s str, &'s str)>,
    visit: &mut dyn FnMut(Item),
) -> Result<()> {
    let mut id = 0;
    let (mut keys, mut vals) = (Vec::new(), Vec::new());
    let (mut roles, mut memids, mut types) = (Vec::new(), Vec::new(), Vec::new());
    for f in Fields::new(msg) {
        match f? {
            (1, v) => id = v.as_i64()?,
            (2, v) => keys = packed_u64(v.as_bytes()?)?,
            (3, v) => vals = packed_u64(v.as_bytes()?)?,
            (8, v) => roles = packed_u64(v.as_bytes()?)?,
            (9, v) => memids = packed_delta_sint64(v.as_bytes()?)?,
            (10, v) => types = packed_u64(v.as_bytes()?)?,
            _ => {}
        }
    }
    if roles.len() != memids.len() || types.len() != memids.len() {
        return format_err("relation members with mismatched lengths");
    }
    let mut members = Vec::with_capacity(memids.len());
    for i in 0..memids.len() {
        let kind = match types[i] {
            0 => MemberKind::Node,
            1 => MemberKind::Way,
            2 => MemberKind::Relation,
            t => return format_err(format!("unknown member type {t}")),
        };
        members.push(Member {
            kind,
            id: memids[i],
            role: st(roles[i])?.to_string(),
        });
    }
    keys_vals(&keys, &vals, st, tags)?;
    visit(Item::Relation { id, tags, members });
    Ok(())
}

/// Map `f` over `items` on `n_threads` threads, keeping the input order and
/// returning the first error.
fn par_map<I: Sync, T: Send>(
    items: &[I],
    n_threads: usize,
    f: impl Fn(&I) -> Result<T> + Sync,
) -> Result<Vec<T>> {
    let n_threads = n_threads.clamp(1, items.len().max(1));
    if n_threads == 1 {
        return items.iter().map(&f).collect();
    }
    let next = AtomicUsize::new(0);
    let mut chunks: Vec<Vec<(usize, Result<T>)>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= items.len() {
                            break;
                        }
                        out.push((i, f(&items[i])));
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("PBF worker thread panicked"))
            .collect()
    });
    let mut all: Vec<(usize, Result<T>)> = chunks.drain(..).flatten().collect();
    all.sort_unstable_by_key(|(i, _)| *i);
    all.into_iter().map(|(_, r)| r).collect()
}
