//! A reader for the subset of GeoTIFF that elevation rasters use, written
//! from the TIFF 6.0 and GeoTIFF 1.1 specifications: classic and BigTIFF,
//! either byte order, one band of 8, 16 or 32-bit integers or 32 or 64-bit
//! floats, strips or tiles, no compression, LZW, Deflate or
//! PackBits, horizontal (2) and floating-point (3) predictors, and the
//! georeferencing of one tie point and a pixel scale in WGS 84 longitude
//! and latitude (EPSG:4326). Anything else is an error that says what is
//! not supported. Every combination of these was compared with what R5
//! (GeoTools 25 and imageio-ext) reads; where R5 fails (several bands, LZW
//! with the floating-point predictor, the horizontal predictor on 64-bit
//! samples, 64-bit integers, ZSTD, no georeferencing) this reader fails
//! too, and where R5 reprojects (any other coordinate reference system) or
//! decodes lossily (JPEG) it refuses the raster.
//!
//! The blocks (strips or tiles) are decoded only when a sample falls in
//! them, once each, and may be read from several threads.

use std::sync::OnceLock;

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
        let n = usize::try_from(e.count).map_err(|_| format!("tag {tag} is too long"))?;
        let size = Self::type_size(e.typ).unwrap_or(0);
        if n.checked_mul(size).map_or(true, |t| t > self.b.len()) {
            return Err(format!("tag {tag} is truncated"));
        }
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

    /// DOUBLE values of an entry (the GeoTIFF tags).
    fn doubles(&self, e: Entry, tag: u16) -> TiffResult<Vec<f64>> {
        if e.typ != 12 {
            return Err(format!("tag {tag} has type {}, not DOUBLE", e.typ));
        }
        let n = usize::try_from(e.count).map_err(|_| format!("tag {tag} is too long"))?;
        if n.checked_mul(8).map_or(true, |t| t > self.b.len()) {
            return Err(format!("tag {tag} is truncated"));
        }
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
    offsets: Vec<u64>,
    counts: Vec<u64>,
    pub(crate) georef: Georef,
    decoded: Vec<OnceLock<TiffResult<Vec<f64>>>>,
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

const KEY_MODEL_TYPE: u16 = 1024;
const KEY_RASTER_TYPE: u16 = 1025;
const KEY_GEOGRAPHIC_TYPE: u16 = 2048;
const KEY_GEOG_ANGULAR_UNITS: u16 = 2054;
const KEY_PROJECTED_TYPE: u16 = 3072;

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
        let (n, mut at, step) = if r.big {
            (r.u64(ifd)?, ifd + 8, 20)
        } else {
            (u64::from(r.u16(ifd)?), ifd + 2, 12)
        };
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
            let offsets = r.ints(
                get(TAG_TILE_OFFSETS).ok_or("tiles without offsets")?,
                TAG_TILE_OFFSETS,
            )?;
            let counts = r.ints(
                get(TAG_TILE_COUNTS).ok_or("tiles without byte counts")?,
                TAG_TILE_COUNTS,
            )?;
            (tw, th, offsets, counts)
        } else {
            let rows = int1(TAG_ROWS_PER_STRIP, Some(u64::from(height)))?.min(u64::from(height));
            let offsets = r.ints(
                get(TAG_STRIP_OFFSETS).ok_or("strips without offsets")?,
                TAG_STRIP_OFFSETS,
            )?;
            let counts = r.ints(
                get(TAG_STRIP_COUNTS).ok_or("strips without byte counts")?,
                TAG_STRIP_COUNTS,
            )?;
            (u64::from(width), rows, offsets, counts)
        };
        if block_w == 0 || block_h == 0 || block_w > 1 << 20 || block_h > 1 << 24 {
            return Err(format!("blocks of {block_w} by {block_h} pixels"));
        }
        let (block_w, block_h) = (block_w as u32, block_h as u32);
        let blocks_across = width.div_ceil(block_w);
        let blocks_down = height.div_ceil(block_h);
        let n_blocks = blocks_across as usize * blocks_down as usize;
        if offsets.len() < n_blocks || counts.len() < n_blocks {
            return Err(format!(
                "{} block offsets and {} byte counts for {n_blocks} blocks",
                offsets.len(),
                counts.len()
            ));
        }

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
            let n = keys[3] as usize;
            (0..n).find_map(|i| {
                let k = keys.get(4 + 4 * i..8 + 4 * i)?;
                // only keys held in the directory itself (location 0)
                (k[0] == u64::from(id) && k[1] == 0).then_some(k[3])
            })
        };
        let model = key(KEY_MODEL_TYPE);
        let geographic = key(KEY_GEOGRAPHIC_TYPE);
        if model != Some(2) || geographic != Some(4326) || key(KEY_PROJECTED_TYPE).is_some() {
            return Err(format!(
                "the coordinate reference system is not WGS 84 longitude and latitude \
                 (EPSG:4326): model type {}, geographic type {}; r5r converts other systems \
                 with GeoTools, which this reader does not reproduce: reproject the raster to \
                 EPSG:4326 (for instance with gdalwarp -t_srs EPSG:4326)",
                model.map_or("none".into(), |v| v.to_string()),
                geographic.map_or("none".into(), |v| v.to_string()),
            ));
        }
        if let Some(u) = key(KEY_GEOG_ANGULAR_UNITS) {
            if u != 9102 {
                return Err(format!(
                    "angular unit {u} (not degrees, 9102) for EPSG:4326 is not supported"
                ));
            }
        }
        let point = match key(KEY_RASTER_TYPE) {
            None | Some(1) => false,
            Some(2) => true,
            Some(t) => return Err(format!("raster type {t} is not supported")),
        };
        let georef = Georef::new(sx, sy, tie[3], tie[4], point, width, height);
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
            georef,
            decoded: (0..n_blocks).map(|_| OnceLock::new()).collect(),
        })
    }

    /// The value of pixel `(x, y)` as GeoTools' `getPixel` gives it (a
    /// double: integers exactly, floats widened).
    pub(crate) fn value(&self, x: u32, y: u32) -> TiffResult<f64> {
        let bx = x / self.block_w;
        let by = y / self.block_h;
        let k = by as usize * self.blocks_across as usize + bx as usize;
        let block = self.decoded[k]
            .get_or_init(|| self.decode(k, by))
            .as_ref()
            .map_err(Clone::clone)?;
        let i = (y - by * self.block_h) as usize * self.block_w as usize
            + (x - bx * self.block_w) as usize;
        Ok(block[i])
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
        let off = usize::try_from(self.offsets[k]).map_err(|_| "a block is out of reach")?;
        let len = usize::try_from(self.counts[k]).map_err(|_| "a block is out of reach")?;
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
        let mut out = Vec::with_capacity(self.block_w as usize * rows);
        let mut row = vec![0u8; row_bytes];
        for r in 0..rows {
            row.copy_from_slice(&data[r * row_bytes..(r + 1) * row_bytes]);
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
    let mut out: Vec<u8> = Vec::with_capacity(limit);
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
    let mut out = Vec::with_capacity(limit);
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
}
