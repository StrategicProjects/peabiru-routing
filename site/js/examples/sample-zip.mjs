// A small .zip file written in memory (its files stored, not compressed),
// for the examples that make up a GTFS feed.

const CRC = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c;
});

function crc32(bytes) {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC[(c ^ b) & 255] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** The bytes of a .zip file of `files`, an object {name: text}. */
export function storedZip(files) {
  const enc = new TextEncoder();
  const u16 = (v) => [v & 255, (v >>> 8) & 255];
  const u32 = (v) => [v & 255, (v >>> 8) & 255, (v >>> 16) & 255, (v >>> 24) & 255];
  const local = [];
  const central = [];
  let offset = 0;
  for (const [name, text] of Object.entries(files)) {
    const n = enc.encode(name);
    const data = enc.encode(text);
    // version, flags, method 0 (stored), time and date 0, CRC-32, sizes
    const common = [20, 0, 0, 0, 0, 0, 0, 0, 0, 0, ...u32(crc32(data)), ...u32(data.length), ...u32(data.length), ...u16(n.length)];
    const head = Uint8Array.from([...u32(0x04034b50), ...common, 0, 0]);
    local.push(head, n, data);
    central.push(Uint8Array.from([...u32(0x02014b50), 20, 0, ...common, ...new Array(12).fill(0), ...u32(offset)]), n);
    offset += head.length + n.length + data.length;
  }
  const size = central.reduce((s, a) => s + a.length, 0);
  const count = Object.keys(files).length;
  const end = Uint8Array.from([...u32(0x06054b50), 0, 0, 0, 0, ...u16(count), ...u16(count), ...u32(size), ...u32(offset), 0, 0]);
  const parts = [...local, ...central, end];
  const out = new Uint8Array(parts.reduce((s, a) => s + a.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}
