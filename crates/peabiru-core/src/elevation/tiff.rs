//! A reader for the subset of GeoTIFF that elevation rasters use, written
//! from the TIFF 6.0 and GeoTIFF 1.1 specifications: classic and BigTIFF,
//! either byte order, one band of 8, 16 or 32-bit integers or 32 or 64-bit
//! floats, strips or tiles, no compression, LZW, Deflate or
//! PackBits, horizontal (2) and floating-point (3) predictors, and the
//! georeferencing of one tie point and a pixel scale in a coordinate
//! reference system given by its EPSG code, among those whose transform
//! peabiru reproduces ([`crs::lookup`]: WGS 84 longitude and latitude, and
//! the geographic and projected systems of Brazilian and world-wide data).
//! Anything else is an error that says what is not supported. Every
//! combination of these was compared with what R5 (GeoTools 25 and
//! imageio-ext) reads; where R5 fails (several bands, LZW with the
//! floating-point predictor, the horizontal predictor on 64-bit samples,
//! 64-bit integers, ZSTD, no georeferencing) this reader fails too, and
//! where R5 reads something peabiru does not reproduce (another or a
//! user-defined coordinate reference system) or decodes lossily (JPEG) it
//! refuses the raster.
//!
//! The blocks (strips or tiles) are decoded only when a sample falls in
//! them, once each, and may be read from several threads.
//!
//! The file is untrusted input (in the JavaScript package it may be a
//! user's upload): every offset, count and size read from it is checked
//! against the bytes present before use, products of header values use
//! checked arithmetic, nothing is allocated from a header value alone, and
//! the memory spent on decoded pixels has two budgets,
//! [`MAX_BLOCK_PIXELS`] per strip or tile and [`MAX_DECODED_PIXELS`] for
//! the whole raster. The block offset and byte count tables are read from
//! the file a value at a time, and must hold exactly one value per block;
//! decoded blocks are kept in a table only for up to 65,536 blocks, in a
//! map of those read beyond. A malformed file is an error, never a panic.

use super::crs::{self, Crs};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Most pixels in one strip or tile: 32 Mi (256 MiB once decoded as
/// doubles). GDAL writes tiles of 256 x 256 or 512 x 512 pixels and strips
/// of a few rows; the largest common layout, a whole uncompressed 1-degree
/// SRTM or Copernicus tile in a single strip, is 3601 x 3601 = 13 Mi.
pub(crate) const MAX_BLOCK_PIXELS: usize = 32 << 20;
/// Most pixels decoded from one raster: 128 Mi (1 GiB as doubles), such as
/// a 1-arc-second area of 3 by 3 degrees, far more than a metropolitan
/// area's network touches. Only the blocks under the network are decoded,
/// so a larger raster is fine as long as the network covers less of it;
/// past the budget the build fails with a message asking to crop it.
pub(crate) const MAX_DECODED_PIXELS: usize = 128 << 20;
/// Up to this many blocks the decoded blocks are kept in a table with a
/// slot per block (40 bytes each); beyond, in a map of the blocks decoded.
const DENSE_BLOCKS: usize = 1 << 16;

/// An error reading the raster: what is wrong, without the file name
/// (the caller adds it).
pub(crate) type TiffResult<T> = Result<T, String>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Compression {
    None,
    Lzw,
    Deflate,
    PackBits,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Uint,
    Int,
    Float,
}

/// One IFD entry: type, count and where its values are.
#[derive(Clone, Copy)]
struct Entry {
    typ: u16,
    count: u64,
    /// Offset of the values in the file, or of the inline value field.
    at: usize,
}

struct Reader<'a> {
    b: &'a [u8],
    le: bool,
    big: bool,
}

impl<'a> Reader<'a> {
    fn bytes(&self, at: usize, n: usize) -> TiffResult<&'a [u8]> {
        at.checked_add(n)
            .and_then(|end| self.b.get(at..end))
            .ok_or_else(|| "the file is truncated (an offset points past its end)".to_string())
    }
    fn u16(&self, at: usize) -> TiffResult<u16> {
        let s = self.bytes(at, 2)?;
        let a = [s[0], s[1]];
        Ok(if self.le {
            u16::from_le_bytes(a)
        } else {
            u16::from_be_bytes(a)
        })
    }
    fn u32(&self, at: usize) -> TiffResult<u32> {
        let s = self.bytes(at, 4)?;
        let a = [s[0], s[1], s[2], s[3]];
        Ok(if self.le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        })
    }
    fn u64(&self, at: usize) -> TiffResult<u64> {
        let s = self.bytes(at, 8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(if self.le {
            u64::from_le_bytes(a)
        } else {
            u64::from_be_bytes(a)
        })
    }

    fn type_size(typ: u16) -> Option<usize> {
        Some(match typ {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 | 13 => 4,
            5 | 10 | 12 | 16 | 17 | 18 => 8,
            _ => return None,
        })
    }

    /// Integer values of an entry (BYTE, SHORT, LONG, LONG8 and their
    /// signed forms, negative values refused).
    fn ints(&self, e: Entry, tag: u16) -> TiffResult<Vec<u64>> {
        let size = match e.typ {
            1 | 3 | 4 | 6 | 7 | 8 | 9 | 13 | 16 | 17 | 18 => Self::type_size(e.typ).unwrap_or(8),
            t => return Err(format!("tag {tag} has type {t}, not an integer type")),
        };
        let n = self.checked_count(e, size, tag)?;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let v = match e.typ {
                1 | 7 => u64::from(self.bytes(e.at + i, 1)?[0]),
                3 => u64::from(self.u16(e.at + 2 * i)?),
                4 | 13 => u64::from(self.u32(e.at + 4 * i)?),
                16 | 18 => self.u64(e.at + 8 * i)?,
                6 => {
                    let v = self.bytes(e.at + i, 1)?[0] as i8;
                    u64::try_from(v).map_err(|_| format!("tag {tag} is negative"))?
                }
                8 => u64::try_from(self.u16(e.at + 2 * i)? as i16)
                    .map_err(|_| format!("tag {tag} is negative"))?,
                9 => u64::try_from(self.u32(e.at + 4 * i)? as i32)
                    .map_err(|_| format!("tag {tag} is negative"))?,
                17 => u64::try_from(self.u64(e.at + 8 * i)? as i64)
                    .map_err(|_| format!("tag {tag} is negative"))?,
                t => return Err(format!("tag {tag} has type {t}, not an integer type")),
            };
            out.push(v);
        }
        Ok(out)
    }

    /// The number of values of an entry whose values (of `size` bytes) all
    /// lie in the file.
    fn checked_count(&self, e: Entry, size: usize, tag: u16) -> TiffResult<usize> {
        let n = usize::try_from(e.count).map_err(|_| format!("tag {tag} is too long"))?;
        n.checked_mul(size)
            .and_then(|len| e.at.checked_add(len))
            .filter(|&end| end <= self.b.len())
            .ok_or_else(|| {
                format!("tag {tag} is truncated (its values run past the end of the file)")
            })?;
        Ok(n)
    }

    /// DOUBLE values of an entry (the GeoTIFF tags).
    fn doubles(&self, e: Entry, tag: u16) -> TiffResult<Vec<f64>> {
        if e.typ != 12 {
            return Err(format!("tag {tag} has type {}, not DOUBLE", e.typ));
        }
        let n = self.checked_count(e, 8, tag)?;
        (0..n)
            .map(|i| self.u64(e.at + 8 * i).map(f64::from_bits))
            .collect()
    }
}

/// The georeferencing GeoTools builds from the tags, as R5's
/// `RasterDataSourceSampler` reads the coverage (GeoTools 25,
/// `GridCoverage2D.evaluate` without interpolation): the grid-to-world
/// affine transform maps pixel *centres*, its inverse is computed
/// element by element as `java.awt.geom.AffineTransform.createInverse`
/// does, and the envelope is that of the grid corners.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Georef {
    /// Inverse transform: `x = lon * i00 + i02`, `y = lat * i11 + i12`.
    i00: f64,
    i02: f64,
    i11: f64,
    i12: f64,
    /// `Envelope2D`: minimum corner, width and height.
    env_x: f64,
    env_y: f64,
    env_w: f64,
    env_h: f64,
}

impl Georef {
    /// From the pixel scale, the tie point (`I = J = 0`) and whether the
    /// tie point is a pixel's centre (`PixelIsPoint`) or corner
    /// (`PixelIsArea`), for an image `w` by `h` pixels.
    pub(crate) fn new(sx: f64, sy: f64, tx: f64, ty: f64, point: bool, w: u32, h: u32) -> Self {
        // (the caller checks that the result is finite)
        let m00 = sx;
        let m11 = -sy;
        // Pixel centres: the corner transform followed by half a pixel,
        // unless the tie point already is a centre.
        let (m02, m12) = if point {
            (tx, ty)
        } else {
            (tx + 0.5 * m00, ty + 0.5 * m11)
        };
        // The grid range in centre coordinates is [-0.5, w - 0.5].
        let x_min = -0.5 * m00 + m02;
        let x_max = (f64::from(w) - 0.5) * m00 + m02;
        let y_min = (f64::from(h) - 0.5) * m11 + m12;
        let y_max = -0.5 * m11 + m12;
        Georef {
            i00: 1.0 / m00,
            i02: -m02 / m00,
            i11: 1.0 / m11,
            i12: -m12 / m11,
            env_x: x_min,
            env_y: y_min,
            env_w: x_max - x_min,
            env_h: y_max - y_min,
        }
    }

    fn is_finite(&self) -> bool {
        [
            self.i00, self.i02, self.i11, self.i12, self.env_x, self.env_y, self.env_w, self.env_h,
        ]
        .iter()
        .all(|v| v.is_finite())
    }

    /// Whether `(x, y)` lies within `margin` (in the raster's units) of the
    /// envelope's edges ([`Near::Edge`]: R5 may then read a pixel, 0 m, or
    /// fail on the sliver between the last pixel and the envelope), or of
    /// the boundary between two pixels inside it ([`Near::Pixel`]).
    pub(crate) fn near_boundary(&self, x: f64, y: f64, margin: f64) -> Near {
        let (x0, x1) = (self.env_x, self.env_x + self.env_w);
        let (y0, y1) = (self.env_y, self.env_y + self.env_h);
        if !(x >= x0 - margin && x <= x1 + margin && y >= y0 - margin && y <= y1 + margin) {
            // well outside (or NaN): R5 reads 0 m wherever it lands
            return Near::No;
        }
        if (x - x0).abs() <= margin
            || (x - x1).abs() <= margin
            || (y - y0).abs() <= margin
            || (y - y1).abs() <= margin
        {
            return Near::Edge;
        }
        let near_half = |f: f64, m: f64| (f - f.floor() - 0.5).abs() <= m;
        if near_half(x * self.i00 + self.i02, margin * self.i00.abs())
            || near_half(y * self.i11 + self.i12, margin * self.i11.abs())
        {
            Near::Pixel
        } else {
            Near::No
        }
    }

    /// The pixel R5 reads at `(lon, lat)`: `None` outside the envelope
    /// (R5 then uses an elevation of 0), `Err(())` inside the envelope
    /// but outside the image, where GeoTools throws and R5's network
    /// build fails.
    #[allow(clippy::result_unit_err)]
    pub(crate) fn pixel(
        &self,
        lon: f64,
        lat: f64,
        w: u32,
        h: u32,
    ) -> Result<Option<(u32, u32)>, ()> {
        // `Envelope2D.contains`: both edges inclusive.
        let inside = lon >= self.env_x
            && lat >= self.env_y
            && lon <= self.env_x + self.env_w
            && lat <= self.env_y + self.env_h;
        if !inside {
            return Ok(None);
        }
        let fx = lon * self.i00 + self.i02;
        let fy = lat * self.i11 + self.i12;
        // `(int) Math.round(double)`
        let x = super::java_round(fx) as i32;
        let y = super::java_round(fy) as i32;
        if x < 0 || y < 0 || x as u32 >= w || y as u32 >= h {
            return Err(());
        }
        Ok(Some((x as u32, y as u32)))
    }
}

/// What lies within a margin of a point: see [`Georef::near_boundary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Near {
    No,
    Pixel,
    Edge,
}

type Block = TiffResult<Vec<f64>>;

/// The decoded blocks: a slot per block for a few blocks, a map of those
/// decoded for many (so that a file declaring millions of tiny blocks costs
/// nothing until they are read, and each one read counts against
/// [`MAX_DECODED_PIXELS`]).
enum Slots {
    Dense(Vec<OnceLock<Block>>),
    Sparse(Mutex<HashMap<usize, Arc<OnceLock<Block>>>>),
}

/// A GeoTIFF elevation raster, read lazily from its bytes.
pub(crate) struct Raster<'a> {
    b: &'a [u8],
    le: bool,
    pub(crate) width: u32,
    pub(crate) height: u32,
    bits: u32,
    kind: Kind,
    compression: Compression,
    predictor: u16,
    /// Block size in pixels (strips: the image width by the rows per strip).
    block_w: u32,
    block_h: u32,
    blocks_across: u32,
    tiled: bool,
    /// The block offsets and byte counts, read from the file when a block
    /// is decoded (never copied whole: their number comes from the file).
    offsets: Entry,
    counts: Entry,
    big: bool,
    pub(crate) georef: Georef,
    /// The coordinate reference system of the georeferencing.
    pub(crate) crs: Crs,
    /// GDAL's declared no-data value, if any (only for warnings).
    pub(crate) nodata: Option<f64>,
    decoded: Slots,
    /// Pixels decoded so far, against [`MAX_DECODED_PIXELS`].
    spent: AtomicUsize,
}

const TAG_WIDTH: u16 = 256;
const TAG_LENGTH: u16 = 257;
const TAG_BITS: u16 = 258;
const TAG_COMPRESSION: u16 = 259;
const TAG_PHOTOMETRIC: u16 = 262;
const TAG_FILL_ORDER: u16 = 266;
const TAG_STRIP_OFFSETS: u16 = 273;
const TAG_SAMPLES: u16 = 277;
const TAG_ROWS_PER_STRIP: u16 = 278;
const TAG_STRIP_COUNTS: u16 = 279;
const TAG_PLANAR: u16 = 284;
const TAG_PREDICTOR: u16 = 317;
const TAG_TILE_WIDTH: u16 = 322;
const TAG_TILE_LENGTH: u16 = 323;
const TAG_TILE_OFFSETS: u16 = 324;
const TAG_TILE_COUNTS: u16 = 325;
const TAG_SAMPLE_FORMAT: u16 = 339;
const TAG_PIXEL_SCALE: u16 = 33550;
const TAG_TIEPOINT: u16 = 33922;
const TAG_TRANSFORMATION: u16 = 34264;
const TAG_GEO_KEYS: u16 = 34735;

const TAG_GDAL_NODATA: u16 = 42113;

const KEY_MODEL_TYPE: u16 = 1024;
const KEY_RASTER_TYPE: u16 = 1025;
const KEY_GEOGRAPHIC_TYPE: u16 = 2048;
const KEY_GEOG_ANGULAR_UNITS: u16 = 2054;
const KEY_PROJECTED_TYPE: u16 = 3072;
const KEY_PROJ_LINEAR_UNITS: u16 = 3076;
/// The GeoTIFF code of a user-defined system (given by its parameters).
const USER_DEFINED: u64 = 32767;

/// The coordinate reference system of the GeoTIFF keys, as GeoTools
/// decodes it (`GeoTiffMetadata2CRSAdapter`): the model type chooses
/// between the projected and the geographic system, which must be given
/// by an EPSG code among those [`crs::lookup`] supports.
fn crs_of(key: &impl Fn(u16) -> Option<u64>) -> TiffResult<Crs> {
    const HOW: &str = "reproject the raster to one of them, for instance to WGS 84 \
                       (gdalwarp -t_srs EPSG:4326)";
    let (code, projected) = match key(KEY_MODEL_TYPE) {
        Some(1) => (key(KEY_PROJECTED_TYPE), true),
        Some(2) => (key(KEY_GEOGRAPHIC_TYPE), false),
        Some(3) => {
            return Err(
                "the raster is in geocentric coordinates (model type 3), which r5r \
                        cannot sample"
                    .into(),
            )
        }
        m => {
            return Err(format!(
                "the raster has no coordinate reference system (GeoTIFF model type {}); r5r \
                 fails on such a raster too",
                m.map_or("missing".into(), |v| v.to_string())
            ))
        }
    };
    let kind = if projected { "projected" } else { "geographic" };
    let code = match code {
        None | Some(USER_DEFINED) => {
            return Err(format!(
                "the raster's {kind} coordinate reference system is user-defined (given by \
                 its parameters, not by an EPSG code), which peabiru does not reproduce; \
                 supported: {}; {HOW}",
                crs::SUPPORTED
            ))
        }
        Some(c) => c,
    };
    let crs = u32::try_from(code)
        .ok()
        .and_then(crs::lookup)
        .filter(|c| c.projection.is_some() == projected)
        .ok_or_else(|| {
            format!(
                "the raster is in EPSG:{code}, a coordinate reference system whose transform \
                 peabiru has not matched with r5r's (GeoTools'); supported: {}; {HOW}",
                crs::SUPPORTED
            )
        })?;
    if !projected {
        if let Some(u) = key(KEY_GEOG_ANGULAR_UNITS) {
            if u != 9102 {
                return Err(format!(
                    "angular unit {u} (not degrees, 9102) for {} is not supported",
                    crs.label()
                ));
            }
        }
    } else if let Some(u) = key(KEY_PROJ_LINEAR_UNITS) {
        if u != 9001 {
            return Err(format!(
                "linear unit {u} (not metres, 9001) for {} is not supported",
                crs.label()
            ));
        }
    }
    Ok(crs)
}

/// GDAL's no-data value (TIFF tag 42113, ASCII), if the file declares a
/// number there: only for the warning about samples that hold it.
fn nodata_of(r: &Reader<'_>, e: Option<Entry>) -> Option<f64> {
    let e = e?;
    // a number takes a few dozen characters: ignore anything longer
    if e.typ != 2 || e.count == 0 || e.count > 64 {
        return None;
    }
    let n = r.checked_count(e, 1, TAG_GDAL_NODATA).ok()?;
    let text = std::str::from_utf8(r.bytes(e.at, n).ok()?).ok()?;
    text.trim_end_matches('\0').trim().parse::<f64>().ok()
}

impl<'a> Raster<'a> {
    /// Parse the first image of a TIFF file and check that it is one this
    /// reader (and R5) can sample.
    pub(crate) fn parse(b: &'a [u8]) -> TiffResult<Self> {
        let le = match b.get(..2) {
            Some(b"II") => true,
            Some(b"MM") => false,
            _ => return Err("not a TIFF file (it does not start with II or MM)".into()),
        };
        let mut r = Reader { b, le, big: false };
        let version = r.u16(2)?;
        let first = match version {
            42 => u64::from(r.u32(4)?),
            43 => {
                r.big = true;
                if r.u16(4)? != 8 {
                    return Err("a BigTIFF file with an offset size other than 8".into());
                }
                r.u64(8)?
            }
            v => return Err(format!("not a TIFF file (version {v})")),
        };
        let ifd =
            usize::try_from(first).map_err(|_| "the first image directory is out of reach")?;
        if ifd < if r.big { 16 } else { 8 } {
            return Err("the first image directory overlaps the file header".into());
        }
        let (n, mut at, step) = if r.big {
            (r.u64(ifd)?, ifd + 8, 20)
        } else {
            (u64::from(r.u16(ifd)?), ifd + 2, 12)
        };
        // every entry must be in the file before any is read
        usize::try_from(n)
            .ok()
            .and_then(|n| n.checked_mul(step))
            .and_then(|len| at.checked_add(len))
            .filter(|&end| end <= b.len())
            .ok_or("the image directory runs past the end of the file")?;
        let mut entries: Vec<(u16, Entry)> = Vec::new();
        for _ in 0..n {
            let tag = r.u16(at)?;
            let typ = r.u16(at + 2)?;
            let (count, field) = if r.big {
                (r.u64(at + 4)?, at + 12)
            } else {
                (u64::from(r.u32(at + 4)?), at + 8)
            };
            let inline = if r.big { 8 } else { 4 };
            let size = Reader::type_size(typ).map(|s| s as u64);
            let value_at = match size {
                Some(s) if s.saturating_mul(count) > inline => {
                    if r.big {
                        r.u64(field)?
                    } else {
                        u64::from(r.u32(field)?)
                    }
                }
                _ => field as u64,
            };
            let value_at = usize::try_from(value_at).map_err(|_| "a tag points out of the file")?;
            entries.push((
                tag,
                Entry {
                    typ,
                    count,
                    at: value_at,
                },
            ));
            at += step;
        }
        let get = |tag: u16| entries.iter().find(|(t, _)| *t == tag).map(|&(_, e)| e);
        let int1 = |tag: u16, default: Option<u64>| -> TiffResult<u64> {
            match get(tag) {
                Some(e) => {
                    let v = r.ints(e, tag)?;
                    v.first()
                        .copied()
                        .ok_or_else(|| format!("tag {tag} is empty"))
                }
                None => default.ok_or_else(|| format!("the required tag {tag} is missing")),
            }
        };
        let width = int1(TAG_WIDTH, None)?;
        let height = int1(TAG_LENGTH, None)?;
        if width == 0 || height == 0 || width > u64::from(u32::MAX) || height > u64::from(u32::MAX)
        {
            return Err(format!("an image of {width} by {height} pixels"));
        }
        let (width, height) = (width as u32, height as u32);
        let samples = int1(TAG_SAMPLES, Some(1))?;
        if samples != 1 {
            return Err(format!(
                "{samples} bands (samples per pixel); an elevation raster must have one band \
                 (r5r fails on such a raster too)"
            ));
        }
        let bits = int1(TAG_BITS, Some(1))?;
        let format = int1(TAG_SAMPLE_FORMAT, Some(1))?;
        let kind = match (format, bits) {
            (1, 8 | 16 | 32) => Kind::Uint,
            (2, 16 | 32) => Kind::Int,
            (3, 32 | 64) => Kind::Float,
            (1 | 2, 64) => {
                return Err(
                    "64-bit integer samples are not supported (r5r fails on such a \
                            raster too); use 16 or 32-bit integers or floats"
                        .into(),
                )
            }
            (2, 8) => {
                return Err(
                    "signed 8-bit samples are not supported; use 16-bit integers \
                            or floats"
                        .into(),
                )
            }
            (f, b) => {
                return Err(format!(
                    "samples of {b} bits with sample format {f} are not supported (only 8, 16, \
                     32 or 64-bit integers and 32 or 64-bit floats)"
                ))
            }
        };
        let photometric = int1(TAG_PHOTOMETRIC, Some(1))?;
        if photometric > 1 {
            return Err(format!(
                "photometric interpretation {photometric} is not supported (only grey levels)"
            ));
        }
        if int1(TAG_FILL_ORDER, Some(1))? != 1 {
            return Err("fill order 2 (least significant bit first) is not supported".into());
        }
        let _planar = int1(TAG_PLANAR, Some(1))?; // one band: either layout is the same
        let compression = match int1(TAG_COMPRESSION, Some(1))? {
            1 => Compression::None,
            5 => Compression::Lzw,
            8 | 32946 => Compression::Deflate,
            32773 => Compression::PackBits,
            7 => {
                return Err(
                    "JPEG compression is not supported (it is lossy: use LZW or Deflate \
                            for elevation)"
                        .into(),
                )
            }
            50000 => {
                return Err(
                    "ZSTD compression is not supported (r5r fails on such a raster too)".into(),
                )
            }
            c => {
                return Err(format!(
                    "compression {c} is not supported (only none, LZW, Deflate and PackBits)"
                ))
            }
        };
        let predictor = int1(TAG_PREDICTOR, Some(1))? as u16;
        match (predictor, kind, compression) {
            (1, _, _) => {}
            (2, _, _) if bits == 64 => {
                return Err(
                    "the horizontal predictor on 64-bit samples is not supported \
                            (r5r fails on such a raster too); use the floating-point predictor \
                            (3) or none"
                        .into(),
                )
            }
            (2, _, _) => {}
            (3, Kind::Float, Compression::Lzw) => {
                return Err("LZW compression with the floating-point predictor is not \
                            supported (r5r fails on such a raster too); use Deflate"
                    .into())
            }
            (3, Kind::Float, _) => {}
            (p, _, _) => return Err(format!("predictor {p} is not supported for these samples")),
        }
        let tiled = get(TAG_TILE_WIDTH).is_some();
        let (block_w, block_h, offsets, counts) = if tiled {
            let tw = int1(TAG_TILE_WIDTH, None)?;
            let th = int1(TAG_TILE_LENGTH, None)?;
            // TIFF 6.0: tile sizes are multiples of 16 (no DEM uses less)
            if tw % 16 != 0 || th % 16 != 0 {
                return Err(format!(
                    "tiles of {tw} by {th} pixels; TIFF tiles are multiples of 16 pixels"
                ));
            }
            let offsets = get(TAG_TILE_OFFSETS).ok_or("tiles without offsets")?;
            let counts = get(TAG_TILE_COUNTS).ok_or("tiles without byte counts")?;
            (
                tw,
                th,
                (offsets, TAG_TILE_OFFSETS),
                (counts, TAG_TILE_COUNTS),
            )
        } else {
            let rows = int1(TAG_ROWS_PER_STRIP, Some(u64::from(height)))?.min(u64::from(height));
            let offsets = get(TAG_STRIP_OFFSETS).ok_or("strips without offsets")?;
            let counts = get(TAG_STRIP_COUNTS).ok_or("strips without byte counts")?;
            (
                u64::from(width),
                rows,
                (offsets, TAG_STRIP_OFFSETS),
                (counts, TAG_STRIP_COUNTS),
            )
        };
        if block_w == 0
            || block_h == 0
            || block_w > u64::from(u32::MAX)
            || block_h > u64::from(u32::MAX)
        {
            return Err(format!("blocks of {block_w} by {block_h} pixels"));
        }
        let (block_w, block_h) = (block_w as u32, block_h as u32);
        let block_px = (block_w as usize)
            .checked_mul(block_h as usize)
            .filter(|&p| p <= MAX_BLOCK_PIXELS)
            .ok_or_else(|| {
                format!(
                    "blocks (strips or tiles) of {block_w} by {block_h} pixels, more than the \
                     {MAX_BLOCK_PIXELS} pixels peabiru decodes at once; write the raster in \
                     tiles (gdal_translate -co TILED=YES)"
                )
            })?;
        let _ = block_px;
        let blocks_across = width.div_ceil(block_w);
        let blocks_down = height.div_ceil(block_h);
        let n_blocks = (blocks_across as usize)
            .checked_mul(blocks_down as usize)
            .ok_or("too many blocks for the image")?;
        // exactly one offset and one byte count per block, all in the file
        // (so the number of blocks is bounded by the file's size)
        for (e, tag) in [offsets, counts] {
            if !matches!(e.typ, 3 | 4 | 16) {
                return Err(format!(
                    "tag {tag} has type {}, not SHORT, LONG or LONG8",
                    e.typ
                ));
            }
            if e.count != n_blocks as u64 {
                return Err(format!(
                    "{} block offsets or byte counts (tag {tag}) for the {n_blocks} blocks \
                     of the image",
                    e.count
                ));
            }
            r.checked_count(e, Reader::type_size(e.typ).unwrap_or(8), tag)?;
        }
        let (offsets, counts) = (offsets.0, counts.0);

        // Georeferencing.
        if get(TAG_TRANSFORMATION).is_some() {
            return Err(
                "the georeferencing is a model transformation (tag 34264), which is \
                        not supported; give it as a tie point and a pixel scale (a raster \
                        that is not rotated)"
                    .into(),
            );
        }
        let (scale, tie) = match (get(TAG_PIXEL_SCALE), get(TAG_TIEPOINT)) {
            (Some(s), Some(t)) => (r.doubles(s, TAG_PIXEL_SCALE)?, r.doubles(t, TAG_TIEPOINT)?),
            _ => {
                return Err(
                    "the raster has no georeferencing (GeoTIFF tie point and pixel \
                            scale); r5r fails on such a raster too"
                        .into(),
                )
            }
        };
        if scale.len() < 2 || tie.len() != 6 {
            return Err(format!(
                "{} pixel scale values and {} tie point values (expected 3 and 6: a single \
                 tie point)",
                scale.len(),
                tie.len()
            ));
        }
        if tie[0] != 0.0 || tie[1] != 0.0 {
            return Err(format!(
                "the tie point is at pixel ({}, {}); only a tie point at pixel (0, 0) is \
                 supported",
                tie[0], tie[1]
            ));
        }
        let (sx, sy) = (scale[0], scale[1]);
        if !(sx > 0.0 && sy > 0.0 && sx.is_finite() && sy.is_finite())
            || !tie[3].is_finite()
            || !tie[4].is_finite()
        {
            return Err(format!(
                "a pixel scale of ({sx}, {sy}) and a tie point at ({}, {}): only finite, \
                 positive scales are supported (a north-up raster)",
                tie[3], tie[4]
            ));
        }
        let keys = match get(TAG_GEO_KEYS) {
            Some(e) => r.ints(e, TAG_GEO_KEYS)?,
            None => {
                return Err(
                    "the raster has no GeoTIFF keys (no coordinate reference system); \
                            r5r fails on such a raster too"
                        .into(),
                )
            }
        };
        let key = |id: u16| -> Option<u64> {
            if keys.len() < 4 {
                return None;
            }
            // no more keys than the directory holds, whatever its count says
            let n = usize::try_from(keys[3]).unwrap_or(usize::MAX);
            keys[4..].chunks_exact(4).take(n).find_map(|k| {
                // only keys held in the directory itself (location 0)
                (k[0] == u64::from(id) && k[1] == 0).then_some(k[3])
            })
        };
        let crs = crs_of(&key)?;
        let nodata = nodata_of(&r, get(TAG_GDAL_NODATA));
        let point = match key(KEY_RASTER_TYPE) {
            None | Some(1) => false,
            Some(2) => true,
            Some(t) => return Err(format!("raster type {t} is not supported")),
        };
        let georef = Georef::new(sx, sy, tie[3], tie[4], point, width, height);
        if !georef.is_finite() {
            return Err(format!(
                "a pixel scale of ({sx}, {sy}) and a tie point at ({}, {}) give no usable grid",
                tie[3], tie[4]
            ));
        }
        Ok(Raster {
            b,
            le,
            width,
            height,
            bits: bits as u32,
            kind,
            compression,
            predictor,
            block_w,
            block_h,
            blocks_across,
            tiled,
            offsets,
            counts,
            big: r.big,
            georef,
            crs,
            nodata,
            decoded: if n_blocks <= DENSE_BLOCKS {
                Slots::Dense((0..n_blocks).map(|_| OnceLock::new()).collect())
            } else {
                Slots::Sparse(Mutex::new(HashMap::new()))
            },
            spent: AtomicUsize::new(0),
        })
    }

    /// The value of pixel `(x, y)` as GeoTools' `getPixel` gives it (a
    /// double: integers exactly, floats widened).
    pub(crate) fn value(&self, x: u32, y: u32) -> TiffResult<f64> {
        if x >= self.width || y >= self.height {
            return Err(format!("pixel ({x}, {y}) is outside the image"));
        }
        let bx = x / self.block_w;
        let by = y / self.block_h;
        let k = by as usize * self.blocks_across as usize + bx as usize;
        let i = (y - by * self.block_h) as usize * self.block_w as usize
            + (x - bx * self.block_w) as usize;
        let read = |slot: &OnceLock<Block>| -> TiffResult<f64> {
            let block = slot
                .get_or_init(|| self.decode(k, by))
                .as_ref()
                .map_err(Clone::clone)?;
            block
                .get(i)
                .copied()
                .ok_or_else(|| "a block is too short".into())
        };
        match &self.decoded {
            Slots::Dense(v) => read(v.get(k).ok_or("a block is missing")?),
            Slots::Sparse(m) => {
                let slot = {
                    let mut m = m.lock().unwrap_or_else(|p| p.into_inner());
                    Arc::clone(m.entry(k).or_default())
                };
                read(&slot)
            }
        }
    }

    /// Value `k` of a block offset or byte count table (checked in `parse`
    /// to lie in the file).
    fn table_value(&self, e: Entry, k: usize) -> Option<usize> {
        let r = Reader {
            b: self.b,
            le: self.le,
            big: self.big,
        };
        let v = match e.typ {
            3 => u64::from(r.u16(e.at.checked_add(k.checked_mul(2)?)?).ok()?),
            4 => u64::from(r.u32(e.at.checked_add(k.checked_mul(4)?)?).ok()?),
            _ => r.u64(e.at.checked_add(k.checked_mul(8)?)?).ok()?,
        };
        usize::try_from(v).ok()
    }

    /// Rows held by block row `by` (the last strip may be short; tiles are
    /// always full).
    fn rows_in(&self, by: u32) -> u32 {
        if self.tiled {
            self.block_h
        } else {
            self.block_h.min(self.height - by * self.block_h)
        }
    }

    fn decode(&self, k: usize, by: u32) -> TiffResult<Vec<f64>> {
        let block_px = self.block_w as usize * self.block_h as usize; // <= MAX_BLOCK_PIXELS
        let before = self.spent.fetch_add(block_px, Ordering::Relaxed);
        if before.saturating_add(block_px) > MAX_DECODED_PIXELS {
            return Err(format!(
                "the network covers more of the raster than the {MAX_DECODED_PIXELS} pixels \
                 peabiru decodes from one raster; crop the raster to the network's area \
                 (gdal_translate -projwin)"
            ));
        }
        let off = self.table_value(self.offsets, k);
        let len = self.table_value(self.counts, k);
        let (Some(off), Some(len)) = (off, len) else {
            return Err("a block is out of reach".into());
        };
        if len == 0 {
            return Err(
                "a block is missing (a sparse file); write the raster without SPARSE_OK".into(),
            );
        }
        let raw = off
            .checked_add(len)
            .and_then(|end| self.b.get(off..end))
            .ok_or("the file is truncated (a block points past its end)")?;
        let bps = (self.bits / 8) as usize;
        let rows = self.rows_in(by) as usize;
        // <= MAX_BLOCK_PIXELS * 8: no overflow
        let row_bytes = self.block_w as usize * bps;
        let need = row_bytes * rows;
        let data: std::borrow::Cow<[u8]> = match self.compression {
            Compression::None => std::borrow::Cow::Borrowed(raw),
            Compression::Lzw => std::borrow::Cow::Owned(lzw_decode(raw, need)?),
            Compression::Deflate => std::borrow::Cow::Owned(
                miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(raw, need.max(1))
                    .or_else(|e| {
                        // a block may hold more than needed: keep what came out
                        if matches!(e.status, miniz_oxide::inflate::TINFLStatus::HasMoreOutput) {
                            Ok(e.output)
                        } else {
                            Err(e)
                        }
                    })
                    .map_err(|e| {
                        format!("a Deflate block cannot be decompressed ({:?})", e.status)
                    })?,
            ),
            Compression::PackBits => std::borrow::Cow::Owned(packbits_decode(raw, need)?),
        };
        if data.len() < need {
            return Err(format!(
                "a block decodes to {} bytes instead of {need}",
                data.len()
            ));
        }
        let mut out = Vec::with_capacity(block_px);
        let mut row = vec![0u8; row_bytes];
        for chunk in data[..need].chunks_exact(row_bytes) {
            row.copy_from_slice(chunk);
            self.row_values(&mut row, &mut out);
        }
        // tiles as wide as the image but strips by count: pad to full blocks
        out.resize(self.block_w as usize * self.block_h as usize, 0.0);
        Ok(out)
    }

    /// One decompressed row: undo the predictor and append its values.
    fn row_values(&self, row: &mut [u8], out: &mut Vec<f64>) {
        let bps = (self.bits / 8) as usize;
        let w = self.block_w as usize;
        let le = self.le;
        // read a sample in file byte order as bits
        let word = |s: &[u8]| -> u64 {
            let mut v = 0u64;
            if le {
                for (i, &b) in s.iter().enumerate() {
                    v |= u64::from(b) << (8 * i);
                }
            } else {
                for &b in s {
                    v = (v << 8) | u64::from(b);
                }
            }
            v
        };
        let mask = if bps == 8 {
            u64::MAX
        } else {
            (1u64 << (8 * bps)) - 1
        };
        let mut words: Vec<u64> = match self.predictor {
            3 => {
                // byte differencing over the whole row, then the bytes of each
                // sample gathered from the planes, most significant first
                for i in 1..row.len() {
                    row[i] = row[i].wrapping_add(row[i - 1]);
                }
                (0..w)
                    .map(|i| {
                        let mut v = 0u64;
                        for k in 0..bps {
                            v = (v << 8) | u64::from(row[k * w + i]);
                        }
                        v
                    })
                    .collect()
            }
            _ => row.chunks_exact(bps).map(word).collect(),
        };
        if self.predictor == 2 {
            for i in 1..words.len() {
                words[i] = words[i].wrapping_add(words[i - 1]) & mask;
            }
        }
        out.extend(words.into_iter().map(|v| match (self.kind, self.bits) {
            (Kind::Uint, _) => v as f64,
            (Kind::Int, 16) => f64::from(v as u16 as i16),
            (Kind::Int, _) => f64::from(v as u32 as i32),
            (Kind::Float, 32) => f64::from(f32::from_bits(v as u32)),
            (Kind::Float, _) => f64::from_bits(v),
        }));
    }
}

/// TIFF LZW (most significant bit first, code width growing one code
/// early, clear code 256, end of information 257), up to `limit` bytes.
fn lzw_decode(input: &[u8], limit: usize) -> TiffResult<Vec<u8>> {
    const CLEAR: u16 = 256;
    const EOI: u16 = 257;
    if input.len() >= 2 && input[0] == 0 && input[1] & 0x01 != 0 {
        return Err("old-style LZW compression is not supported".into());
    }
    // table: prefix code, last byte, first byte, length
    let mut prefix = vec![0u16; 4096];
    let mut suffix = vec![0u8; 4096];
    let mut first = vec![0u8; 4096];
    let mut length = vec![0u32; 4096];
    for c in 0..256 {
        suffix[c] = c as u8;
        first[c] = c as u8;
        length[c] = 1;
    }
    // grows as codes come, up to about `limit` (one code adds at most 4096 bytes)
    let mut out: Vec<u8> = Vec::with_capacity(limit.min(input.len().saturating_mul(4)));
    let mut width = 9u32;
    let mut next = 258u16;
    let mut prev: Option<u16> = None;
    let (mut acc, mut nacc, mut pos) = (0u32, 0u32, 0usize);
    let bad = || "an LZW block is corrupt".to_string();
    loop {
        while nacc < width {
            if pos >= input.len() {
                return Ok(out); // ran out without EOI: keep what came out
            }
            acc = (acc << 8) | u32::from(input[pos]);
            pos += 1;
            nacc += 8;
        }
        let code = ((acc >> (nacc - width)) & ((1 << width) - 1)) as u16;
        nacc -= width;
        if code == EOI {
            return Ok(out);
        }
        if code == CLEAR {
            width = 9;
            next = 258;
            prev = None;
            continue;
        }
        let Some(p) = prev else {
            if code > 255 {
                return Err(bad());
            }
            out.push(code as u8);
            prev = Some(code);
            if out.len() >= limit {
                return Ok(out);
            }
            continue;
        };
        let (emit, first_byte) = if code < next {
            (code, first[code as usize])
        } else if code == next {
            (p, first[p as usize])
        } else {
            return Err(bad());
        };
        // write the string of `emit`, then the KwKwK byte if needed
        let n = length[emit as usize] as usize;
        let start = out.len();
        out.resize(start + n, 0);
        let mut c = emit;
        for i in (0..n).rev() {
            out[start + i] = suffix[c as usize];
            c = prefix[c as usize];
        }
        if code == next {
            out.push(first_byte);
        }
        if (next as usize) < 4096 {
            prefix[next as usize] = p;
            suffix[next as usize] = first_byte;
            first[next as usize] = first[p as usize];
            length[next as usize] = length[p as usize] + 1;
            next += 1;
            if next == (1 << width) - 1 && width < 12 {
                width += 1;
            }
        }
        prev = Some(code);
        if out.len() >= limit {
            return Ok(out);
        }
    }
}

/// PackBits (Apple), up to `limit` bytes.
fn packbits_decode(input: &[u8], limit: usize) -> TiffResult<Vec<u8>> {
    // a run byte gives at most 128 bytes; grow from the input's size
    let mut out = Vec::with_capacity(limit.min(input.len().saturating_mul(2)));
    let mut i = 0;
    while i < input.len() && out.len() < limit {
        let n = input[i] as i8;
        i += 1;
        if n >= 0 {
            let k = n as usize + 1;
            let s = input.get(i..i + k).ok_or("a PackBits block is corrupt")?;
            out.extend_from_slice(s);
            i += k;
        } else if n != -128 {
            let b = *input.get(i).ok_or("a PackBits block is corrupt")?;
            i += 1;
            out.extend(std::iter::repeat(b).take((1 - isize::from(n)) as usize));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packbits_round_trip_example() {
        // the example of the TIFF 6.0 specification
        let packed = [
            0xFEu8, 0xAA, 0x02, 0x80, 0x00, 0x2A, 0xFD, 0xAA, 0x03, 0x80, 0x00, 0x2A, 0x22, 0xF7,
            0xAA,
        ];
        let want = [
            0xAAu8, 0xAA, 0xAA, 0x80, 0x00, 0x2A, 0xAA, 0xAA, 0xAA, 0xAA, 0x80, 0x00, 0x2A, 0x22,
            0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
        ];
        assert_eq!(packbits_decode(&packed, 100).unwrap(), want);
    }

    #[test]
    fn lzw_decodes_a_hand_made_stream() {
        // clear, 'a', 'b', 258 ("ab"), 260 ("aba", KwKwK... no: next code), EOI
        // codes: 256 97 98 258 257 at 9 bits
        let codes = [256u16, 97, 98, 258, 257];
        let mut bits = Vec::new();
        for c in codes {
            for i in (0..9).rev() {
                bits.push((c >> i) & 1);
            }
        }
        while bits.len() % 8 != 0 {
            bits.push(0);
        }
        let bytes: Vec<u8> = bits
            .chunks(8)
            .map(|b| b.iter().fold(0u8, |a, &x| (a << 1) | x as u8))
            .collect();
        assert_eq!(lzw_decode(&bytes, 100).unwrap(), b"abab");
    }

    /// A little-endian classic GeoTIFF of `w` by `h` 16-bit signed pixels
    /// (value `y * w + x`) in one uncompressed strip, georeferenced in
    /// EPSG:4326, with extra entries `extra` as `(tag, type, count, value
    /// or offset)` replacing any of the same tag.
    pub(super) fn tiny(w: u16, h: u16, extra: &[(u16, u16, u32, u32)]) -> Vec<u8> {
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                pixels.extend_from_slice(&((y * w + x) as i16).to_le_bytes());
            }
        }
        // layout: header (8), doubles (scale 24 + tie 48), geokeys (32 shorts), pixels, IFD
        let mut b = b"II*\0\0\0\0\0".to_vec();
        let scale_at = b.len() as u32;
        for v in [0.001f64, 0.001, 0.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let tie_at = b.len() as u32;
        for v in [0.0f64, 0.0, 0.0, -46.65, -23.52, 0.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let keys_at = b.len() as u32;
        for v in [
            1u16, 1, 0, 3, 1024, 0, 1, 2, 1025, 0, 1, 1, 2048, 0, 1, 4326,
        ] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let pix_at = b.len() as u32;
        b.extend_from_slice(&pixels);
        let mut entries: Vec<(u16, u16, u32, u32)> = vec![
            (256, 3, 1, u32::from(w)),
            (257, 3, 1, u32::from(h)),
            (258, 3, 1, 16),
            (259, 3, 1, 1),
            (262, 3, 1, 1),
            (273, 4, 1, pix_at),
            (277, 3, 1, 1),
            (278, 3, 1, u32::from(h)),
            (279, 4, 1, pixels.len() as u32),
            (339, 3, 1, 2),
            (33550, 12, 3, scale_at),
            (33922, 12, 6, tie_at),
            (34735, 3, 16, keys_at),
        ];
        for &(t, ty, c, v) in extra {
            entries.retain(|e| e.0 != t);
            entries.push((t, ty, c, v));
        }
        entries.sort_by_key(|e| e.0);
        let ifd = b.len() as u32;
        b[4..8].copy_from_slice(&ifd.to_le_bytes());
        b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (t, ty, c, v) in entries {
            b.extend_from_slice(&t.to_le_bytes());
            b.extend_from_slice(&ty.to_le_bytes());
            b.extend_from_slice(&c.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        b
    }

    /// Parse and read every pixel: the first error, if any.
    fn read_all(b: &[u8]) -> TiffResult<f64> {
        let r = Raster::parse(b)?;
        let mut sum = 0.0;
        // a bounded sample of pixels (the image may claim to be huge)
        let (w, h) = (r.width.min(64), r.height.min(64));
        for y in 0..h {
            for x in 0..w {
                sum += r.value(x, y)?;
            }
        }
        Ok(sum)
    }

    #[test]
    fn the_tiny_file_reads() {
        let b = tiny(5, 4, &[]);
        let r = Raster::parse(&b).unwrap();
        assert_eq!((r.width, r.height), (5, 4));
        assert_eq!(r.value(3, 2).unwrap(), 13.0);
        assert!(r.value(5, 0).is_err());
        assert_eq!(r.georef.pixel(-46.6495, -23.5205, 5, 4), Ok(Some((0, 0))));
        assert_eq!(r.georef.pixel(-46.0, -23.0, 5, 4), Ok(None));
    }

    fn fails(b: &[u8], what: &str) {
        match read_all(b) {
            Ok(_) => panic!("expected an error about {what}"),
            Err(e) => assert!(e.contains(what), "error {e:?} does not mention {what:?}"),
        }
    }

    #[test]
    fn malformed_headers_and_directories_are_errors() {
        fails(b"", "not a TIFF");
        fails(b"II", "truncated");
        fails(b"II*\0", "truncated");
        fails(b"XX*\0\x08\0\0\0", "not a TIFF");
        fails(b"II\x2b\0\x04\0\0\0", "offset size");
        let mut b = tiny(3, 3, &[]);
        b[4..8].copy_from_slice(&4u32.to_le_bytes());
        fails(&b, "overlaps the file header");
        let mut b = tiny(3, 3, &[]);
        b[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        fails(&b, "truncated");
        // an entry count far beyond the file
        let mut b = tiny(3, 3, &[]);
        let ifd = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
        b[ifd..ifd + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        fails(&b, "runs past the end");
    }

    #[test]
    fn malformed_entries_are_errors() {
        // the width with an unknown field type and a huge count
        fails(
            &tiny(3, 3, &[(256, 99, u32::MAX, 3)]),
            "not an integer type",
        );
        // strip offsets: more than the blocks of the image
        fails(
            &tiny(3, 3, &[(273, 4, 1 << 30, 8)]),
            "block offsets or byte counts",
        );
        fails(&tiny(3, 3, &[(256, 3, 0, 0)]), "empty");
        fails(&tiny(3, 3, &[(256, 3, 1, 0)]), "an image of 0 by 3");
        // a strip and its byte count past the end of the file
        fails(&tiny(3, 3, &[(273, 4, 1, u32::MAX - 4)]), "past its end");
        fails(&tiny(3, 3, &[(279, 4, 1, u32::MAX)]), "past its end");
        fails(&tiny(3, 3, &[(279, 4, 1, 0)]), "missing");
        // fewer strips than the image needs
        fails(&tiny(3, 3, &[(278, 3, 1, 1)]), "block offsets");
        // samples, formats, predictors
        fails(&tiny(3, 3, &[(258, 3, 1, 12)]), "not supported");
        fails(
            &tiny(3, 3, &[(258, 3, 1, 64), (339, 3, 1, 2)]),
            "64-bit integer",
        );
        fails(&tiny(3, 3, &[(317, 3, 1, 3)]), "predictor 3");
        fails(&tiny(3, 3, &[(317, 3, 1, 9)]), "predictor 9");
        fails(&tiny(3, 3, &[(277, 3, 1, 2)]), "2 bands");
        fails(&tiny(3, 3, &[(259, 3, 1, 50000)]), "ZSTD");
        fails(&tiny(3, 3, &[(259, 3, 1, 7)]), "JPEG");
    }

    #[test]
    fn malformed_blocks_are_errors() {
        // tiles of 0 pixels, and of more than a block may hold
        let tiled = |tw: u32, th: u32| {
            tiny(
                3,
                3,
                &[
                    (322, 4, 1, tw),
                    (323, 4, 1, th),
                    (324, 4, 1, 8),
                    (325, 4, 1, 18),
                ],
            )
        };
        fails(&tiled(0, 16), "blocks of 0 by 16");
        fails(&tiled(1 << 16, 1 << 16), "more than the");
        fails(&tiled(!15, !15), "more than the");
        fails(&tiled(1, 1), "multiples of 16");
        // a block that decodes short
        fails(&tiny(3, 3, &[(279, 4, 1, 4)]), "decodes to");
        // compressed garbage
        fails(&tiny(3, 3, &[(259, 3, 1, 8)]), "Deflate");
        let lzw = tiny(3, 3, &[(259, 3, 1, 5)]);
        assert!(read_all(&lzw).is_err());
        let pack = tiny(3, 3, &[(259, 3, 1, 32773)]);
        assert!(read_all(&pack).is_err());
    }

    /// `tiny` with its strip replaced by `strip`, appended after the IFD.
    fn with_strip(compression: u32, strip: &[u8]) -> Vec<u8> {
        let n = strip.len() as u32;
        let at = tiny(
            3,
            3,
            &[(259, 3, 1, compression), (273, 4, 1, 0), (279, 4, 1, n)],
        )
        .len();
        let mut b = tiny(
            3,
            3,
            &[
                (259, 3, 1, compression),
                (273, 4, 1, at as u32),
                (279, 4, 1, n),
            ],
        );
        b.extend_from_slice(strip);
        b
    }

    #[test]
    fn huge_block_tables_fail_fast() {
        // a 4 Gi x 4 Gi pixel image in the smallest tiles: 2^56 blocks,
        // declared with one offset, or with 2^28 offsets the file lacks
        let t0 = std::time::Instant::now();
        let huge = |count: u32| {
            tiny(
                3,
                3,
                &[
                    (256, 4, 1, u32::MAX),
                    (257, 4, 1, u32::MAX),
                    (322, 4, 1, 16),
                    (323, 4, 1, 16),
                    (324, 4, count, 8),
                    (325, 4, count, 8),
                ],
            )
        };
        fails(&huge(1), "for the 72057594037927936 blocks");
        fails(&huge(u32::MAX), "block offsets or byte counts");
        // a strip per row of a tall image: as many offsets as rows, which
        // must all be in the file
        fails(
            &tiny(
                3,
                3,
                &[(257, 4, 1, 1 << 30), (278, 3, 1, 1), (273, 4, 1 << 30, 8)],
            ),
            "truncated",
        );
        assert!(t0.elapsed().as_millis() < 500);
        // many blocks that are present: kept sparse, and each one read
        // counts against the budget
        let w = 16 * 300u32;
        let n = 300 * 300;
        let make = |at: u32| {
            tiny(
                3,
                3,
                &[
                    (256, 4, 1, w),
                    (257, 4, 1, w),
                    (322, 4, 1, 16),
                    (323, 4, 1, 16),
                    (324, 4, n, at),
                    (325, 4, n, at + 4 * n),
                ],
            )
        };
        let at = make(0).len() as u32;
        let mut b = make(at);
        // offsets all pointing at the same 512 bytes, byte counts 512
        let pix = (b.len() as u32 + 8 * n).to_le_bytes();
        let pad = b.len();
        b.resize(pad + 8 * n as usize, 0);
        for k in 0..n as usize {
            b[pad + 4 * k..pad + 4 * k + 4].copy_from_slice(&pix);
            let c = pad + 4 * n as usize + 4 * k;
            b[c..c + 4].copy_from_slice(&512u32.to_le_bytes());
        }
        b.resize(b.len() + 512, 1);
        // (the IFD's offsets point at `at`, where the tables begin)
        assert_eq!(pad as u32, at);
        let r = Raster::parse(&b).unwrap();
        assert!(matches!(r.decoded, Slots::Sparse(_)));
        assert_eq!(
            r.value(w - 1, w - 1).unwrap(),
            f64::from(i16::from_le_bytes([1, 1]))
        );
    }

    #[test]
    fn a_deflate_bomb_stops_at_the_block_size() {
        // 64 MiB of zeros deflated to about 64 KiB, as a 3 x 3 strip
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&vec![0u8; 64 << 20], 9);
        let file = with_strip(8, &packed);
        let r = Raster::parse(&file).unwrap();
        assert_eq!(r.value(2, 2).unwrap(), 0.0);
        // PackBits runs and LZW codes past the strip's size stop there too
        let runs: Vec<u8> = std::iter::repeat([0x81u8, 7])
            .take(10_000)
            .flatten()
            .collect();
        let file = with_strip(32773, &runs);
        let r = Raster::parse(&file).unwrap();
        assert_eq!(
            r.value(0, 0).unwrap(),
            f64::from(i16::from_le_bytes([7, 7]))
        );
    }

    #[test]
    fn geokeys_and_georeferencing() {
        // a key count far beyond the keys present
        let b = tiny(3, 3, &[]);
        let keys_at = 8 + 24 + 48;
        let mut c = b.clone();
        c[keys_at + 6..keys_at + 8].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(read_all(&c).is_ok());
        // no CRS keys
        fails(
            &tiny(3, 3, &[(34735, 3, 4, 8 + 24 + 48)]),
            "no coordinate reference system",
        );
        // a zero pixel scale, and no georeferencing
        let mut z = b.clone();
        z[8..16].copy_from_slice(&0f64.to_le_bytes());
        fails(&z, "positive scales");
        let mut s = b.clone();
        s[8..16].copy_from_slice(&f64::MIN_POSITIVE.to_le_bytes());
        fails(&s, "no usable grid");
        let mut n = b;
        n[16..24].copy_from_slice(&f64::NAN.to_le_bytes());
        fails(&n, "positive scales");
    }

    /// `tiny(5, 4)` with its GeoTIFF key directory replaced by `keys`
    /// (`(id, value)`, held in the directory).
    pub(super) fn with_keys(keys: &[(u16, u16)]) -> Vec<u8> {
        let n = keys.len() as u32;
        let at = tiny(5, 4, &[(34735, 3, 4 + 4 * n, 0)]).len() as u32;
        let mut b = tiny(5, 4, &[(34735, 3, 4 + 4 * n, at)]);
        for v in [1u16, 1, 0, keys.len() as u16] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for &(id, v) in keys {
            for w in [id, 0, 1, v] {
                b.extend_from_slice(&w.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn coordinate_reference_systems() {
        let crs = |keys: &[(u16, u16)]| Raster::parse(&with_keys(keys)).map(|r| r.crs.code);
        assert_eq!(crs(&[(1024, 2), (2048, 4326)]), Ok(4326));
        assert_eq!(crs(&[(1024, 2), (2048, 4674), (2054, 9102)]), Ok(4674));
        assert_eq!(crs(&[(1024, 1), (3072, 31982), (3076, 9001)]), Ok(31982));
        // the model type chooses which key counts
        assert_eq!(crs(&[(1024, 1), (2048, 4674), (3072, 32722)]), Ok(32722));
        assert_eq!(crs(&[(1024, 2), (2048, 4618), (3072, 32722)]), Ok(4618));
        let err = |keys: &[(u16, u16)], what: &str| {
            let e = crs(keys).unwrap_err();
            assert!(e.contains(what), "{e:?} does not mention {what:?}");
        };
        err(&[(1024, 1), (3072, 32767)], "user-defined");
        err(&[(1024, 1)], "user-defined");
        err(&[(1024, 2), (2048, 32767)], "user-defined");
        err(&[(1024, 1), (3072, 2193)], "EPSG:2193");
        // a projected code given as geographic, and the reverse
        err(&[(1024, 2), (2048, 31982)], "EPSG:31982");
        err(&[(1024, 1), (3072, 4674)], "EPSG:4674");
        err(&[(1024, 3)], "geocentric");
        err(&[(1024, 7)], "model type 7");
        err(
            &[(1024, 1), (3072, 31982), (3076, 9002)],
            "linear unit 9002",
        );
        err(
            &[(1024, 2), (2048, 4674), (2054, 9101)],
            "angular unit 9101",
        );
        // a key held in another tag (location != 0) is not read
        let mut b = with_keys(&[(1024, 1), (3072, 31982)]);
        let last = b.len() - 6;
        b[last..last + 2].copy_from_slice(&34737u16.to_le_bytes());
        assert!(Raster::parse(&b).err().unwrap().contains("user-defined"));
    }

    #[test]
    fn the_nodata_tag() {
        // "-9999\0" after the file, as GDAL writes it
        let text = b"-9999\0";
        let nodata = |entry: (u16, u16, u32, u32), extra: &[u8]| {
            let at = tiny(3, 3, &[entry]).len() as u32;
            let (t, ty, c, _) = entry;
            let mut b = tiny(3, 3, &[(t, ty, c, at)]);
            b.extend_from_slice(extra);
            Raster::parse(&b).map(|r| r.nodata)
        };
        assert_eq!(nodata((42113, 2, 6, 0), text), Ok(Some(-9999.0)));
        assert!(nodata((42113, 2, 5, 0), b"nan\0\0")
            .unwrap()
            .unwrap()
            .is_nan());
        // not a number, too long, past the end, another type: no value
        assert_eq!(nodata((42113, 2, 5, 0), b"abcd\0"), Ok(None));
        assert_eq!(nodata((42113, 2, 1000, 0), &[b'1'; 1000]), Ok(None));
        assert_eq!(nodata((42113, 2, 60, 0), text), Ok(None));
        assert_eq!(nodata((42113, 3, 6, 0), text), Ok(None));
        assert_eq!(Raster::parse(&tiny(3, 3, &[])).map(|r| r.nodata), Ok(None));
    }

    #[test]
    fn near_a_pixel_boundary_strictmath_decides_and_the_sample_is_counted() {
        // tiny(5, 4) in SIRGAS 2000: pixels 0.001 degrees wide from -46.65
        let b = with_keys(&[(1024, 2), (2048, 4674)]);
        let c = crs::lookup(4674).unwrap();
        let lat = -23.5215;
        // the WGS 84 longitude that SIRGAS 2000 puts on the boundary between
        // columns 1 and 2 (Newton steps; the transform is nearly the identity)
        let target = -46.65 + 2.0 * 0.001;
        let mut lon = target;
        for _ in 0..4 {
            lon += target - c.apply(lon, lat).0;
        }
        let near = |d: f64| crate::elevation::read_points(&b, &[(lon + d, lat)]).unwrap();
        // on the boundary: read as StrictMath's point falls, and flagged
        let on = near(0.0);
        let [Ok((v, true))] = on[..] else {
            panic!("{on:?}")
        };
        assert!(v == 6.0 || v == 7.0, "{v}");
        // a micrometre away it is not ambiguous
        assert!(
            matches!(near(1e-11)[..], [Ok((7.0, false))]),
            "{:?}",
            near(1e-11)
        );
        assert!(
            matches!(near(-1e-11)[..], [Ok((6.0, false))]),
            "{:?}",
            near(-1e-11)
        );
    }

    #[test]
    fn strictmath_on_the_outer_edge_is_still_an_error() {
        // in WGS 84, the envelope's east edge itself is inside the envelope
        // but rounds past the last pixel: GeoTools throws, r5r fails
        let b = tiny(5, 4, &[]);
        let east = (5.0 - 0.5) * 0.001 + (-46.65 + 0.5 * 0.001);
        let r = crate::elevation::read_points(&b, &[(east, -23.5215)]).unwrap();
        assert!(
            matches!(&r[..], [Err(e)] if e.contains("outer edge")),
            "{r:?}"
        );
    }

    /// splitmix64
    fn next(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    #[test]
    fn the_fuzz_seeds_read() {
        for b in [
            &include_bytes!("../../tests/data/fuzz_lzw.tif")[..],
            &include_bytes!("../../tests/data/fuzz_p3.tif")[..],
            &include_bytes!("../../tests/data/fuzz_big.tif")[..],
        ] {
            let r = Raster::parse(b).unwrap();
            assert_eq!((r.width, r.height), (40, 30));
            assert!(read_all(b).unwrap() > 0.0);
        }
    }

    #[test]
    fn mutated_files_never_panic() {
        let hills = include_bytes!("../../tests/data/spo_hills.tif").to_vec();
        // GDAL's output in the layouts the reader takes (40 x 30 crops of a
        // synthetic raster): LZW + horizontal predictor in 16 x 16 tiles,
        // big-endian Deflate + floating-point predictor, BigTIFF PackBits
        let seeds: Vec<Vec<u8>> = vec![
            hills,
            include_bytes!("../../tests/data/fuzz_lzw.tif").to_vec(),
            include_bytes!("../../tests/data/fuzz_p3.tif").to_vec(),
            include_bytes!("../../tests/data/fuzz_big.tif").to_vec(),
            tiny(7, 5, &[]),
            with_keys(&[(1024, 1), (3072, 31982), (3076, 9001)]),
            with_keys(&[(1024, 2), (2048, 4674), (2054, 9102)]),
            include_bytes!("../../tests/data/spo_hills_31983.tif").to_vec(),
            include_bytes!("../../tests/data/spo_hills_4674.tif").to_vec(),
            tiny(
                7,
                5,
                &[
                    (322, 4, 1, 16),
                    (323, 4, 1, 16),
                    (324, 4, 1, 200),
                    (325, 4, 1, 512),
                ],
            ),
        ];
        let mut state = 42u64;
        let t0 = std::time::Instant::now();
        let mut read = 0;
        // PEABIRU_TIFF_FUZZ=<n> runs n mutations (default 20000) and seeds them by n
        let n: usize = std::env::var("PEABIRU_TIFF_FUZZ")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20_000);
        state ^= n as u64;
        for i in 0..n {
            let mut b = seeds[i % seeds.len()].clone();
            let edits = 1 + next(&mut state) % 8;
            for _ in 0..edits {
                let at = (next(&mut state) % b.len() as u64) as usize;
                match next(&mut state) % 4 {
                    0 => b[at] = next(&mut state) as u8,
                    1 => b[at] ^= 1 << (next(&mut state) % 8),
                    2 => b[at] = [0u8, 0xff, 0x7f, 0x80][(next(&mut state) % 4) as usize],
                    _ => b.truncate(at.max(1)),
                }
            }
            if read_all(&b).is_ok() {
                read += 1;
            }
        }
        assert!(read > 0);
        assert!(
            n > 20_000 || t0.elapsed().as_secs() < 60,
            "fuzzing took {:?}",
            t0.elapsed()
        );
    }
}
