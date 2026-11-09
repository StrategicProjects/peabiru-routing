//! Minimal protocol-buffers wire-format reader, enough for the OSM PBF
//! messages. Only the wire types used there are supported; anything else is
//! reported as an error rather than guessed at.

use std::fmt;

/// A malformed or unsupported protobuf message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(pub String);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

pub type Result<T> = std::result::Result<T, DecodeError>;

fn err<T>(msg: &str) -> Result<T> {
    Err(DecodeError(msg.to_string()))
}

/// The value of one field, as found on the wire.
#[derive(Debug, Clone, Copy)]
pub enum Value<'a> {
    Varint(u64),
    Fixed64,
    Bytes(&'a [u8]),
    Fixed32,
}

impl<'a> Value<'a> {
    pub fn as_u64(self) -> Result<u64> {
        match self {
            Value::Varint(v) => Ok(v),
            _ => err("expected a varint field"),
        }
    }

    pub fn as_i64(self) -> Result<i64> {
        self.as_u64().map(|v| v as i64)
    }

    pub fn as_sint64(self) -> Result<i64> {
        self.as_u64().map(zigzag)
    }

    pub fn as_bytes(self) -> Result<&'a [u8]> {
        match self {
            Value::Bytes(b) => Ok(b),
            _ => err("expected a length-delimited field"),
        }
    }
}

/// Decode a zigzag-encoded signed integer.
#[inline]
pub fn zigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

/// Iterator over the `(field number, value)` pairs of one message.
pub struct Fields<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Fields<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Fields { buf, pos: 0 }
    }

    fn varint(&mut self) -> Result<u64> {
        read_varint(self.buf, &mut self.pos)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.buf.len());
        match end {
            Some(end) => {
                let out = &self.buf[self.pos..end];
                self.pos = end;
                Ok(out)
            }
            None => err("field runs past the end of the message"),
        }
    }
}

impl<'a> Iterator for Fields<'a> {
    type Item = Result<(u32, Value<'a>)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.buf.len() {
            return None;
        }
        Some((|| {
            let key = self.varint()?;
            let field = (key >> 3) as u32;
            let value = match key & 7 {
                0 => Value::Varint(self.varint()?),
                1 => {
                    self.take(8)?;
                    Value::Fixed64
                }
                2 => {
                    let len = self.varint()? as usize;
                    Value::Bytes(self.take(len)?)
                }
                5 => {
                    self.take(4)?;
                    Value::Fixed32
                }
                w => return err(&format!("unsupported wire type {w}")),
            };
            Ok((field, value))
        })())
    }
}

/// Read one base-128 varint at `*pos`, advancing it.
#[inline]
pub fn read_varint(buf: &[u8], pos: &mut usize) -> Result<u64> {
    let mut out = 0u64;
    let mut shift = 0u32;
    loop {
        let Some(&b) = buf.get(*pos) else {
            return err("truncated varint");
        };
        *pos += 1;
        if shift >= 64 {
            return err("varint longer than 64 bits");
        }
        out |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(out);
        }
        shift += 7;
    }
}

/// Iterator over a packed repeated varint field.
pub struct Packed<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Packed<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Packed { buf, pos: 0 }
    }
}

impl Iterator for Packed<'_> {
    type Item = Result<u64>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.buf.len() {
            None
        } else {
            Some(read_varint(self.buf, &mut self.pos))
        }
    }
}

/// Decode a packed repeated `uint32`/`int32`/`uint64` field.
pub fn packed_u64(buf: &[u8]) -> Result<Vec<u64>> {
    Packed::new(buf).collect()
}

/// Decode a packed repeated `sint64` field that is delta-coded, returning the
/// running sums.
pub fn packed_delta_sint64(buf: &[u8]) -> Result<Vec<i64>> {
    let mut acc = 0i64;
    Packed::new(buf)
        .map(|v| {
            acc = acc.wrapping_add(zigzag(v?));
            Ok(acc)
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod encode {
    //! A tiny encoder used by the tests to build PBF files by hand.

    pub fn varint(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    pub fn zz(v: i64) -> u64 {
        ((v << 1) ^ (v >> 63)) as u64
    }

    pub fn field_varint(out: &mut Vec<u8>, field: u32, v: u64) {
        varint(out, u64::from(field) << 3);
        varint(out, v);
    }

    pub fn field_bytes(out: &mut Vec<u8>, field: u32, b: &[u8]) {
        varint(out, (u64::from(field) << 3) | 2);
        varint(out, b.len() as u64);
        out.extend_from_slice(b);
    }

    pub fn packed(values: impl IntoIterator<Item = u64>) -> Vec<u8> {
        let mut out = Vec::new();
        for v in values {
            varint(&mut out, v);
        }
        out
    }

    pub fn packed_delta(values: &[i64]) -> Vec<u8> {
        let mut prev = 0i64;
        packed(values.iter().map(|&v| {
            let d = v - prev;
            prev = v;
            zz(d)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::encode::*;
    use super::*;

    #[test]
    fn varints_and_zigzag_roundtrip() {
        for v in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX] {
            let mut b = Vec::new();
            varint(&mut b, v);
            let mut pos = 0;
            assert_eq!(read_varint(&b, &mut pos).unwrap(), v);
            assert_eq!(pos, b.len());
        }
        for v in [0i64, -1, 1, -64, 64, i64::MIN, i64::MAX] {
            assert_eq!(zigzag(zz(v)), v);
        }
    }

    #[test]
    fn delta_coded_packed_field() {
        let ids = [10i64, 12, 11, -5, 1_000_000_000_000];
        assert_eq!(packed_delta_sint64(&packed_delta(&ids)).unwrap(), ids);
    }

    #[test]
    fn truncated_input_is_an_error() {
        let mut b = Vec::new();
        field_bytes(&mut b, 1, b"hello");
        b.truncate(4);
        assert!(Fields::new(&b).next().unwrap().is_err());
    }
}
