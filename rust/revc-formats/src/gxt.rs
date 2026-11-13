//! Parser for Vice City GXT (game text) files.

use crate::bytes::{Reader, c_str};
use crate::{Error, Result};

/// A single GXT table containing key-value pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// The name of this table (e.g., "MAIN", "M1").
    pub name: String,
    /// Entries in the order they appear in the file.
    pub entries: Vec<(String, Vec<u16>)>,
}

impl Table {
    /// Look up a key by exact string match, returning the string units if found.
    pub fn get(&self, key: &str) -> Option<&[u16]> {
        self.entries
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v.as_slice())
    }
}

/// A parsed GXT file containing all tables.
#[derive(Debug, Clone, PartialEq)]
pub struct Gxt {
    /// All tables in the order defined by the TABL chunk.
    pub tables: Vec<Table>,
}

impl Gxt {
    /// Parse a GXT file from a byte slice.
    pub fn parse(data: &[u8]) -> Result<Gxt> {
        let mut r = Reader::new(data);
        let total_len = data.len();

        // The Xbox's files have no TABL: one TKEY/TDAT pair, the MAIN table
        if data.starts_with(b"TKEY") {
            return Ok(Gxt { tables: vec![read_table(data, 0, "MAIN".to_string())?] });
        }
        // Read the first chunk to verify magic "TABL"
        let magic = r.bytes(4, "chunk magic")?;
        if magic != b"TABL" {
            return Err(Error::Invalid(format!(
                "expected TABL magic, found {:?}",
                std::str::from_utf8(magic)
            )));
        }
        let _tabl_size = r.u32("TABL size")?;

        // Read TABL payload: N entries of 12 bytes each
        let tabl_data = r.bytes(_tabl_size as usize, "TABL payload")?;
        let tabl_reader = &mut Reader::new(tabl_data);
        let remaining = tabl_reader.remaining();
        if !remaining.is_multiple_of(12) {
            return Err(Error::Invalid(
                "TABL payload size not multiple of 12".into(),
            ));
        }
        let num_entries = remaining / 12;

        let mut tables_info = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            let name_bytes = tabl_reader.bytes(8, "TABL name")?;
            let offset = tabl_reader.u32("TABL offset")?;
            let table_name = c_str(name_bytes);
            tables_info.push((table_name, offset as usize));
        }

        let mut tables = Vec::with_capacity(tables_info.len());

        for (table_name, offset) in tables_info {
            // Seek to the table's offset in the original data
            if offset > total_len {
                return Err(Error::Invalid(format!(
                    "table {} offset {} out of bounds",
                    table_name, offset
                )));
            }

            tables.push(read_table(data, offset, table_name)?);
        }

        Ok(Gxt { tables })
    }

    /// Get a table by name.
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    /// Get the MAIN table.
    pub fn main(&self) -> Option<&Table> {
        self.table("MAIN")
    }
}

/// Converts a slice of u16 code units (game charset) to a lossy Rust String.
/// Units < 0x80 are mapped to ASCII characters. Others are mapped via `char::from_u32` or `U+FFFD`.
pub fn to_lossy_string(units: &[u16]) -> String {
    let mut s = String::with_capacity(units.len());
    for &unit in units {
        if unit < 0x80 {
            s.push(unit as u8 as char);
        } else {
            s.push(char::from_u32(unit as u32).unwrap_or('\u{FFFD}'));
        }
    }
    s
}

/// One table at `offset`: its name header (unless MAIN), then TKEY and TDAT.
fn read_table(data: &[u8], offset: usize, table_name: String) -> Result<Table> {
    let mut table_reader = Reader::new(&data[offset..]);

    // If not MAIN, the first 8 bytes must be the NUL-padded table name
    if table_name != "MAIN" {
        let stored_name_bytes = table_reader.bytes(8, "table header name")?;
        let stored_name = c_str(stored_name_bytes);
        if stored_name != table_name {
            return Err(Error::Invalid(format!(
                "table header name mismatch: expected {}, found {}",
                table_name, stored_name
            )));
        }
    }

    // Next chunk must be "TKEY"
    let key_magic = table_reader.bytes(4, "TKEY magic")?;
    if key_magic != b"TKEY" {
        return Err(Error::Invalid(format!(
            "expected TKEY magic in table {}, found {:?}",
            table_name,
            std::str::from_utf8(key_magic)
        )));
    }
    let key_size = table_reader.u32("TKEY size")?;
    let key_data = table_reader.bytes(key_size as usize, "TKEY payload")?;

    // Validate TKEY payload size is multiple of 12
    if key_data.len() % 12 != 0 {
        return Err(Error::Invalid(format!(
            "TKEY payload size {} is not a multiple of 12",
            key_data.len()
        )));
    }

    let mut keys = Vec::with_capacity(key_data.len() / 12);
    let mut key_reader = Reader::new(key_data);
    for _ in 0..(key_data.len() / 12) {
        let offset_in_dat = key_reader.u32("TKEY offset")?;
        let key_name_bytes = key_reader.bytes(8, "TKEY key name")?;
        let key_name = c_str(key_name_bytes);
        keys.push((offset_in_dat, key_name));
    }

    // Next chunk must be "TDAT"
    let dat_magic = table_reader.bytes(4, "TDAT magic")?;
    if dat_magic != b"TDAT" {
        return Err(Error::Invalid(format!(
            "expected TDAT magic in table {}, found {:?}",
            table_name,
            std::str::from_utf8(dat_magic)
        )));
    }
    let _dat_size = table_reader.u32("TDAT size")?;
    let dat_payload = table_reader.bytes(_dat_size as usize, "TDAT payload")?;

    // Parse entries
    let mut entries = Vec::with_capacity(keys.len());
    for (offset_in_dat, key_name) in keys {
        // Validate offset
        if offset_in_dat as usize > dat_payload.len() {
            return Err(Error::Invalid(format!(
                "TKEY offset {} for key '{}' in table {} is out of TDAT bounds",
                offset_in_dat, key_name, table_name
            )));
        }
        if offset_in_dat % 2 != 0 {
            return Err(Error::Invalid(format!(
                "TKEY offset {} for key '{}' in table {} is not even",
                offset_in_dat, key_name, table_name
            )));
        }

        let start_idx = offset_in_dat as usize;
        let mut units = Vec::new();
        let mut idx = start_idx;
        while idx + 1 < dat_payload.len() {
            let unit = u16::from_le_bytes([dat_payload[idx], dat_payload[idx + 1]]);
            if unit == 0 {
                break;
            }
            units.push(unit);
            idx += 2;
        }
        entries.push((key_name, units));
    }

    Ok(Table {
        name: table_name,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_file_is_one_main_table() {
        // (the Xbox's layout: TKEY with offset + key entries, then TDAT, no TABL)
        let mut keys = Vec::new();
        keys.extend_from_slice(&0u32.to_le_bytes());
        keys.extend_from_slice(b"FEH_A\0\0\0");
        let dat: Vec<u8> = "A:select".encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()).collect();
        let mut data = Vec::new();
        chunk(&mut data, b"TKEY", &keys);
        chunk(&mut data, b"TDAT", &dat);
        let g = Gxt::parse(&data).unwrap();
        assert_eq!(g.tables.len(), 1);
        assert_eq!(g.tables[0].name, "MAIN");
        assert_eq!(g.tables[0].get("FEH_A").unwrap(), "A:select".encode_utf16().collect::<Vec<_>>().as_slice());
    }

    fn chunk(out: &mut Vec<u8>, magic: &[u8; 4], payload: &[u8]) {
        out.extend_from_slice(magic);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
    }

    fn name8(s: &str) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[..s.len()].copy_from_slice(s.as_bytes());
        b
    }

    /// TKEY + TDAT chunks for (key, text) pairs.
    fn table_body(entries: &[(&str, &str)], bad_offset: bool) -> Vec<u8> {
        let (mut keys, mut dat) = (Vec::new(), Vec::new());
        for (k, v) in entries {
            let off = if bad_offset { 9999 } else { dat.len() as u32 };
            keys.extend_from_slice(&off.to_le_bytes());
            keys.extend_from_slice(&name8(k));
            for c in v.encode_utf16().chain([0]) {
                dat.extend_from_slice(&c.to_le_bytes());
            }
        }
        let mut out = Vec::new();
        chunk(&mut out, b"TKEY", &keys);
        chunk(&mut out, b"TDAT", &dat);
        out
    }

    fn build(bad_offset: bool) -> Vec<u8> {
        let main = table_body(&[("A", "Hello"), ("B", "W\u{e9}lt")], bad_offset);
        let mut m1 = name8("M1").to_vec();
        m1.extend(table_body(&[("X", "Mission")], false));
        let tabl_len = 2 * 12;
        let main_at = 8 + tabl_len;
        let m1_at = main_at + main.len();
        let mut tabl = Vec::new();
        tabl.extend_from_slice(&name8("MAIN"));
        tabl.extend_from_slice(&(main_at as u32).to_le_bytes());
        tabl.extend_from_slice(&name8("M1"));
        tabl.extend_from_slice(&(m1_at as u32).to_le_bytes());
        let mut out = Vec::new();
        chunk(&mut out, b"TABL", &tabl);
        out.extend(main);
        out.extend(m1);
        out
    }

    #[test]
    fn parses_tables() {
        let g = Gxt::parse(&build(false)).unwrap();
        assert_eq!(g.tables.len(), 2);
        let main = g.main().unwrap();
        assert_eq!(to_lossy_string(main.get("A").unwrap()), "Hello");
        assert_eq!(to_lossy_string(main.get("B").unwrap()), "W\u{e9}lt");
        assert_eq!(main.get("C"), None);
        assert_eq!(
            to_lossy_string(g.table("M1").unwrap().get("X").unwrap()),
            "Mission"
        );
    }

    #[test]
    fn rejects_bad_offset_and_magic() {
        assert!(matches!(Gxt::parse(&build(true)), Err(Error::Invalid(_))));
        let mut d = build(false);
        d[0] = b'X';
        assert!(matches!(Gxt::parse(&d), Err(Error::Invalid(_))));
    }
}
