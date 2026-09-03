//! A small, self-contained big-endian NBT codec - just enough to round-trip
//! the tag types Minecraft's own save files use (`servers.dat`, `level.dat`,
//! etc.), without pulling in a full NBT crate. Supports both directions so
//! callers can read a file, tweak part of it, and write it back while
//! preserving every field they didn't touch - not just extract a few known
//! keys. Not a general-purpose library: no gzip handling (`servers.dat` is
//! stored uncompressed, unlike `level.dat`).

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Nbt {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<Nbt>),
    Compound(HashMap<String, Nbt>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Nbt {
    fn tag_id(&self) -> u8 {
        match self {
            Nbt::Byte(_) => 1,
            Nbt::Short(_) => 2,
            Nbt::Int(_) => 3,
            Nbt::Long(_) => 4,
            Nbt::Float(_) => 5,
            Nbt::Double(_) => 6,
            Nbt::ByteArray(_) => 7,
            Nbt::String(_) => 8,
            Nbt::List(_) => 9,
            Nbt::Compound(_) => 10,
            Nbt::IntArray(_) => 11,
            Nbt::LongArray(_) => 12,
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> anyhow::Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or_else(|| anyhow::anyhow!("NBT length overflow"))?;
        let slice = self.data.get(self.pos..end).ok_or_else(|| anyhow::anyhow!("Unexpected end of NBT data"))?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> anyhow::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn i8(&mut self) -> anyhow::Result<i8> {
        Ok(self.u8()? as i8)
    }

    fn i16(&mut self) -> anyhow::Result<i16> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> anyhow::Result<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> anyhow::Result<i64> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> anyhow::Result<f32> {
        Ok(f32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f64(&mut self) -> anyhow::Result<f64> {
        Ok(f64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn string(&mut self) -> anyhow::Result<String> {
        let len = self.i16()? as u16 as usize;
        Ok(String::from_utf8_lossy(self.take(len)?).into_owned())
    }
}

fn parse_value(r: &mut Reader, tag_type: u8) -> anyhow::Result<Nbt> {
    Ok(match tag_type {
        1 => Nbt::Byte(r.i8()?),
        2 => Nbt::Short(r.i16()?),
        3 => Nbt::Int(r.i32()?),
        4 => Nbt::Long(r.i64()?),
        5 => Nbt::Float(r.f32()?),
        6 => Nbt::Double(r.f64()?),
        7 => {
            let len = r.i32()?.max(0) as usize;
            let mut v = Vec::with_capacity(len.min(1 << 16));
            for _ in 0..len {
                v.push(r.i8()?);
            }
            Nbt::ByteArray(v)
        }
        8 => Nbt::String(r.string()?),
        9 => {
            let elem_type = r.u8()?;
            let len = r.i32()?.max(0);
            let mut v = Vec::new();
            for _ in 0..len {
                v.push(parse_value(r, elem_type)?);
            }
            Nbt::List(v)
        }
        10 => Nbt::Compound(parse_compound_map(r)?),
        11 => {
            let len = r.i32()?.max(0) as usize;
            let mut v = Vec::with_capacity(len.min(1 << 16));
            for _ in 0..len {
                v.push(r.i32()?);
            }
            Nbt::IntArray(v)
        }
        12 => {
            let len = r.i32()?.max(0) as usize;
            let mut v = Vec::with_capacity(len.min(1 << 16));
            for _ in 0..len {
                v.push(r.i64()?);
            }
            Nbt::LongArray(v)
        }
        other => anyhow::bail!("Unsupported NBT tag type {other}"),
    })
}

fn parse_compound_map(r: &mut Reader) -> anyhow::Result<HashMap<String, Nbt>> {
    let mut map = HashMap::new();
    loop {
        let tag_type = r.u8()?;
        if tag_type == 0 {
            break;
        }
        let name = r.string()?;
        let value = parse_value(r, tag_type)?;
        map.insert(name, value);
    }
    Ok(map)
}

/// Parses a whole NBT file into its root compound (the root tag's own name,
/// conventionally empty, is read and discarded).
pub fn parse(data: &[u8]) -> anyhow::Result<Nbt> {
    let mut r = Reader { data, pos: 0 };
    let tag_type = r.u8()?;
    if tag_type != 10 {
        anyhow::bail!("Root NBT tag is not a compound");
    }
    let _name = r.string()?;
    Ok(Nbt::Compound(parse_compound_map(&mut r)?))
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn write_payload(out: &mut Vec<u8>, value: &Nbt) {
    match value {
        Nbt::Byte(v) => out.push(*v as u8),
        Nbt::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        Nbt::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        Nbt::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        Nbt::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
        Nbt::Double(v) => out.extend_from_slice(&v.to_be_bytes()),
        Nbt::ByteArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for b in v {
                out.push(*b as u8);
            }
        }
        Nbt::String(s) => write_string(out, s),
        Nbt::List(items) => {
            let elem_type = items.first().map(Nbt::tag_id).unwrap_or(0);
            out.push(elem_type);
            out.extend_from_slice(&(items.len() as i32).to_be_bytes());
            for item in items {
                write_payload(out, item);
            }
        }
        Nbt::Compound(map) => {
            for (key, val) in map {
                out.push(val.tag_id());
                write_string(out, key);
                write_payload(out, val);
            }
            out.push(0);
        }
        Nbt::IntArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for i in v {
                out.extend_from_slice(&i.to_be_bytes());
            }
        }
        Nbt::LongArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for i in v {
                out.extend_from_slice(&i.to_be_bytes());
            }
        }
    }
}

/// Serializes a root compound back to file bytes, with an empty root name -
/// the counterpart to `parse`.
pub fn write_root(root: &Nbt) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(root.tag_id());
    write_string(&mut out, "");
    write_payload(&mut out, root);
    out
}
