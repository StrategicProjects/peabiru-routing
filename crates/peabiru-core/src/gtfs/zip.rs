//! Minimal ZIP reader for GTFS feeds: stored and deflated entries, ZIP64
//! sizes and offsets (as `java.util.zip.ZipFile` reads them), no
//! encryption. Sizes are bounded so a hostile archive cannot ask for an
//! unbounded allocation.

use std::fmt;

/// Bounds on what an archive may expand to, against decompression bombs.
///
/// The defaults admit any entry a plain (non-ZIP64) archive can hold: the
/// `stop_times.txt` of a national feed is a few GB at most. Deflate cannot
/// expand data by more than about 1032:1, so an entry whose directory claims
/// a larger ratio is rejected whatever the limits.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Largest uncompressed entry, in bytes (default 4 GiB).
    pub max_entry: u64,
    /// Largest total of all entries read from one archive, in bytes
    /// (default 8 GiB).
    pub max_total: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_entry: 4 << 30,
            max_total: 8 << 30,
        }
    }
}

/// A size or offset field saturated at this value has its real value in
/// the ZIP64 extra field.
const ZIP64_MAGIC: u64 = 0xffff_ffff;

/// A saturated value the ZIP64 extra field does not give.
const UNRESOLVED: u64 = u64::MAX;

/// Deflate cannot expand data by more than about 1032:1; a larger declared
/// ratio means the directory lies.
const MAX_DEFLATE_RATIO: u64 = 1100;

#[derive(Debug)]
pub enum ZipError {
    Io(std::io::Error),
    Format(String),
}

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZipError::Io(e) => write!(f, "cannot read the zip file: {e}"),
            ZipError::Format(m) => write!(f, "malformed zip file: {m}"),
        }
    }
}

impl std::error::Error for ZipError {}

impl From<std::io::Error> for ZipError {
    fn from(e: std::io::Error) -> Self {
        ZipError::Io(e)
    }
}

fn err<T>(m: impl Into<String>) -> Result<T, ZipError> {
    Err(ZipError::Format(m.into()))
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, ZipError> {
    b.get(at..at.saturating_add(2))
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| ZipError::Format("truncated".into()))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, ZipError> {
    b.get(at..at.saturating_add(4))
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| ZipError::Format("truncated".into()))
}

fn u64_at(b: &[u8], at: usize) -> Result<u64, ZipError> {
    b.get(at..at.saturating_add(8))
        .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
        .ok_or_else(|| ZipError::Format("truncated".into()))
}

/// The ZIP64 end of central directory record named by the locator just
/// before the end record at `eocd`: entry count, directory size and
/// offset. As `ZipFile` does, it is used only when every value it gives
/// agrees with the end record's or replaces a saturated one.
fn zip64_end(bytes: &[u8], eocd: usize, n: u64, size: u64, offset: u64) -> Option<(u64, u64, u64)> {
    let loc = eocd.checked_sub(20)?;
    if u32_at(bytes, loc).ok()? != 0x0706_4b50 {
        return None;
    }
    let at = usize::try_from(u64_at(bytes, loc + 8).ok()?).ok()?;
    if u32_at(bytes, at).ok()? != 0x0606_4b50 {
        return None;
    }
    let n64 = u64_at(bytes, at.saturating_add(32)).ok()?;
    let size64 = u64_at(bytes, at.saturating_add(40)).ok()?;
    let offset64 = u64_at(bytes, at.saturating_add(48)).ok()?;
    let agrees = |v64: u64, v: u64, magic: u64| v64 == v || v == magic;
    (agrees(n64, n, 0xffff)
        && agrees(size64, size, ZIP64_MAGIC)
        && agrees(offset64, offset, ZIP64_MAGIC))
    .then_some((n64, size64, offset64))
}

/// Replace the saturated values among `fields` (uncompressed size,
/// compressed size, local header offset, in that order) with those of the
/// ZIP64 extra field (id 1) in `extra`, as `ZipFile` does. A value with no
/// counterpart becomes [`UNRESOLVED`].
fn apply_zip64_extra(extra: &[u8], fields: &mut [u64; 3]) {
    let wanted = fields.map(|f| f == ZIP64_MAGIC);
    for (f, &w) in fields.iter_mut().zip(&wanted) {
        if w {
            *f = UNRESOLVED;
        }
    }
    let mut p = 0;
    while p + 4 <= extra.len() {
        let id = u16::from_le_bytes([extra[p], extra[p + 1]]);
        let len = usize::from(u16::from_le_bytes([extra[p + 2], extra[p + 3]]));
        let Some(data) = extra.get(p + 4..p + 4 + len) else {
            return;
        };
        if id == 1 {
            let mut at = 0;
            for (f, _) in fields.iter_mut().zip(wanted).filter(|&(_, w)| w) {
                let Ok(v) = u64_at(data, at) else {
                    return;
                };
                *f = v;
                at += 8;
            }
            return;
        }
        p += 4 + len;
    }
}

/// One entry of the central directory.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    method: u16,
    flags: u16,
    compressed_size: u64,
    uncompressed_size: u64,
    local_header_offset: u64,
}

/// An archive held in memory.
pub struct Archive {
    bytes: Vec<u8>,
    pub entries: Vec<Entry>,
    limits: Limits,
    /// Uncompressed bytes handed out so far.
    used: std::cell::Cell<u64>,
}

impl Archive {
    pub fn open_with_limits(
        path: impl AsRef<std::path::Path>,
        limits: Limits,
    ) -> Result<Self, ZipError> {
        let mut a = Self::from_bytes(std::fs::read(path)?)?;
        a.limits = limits;
        Ok(a)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ZipError> {
        // End of central directory: signature 0x06054b50, within the last
        // 64 KiB + 22 bytes (the comment is at most 65535 bytes).
        let min = bytes.len().saturating_sub(22 + 65_535);
        let eocd = (min..bytes.len().saturating_sub(21))
            .rev()
            .find(|&i| bytes[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]);
        let Some(eocd) = eocd else {
            return err("no end of central directory record");
        };
        let mut n = u64::from(u16_at(&bytes, eocd + 10)?);
        let mut cd_size = u64::from(u32_at(&bytes, eocd + 12)?);
        let mut cd_offset = u64::from(u32_at(&bytes, eocd + 16)?);
        if let Some(z) = zip64_end(&bytes, eocd, n, cd_size, cd_offset) {
            (n, cd_size, cd_offset) = z;
        } else if cd_size == ZIP64_MAGIC || cd_offset == ZIP64_MAGIC {
            return err("ZIP64 end of central directory record missing");
        }
        let cd = usize::try_from(cd_offset)
            .ok()
            .zip(usize::try_from(cd_size).ok())
            .filter(|&(o, s)| o.checked_add(s).is_some_and(|end| end <= bytes.len()));
        let Some((cd_offset, cd_size)) = cd else {
            return err("central directory out of range");
        };
        // every directory record takes at least 46 bytes
        let mut entries = Vec::with_capacity(n.min(cd_size as u64 / 46) as usize);
        let mut p = cd_offset;
        for _ in 0..n {
            if u32_at(&bytes, p)? != 0x0201_4b50 {
                return err("bad central directory entry");
            }
            let flags = u16_at(&bytes, p + 8)?;
            let method = u16_at(&bytes, p + 10)?;
            let name_len = u16_at(&bytes, p + 28)? as usize;
            let extra_len = u16_at(&bytes, p + 30)? as usize;
            let comment_len = u16_at(&bytes, p + 32)? as usize;
            let mut fields = [
                u64::from(u32_at(&bytes, p + 24)?),
                u64::from(u32_at(&bytes, p + 20)?),
                u64::from(u32_at(&bytes, p + 42)?),
            ];
            let Some(name) = bytes.get(p + 46..p + 46 + name_len) else {
                return err("truncated entry name");
            };
            if fields.contains(&ZIP64_MAGIC) {
                let start = p + 46 + name_len;
                let Some(extra) = bytes.get(start..start + extra_len) else {
                    return err("truncated extra field");
                };
                apply_zip64_extra(extra, &mut fields);
            }
            let [uncompressed_size, compressed_size, local_header_offset] = fields;
            entries.push(Entry {
                name: String::from_utf8_lossy(name).into_owned(),
                method,
                flags,
                compressed_size,
                uncompressed_size,
                local_header_offset,
            });
            p += 46 + name_len + extra_len + comment_len;
        }
        Ok(Archive {
            bytes,
            entries,
            limits: Limits::default(),
            used: std::cell::Cell::new(0),
        })
    }

    /// The table `name`, looked up as gtfs-lib does (`Entity.Loader.loadTable`):
    /// the entry of that name at the archive root (the last one if repeated,
    /// as `java.util.zip.ZipFile.getEntry` returns), else the last entry in a
    /// subfolder with that file name. Unlike gtfs-lib, a subfolder match must
    /// be the whole file name (gtfs-lib takes any name ending in it, such as
    /// `old_stops.txt`) and macOS resource forks under `__MACOSX` are skipped.
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.name == name)
            .or_else(|| {
                self.entries.iter().rev().find(|e| {
                    e.name.rsplit('/').next() == Some(name) && !e.name.starts_with("__MACOSX")
                })
            })
    }

    /// Uncompressed contents of an entry.
    pub fn read(&self, e: &Entry) -> Result<Vec<u8>, ZipError> {
        if e.flags & 1 != 0 {
            return err(format!("{}: encrypted entries are not supported", e.name));
        }
        if [
            e.compressed_size,
            e.uncompressed_size,
            e.local_header_offset,
        ]
        .contains(&UNRESOLVED)
        {
            return err(format!("{}: ZIP64 extra field missing", e.name));
        }
        if e.uncompressed_size > self.limits.max_entry {
            return err(format!(
                "{}: entry of {} bytes exceeds the limit of {} bytes per entry",
                e.name, e.uncompressed_size, self.limits.max_entry
            ));
        }
        if e.method == 8
            && e.uncompressed_size > e.compressed_size.saturating_mul(MAX_DEFLATE_RATIO) + 1024
        {
            return err(format!("{}: implausible compression ratio", e.name));
        }
        let total = self.used.get().saturating_add(e.uncompressed_size);
        if total > self.limits.max_total {
            return err(format!(
                "archive expands beyond the limit of {} bytes in total",
                self.limits.max_total
            ));
        }
        self.used.set(total);
        let h = usize::try_from(e.local_header_offset).unwrap_or(usize::MAX);
        if h > self.bytes.len() || u32_at(&self.bytes, h)? != 0x0403_4b50 {
            return err(format!("{}: bad local header", e.name));
        }
        let name_len = u16_at(&self.bytes, h + 26)? as usize;
        let extra_len = u16_at(&self.bytes, h + 28)? as usize;
        let start = h + 30 + name_len + extra_len;
        let data = usize::try_from(e.compressed_size)
            .ok()
            .and_then(|n| self.bytes.get(start..start.checked_add(n)?));
        let Some(data) = data else {
            return err(format!("{}: data out of range", e.name));
        };
        let out = match e.method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(
                data,
                usize::try_from(e.uncompressed_size).unwrap_or(usize::MAX),
            )
            .map_err(|x| ZipError::Format(format!("{}: deflate: {x:?}", e.name)))?,
            m => {
                return err(format!(
                    "{}: compression method {m} is not supported",
                    e.name
                ))
            }
        };
        if out.len() as u64 != e.uncompressed_size {
            return err(format!("{}: size differs from the directory", e.name));
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Build a zip archive in memory (deflated when `deflate`).
    pub(crate) fn make_zip(files: &[(&str, &[u8])], deflate: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        for (name, data) in files {
            let comp = if deflate {
                miniz_oxide::deflate::compress_to_vec(data, 6)
            } else {
                data.to_vec()
            };
            let offset = out.len() as u32;
            let method: u16 = if deflate { 8 } else { 0 };
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend([20, 0, 0, 0]);
            out.extend(method.to_le_bytes());
            out.extend([0; 8]); // time, date, crc (not checked)
            out.extend((comp.len() as u32).to_le_bytes());
            out.extend((data.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(name.as_bytes());
            out.extend(&comp);
            cd.extend(0x0201_4b50u32.to_le_bytes());
            cd.extend([20, 0, 20, 0, 0, 0]);
            cd.extend(method.to_le_bytes());
            cd.extend([0; 8]);
            cd.extend((comp.len() as u32).to_le_bytes());
            cd.extend((data.len() as u32).to_le_bytes());
            cd.extend((name.len() as u16).to_le_bytes());
            cd.extend([0; 12]); // extra, comment, disk, attrs
            cd.extend(offset.to_le_bytes());
            cd.extend(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend(&cd);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0; 4]);
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((cd.len() as u32).to_le_bytes());
        out.extend(cd_offset.to_le_bytes());
        out.extend([0, 0]);
        out
    }

    #[test]
    fn reads_stored_and_deflated_entries() {
        for deflate in [false, true] {
            let z = make_zip(
                &[
                    ("stops.txt", b"stop_id\n1\n"),
                    ("dir/trips.txt", b"trip_id\nA\n"),
                ],
                deflate,
            );
            let a = Archive::from_bytes(z).unwrap();
            assert_eq!(
                a.read(a.find("stops.txt").unwrap()).unwrap(),
                b"stop_id\n1\n"
            );
            assert_eq!(
                a.read(a.find("trips.txt").unwrap()).unwrap(),
                b"trip_id\nA\n"
            );
            assert!(a.find("routes.txt").is_none());
        }
    }

    #[test]
    fn rejects_garbage_and_lying_sizes() {
        assert!(Archive::from_bytes(b"not a zip".to_vec()).is_err());
        let mut z = make_zip(&[("a.txt", b"hello")], false);
        // claim a larger uncompressed size in the central directory
        let cd = z
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        z[cd + 24] = 9;
        let a = Archive::from_bytes(z).unwrap();
        assert!(a.read(&a.entries[0]).is_err());
    }

    #[test]
    fn limits_bound_entries_ratios_and_totals() {
        let big = vec![b'x'; 100_000];
        let z = make_zip(&[("a.txt", &big), ("b.txt", &big)], true);
        // ratio: 100 kB of 'x' deflates to ~100 bytes (1000:1) - allowed
        let a = Archive::from_bytes(z.clone()).unwrap();
        assert!(a.read(&a.entries[0]).is_ok());
        let mut a = Archive::from_bytes(z.clone()).unwrap();
        a.limits = Limits {
            max_entry: 50_000,
            max_total: 1 << 30,
        };
        assert!(a.read(&a.entries[0]).is_err());
        let mut a = Archive::from_bytes(z).unwrap();
        a.limits = Limits {
            max_entry: 1 << 30,
            max_total: 150_000,
        };
        assert!(a.read(&a.entries[0]).is_ok());
        assert!(a.read(&a.entries[1]).is_err());
    }

    /// Tables are looked up as gtfs-lib does: the root entry first (the
    /// last if repeated), then the last one in a subfolder.
    #[test]
    fn table_lookup_follows_gtfs_lib() {
        let name = |files: &[(&str, &[u8])]| {
            let a = Archive::from_bytes(make_zip(files, false)).unwrap();
            a.read(a.find("stops.txt").unwrap()).unwrap()
        };
        assert_eq!(
            name(&[("old/stops.txt", b"OLD"), ("stops.txt", b"ROOT")]),
            b"ROOT"
        );
        assert_eq!(
            name(&[("stops.txt", b"FIRST"), ("stops.txt", b"SECOND")]),
            b"SECOND"
        );
        assert_eq!(name(&[("a/stops.txt", b"A"), ("b/stops.txt", b"B")]), b"B");
        assert_eq!(
            name(&[
                ("feed/stops.txt", b"FEED"),
                ("__MACOSX/feed/._stops.txt", b"FORK")
            ]),
            b"FEED"
        );
        let a = Archive::from_bytes(make_zip(
            &[("STOPS.TXT", b"x"), ("my_stops.txt", b"y")],
            false,
        ))
        .unwrap();
        assert!(a.find("stops.txt").is_none());
    }

    /// Build a ZIP64 archive in memory: every size and offset saturated in
    /// the headers, with the real values in ZIP64 extra fields, and a ZIP64
    /// end of central directory record. `sizes` overrides the uncompressed
    /// size written for each entry.
    fn make_zip64(files: &[(&str, &[u8])], deflate: bool, sizes: Option<&[u64]>) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        for (i, (name, data)) in files.iter().enumerate() {
            let comp = if deflate {
                miniz_oxide::deflate::compress_to_vec(data, 6)
            } else {
                data.to_vec()
            };
            let size = sizes.map_or(data.len() as u64, |s| s[i]);
            let offset = out.len() as u64;
            let method: u16 = if deflate { 8 } else { 0 };
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend([45, 0, 0, 0]);
            out.extend(method.to_le_bytes());
            out.extend([0; 8]); // time, date, crc (not checked)
            out.extend([0xff; 8]); // sizes
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(20u16.to_le_bytes());
            out.extend(name.as_bytes());
            out.extend(1u16.to_le_bytes());
            out.extend(16u16.to_le_bytes());
            out.extend(size.to_le_bytes());
            out.extend((comp.len() as u64).to_le_bytes());
            out.extend(&comp);
            cd.extend(0x0201_4b50u32.to_le_bytes());
            cd.extend([45, 0, 45, 0, 0, 0]);
            cd.extend(method.to_le_bytes());
            cd.extend([0; 8]);
            cd.extend([0xff; 8]); // sizes
            cd.extend((name.len() as u16).to_le_bytes());
            cd.extend(28u16.to_le_bytes()); // extra
            cd.extend([0; 10]); // comment, disk, attrs
            cd.extend([0xff; 4]); // offset
            cd.extend(name.as_bytes());
            cd.extend(1u16.to_le_bytes());
            cd.extend(24u16.to_le_bytes());
            cd.extend(size.to_le_bytes());
            cd.extend((comp.len() as u64).to_le_bytes());
            cd.extend(offset.to_le_bytes());
        }
        let cd_offset = out.len() as u64;
        out.extend(&cd);
        let end64 = out.len() as u64;
        out.extend(0x0606_4b50u32.to_le_bytes());
        out.extend(44u64.to_le_bytes());
        out.extend([45, 0, 45, 0]);
        out.extend([0; 8]); // disks
        out.extend((files.len() as u64).to_le_bytes());
        out.extend((files.len() as u64).to_le_bytes());
        out.extend((cd.len() as u64).to_le_bytes());
        out.extend(cd_offset.to_le_bytes());
        out.extend(0x0706_4b50u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(end64.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0; 4]);
        out.extend([0xff; 4]); // entry counts
        out.extend([0xff; 8]); // directory size and offset
        out.extend([0, 0]);
        out
    }

    /// ZIP64 archives read as `java.util.zip.ZipFile` reads them.
    #[test]
    fn reads_zip64_archives() {
        for deflate in [false, true] {
            let z = make_zip64(
                &[
                    ("stops.txt", b"stop_id\n1\n"),
                    ("dir/trips.txt", b"trip_id\nA\n"),
                ],
                deflate,
                None,
            );
            let a = Archive::from_bytes(z).unwrap();
            assert_eq!(a.entries.len(), 2);
            assert_eq!(
                a.read(a.find("stops.txt").unwrap()).unwrap(),
                b"stop_id\n1\n"
            );
            assert_eq!(
                a.read(a.find("trips.txt").unwrap()).unwrap(),
                b"trip_id\nA\n"
            );
        }
    }

    #[test]
    fn zip64_errors() {
        // An entry's 64-bit size is checked against the limit, by default 4 GiB.
        let z = make_zip64(&[("stop_times.txt", b"x")], false, Some(&[5 << 30]));
        let a = Archive::from_bytes(z).unwrap();
        assert_eq!(
            a.read(&a.entries[0]).unwrap_err().to_string(),
            "malformed zip file: stop_times.txt: entry of 5368709120 bytes exceeds the limit of 4294967296 bytes per entry"
        );
        // Saturated sizes without a ZIP64 extra field.
        let mut z = make_zip(&[("stops.txt", b"stop_id\nA\n")], false);
        let cd = z
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        z[cd + 20..cd + 28].copy_from_slice(&[0xff; 8]);
        let a = Archive::from_bytes(z).unwrap();
        assert_eq!(
            a.read(&a.entries[0]).unwrap_err().to_string(),
            "malformed zip file: stops.txt: ZIP64 extra field missing"
        );
        // A saturated directory offset without the ZIP64 end record.
        let mut z = make_zip64(&[("stops.txt", b"stop_id\nA\n")], false, None);
        let loc = z.len() - 22 - 20;
        z[loc] = 0;
        assert_eq!(
            Archive::from_bytes(z).err().unwrap().to_string(),
            "malformed zip file: ZIP64 end of central directory record missing"
        );
    }

    #[test]
    fn limit_errors_say_the_limit() {
        let big = vec![b'x'; 100_000];
        let z = make_zip(&[("a.txt", &big), ("b.txt", &big)], true);
        let a = Archive::from_bytes(z.clone()).unwrap();
        let mut small = a.limits;
        small.max_entry = 50_000;
        let mut a = Archive::from_bytes(z.clone()).unwrap();
        a.limits = small;
        assert_eq!(
            a.read(&a.entries[0]).unwrap_err().to_string(),
            "malformed zip file: a.txt: entry of 100000 bytes exceeds the limit of 50000 bytes per entry"
        );
        let mut a = Archive::from_bytes(z).unwrap();
        a.limits.max_total = 150_000;
        a.read(&a.entries[0]).unwrap();
        assert_eq!(
            a.read(&a.entries[1]).unwrap_err().to_string(),
            "malformed zip file: archive expands beyond the limit of 150000 bytes in total"
        );
    }
}
