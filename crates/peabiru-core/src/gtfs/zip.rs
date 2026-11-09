//! Minimal ZIP reader for GTFS feeds: stored and deflated entries, no
//! encryption, no ZIP64. Sizes are bounded so a hostile archive cannot ask
//! for an unbounded allocation.

use std::fmt;

/// Largest uncompressed entry accepted (GTFS `stop_times.txt` of a large
/// metropolitan feed is a few hundred MB).
pub const MAX_ENTRY_SIZE: u64 = 4 << 30;

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
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| ZipError::Format("truncated".into()))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, ZipError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| ZipError::Format("truncated".into()))
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
}

impl Archive {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, ZipError> {
        Self::from_bytes(std::fs::read(path)?)
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
        let n = u16_at(&bytes, eocd + 10)? as usize;
        let cd_size = u32_at(&bytes, eocd + 12)? as usize;
        let cd_offset = u32_at(&bytes, eocd + 16)? as usize;
        if n == 0xffff || cd_offset == 0xffff_ffff {
            return err("ZIP64 archives are not supported");
        }
        if cd_offset
            .checked_add(cd_size)
            .map_or(true, |end| end > bytes.len())
        {
            return err("central directory out of range");
        }
        let mut entries = Vec::with_capacity(n);
        let mut p = cd_offset;
        for _ in 0..n {
            if u32_at(&bytes, p)? != 0x0201_4b50 {
                return err("bad central directory entry");
            }
            let flags = u16_at(&bytes, p + 8)?;
            let method = u16_at(&bytes, p + 10)?;
            let compressed_size = u64::from(u32_at(&bytes, p + 20)?);
            let uncompressed_size = u64::from(u32_at(&bytes, p + 24)?);
            let name_len = u16_at(&bytes, p + 28)? as usize;
            let extra_len = u16_at(&bytes, p + 30)? as usize;
            let comment_len = u16_at(&bytes, p + 32)? as usize;
            let local_header_offset = u64::from(u32_at(&bytes, p + 42)?);
            let Some(name) = bytes.get(p + 46..p + 46 + name_len) else {
                return err("truncated entry name");
            };
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
        Ok(Archive { bytes, entries })
    }

    /// The entry whose file name (ignoring any directory) is `name`.
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.name.rsplit('/').next() == Some(name) && !e.name.starts_with("__MACOSX"))
    }

    /// Uncompressed contents of an entry.
    pub fn read(&self, e: &Entry) -> Result<Vec<u8>, ZipError> {
        if e.flags & 1 != 0 {
            return err(format!("{}: encrypted entries are not supported", e.name));
        }
        if e.uncompressed_size > MAX_ENTRY_SIZE {
            return err(format!("{}: entry larger than 4 GiB", e.name));
        }
        let h = e.local_header_offset as usize;
        if u32_at(&self.bytes, h)? != 0x0403_4b50 {
            return err(format!("{}: bad local header", e.name));
        }
        let name_len = u16_at(&self.bytes, h + 26)? as usize;
        let extra_len = u16_at(&self.bytes, h + 28)? as usize;
        let start = h + 30 + name_len + extra_len;
        let Some(data) = self.bytes.get(start..start + e.compressed_size as usize) else {
            return err(format!("{}: data out of range", e.name));
        };
        let out = match e.method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(
                data,
                e.uncompressed_size as usize,
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
}
