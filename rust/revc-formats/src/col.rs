//! Parsers for Vice City `.col` collision files (version 1).

use crate::bytes::Reader;
use crate::{Error, Result};

/// A sphere collision primitive.
#[derive(Debug, Clone, PartialEq)]
pub struct Sphere {
    pub radius: f32,
    pub center: [f32; 3],
    pub surface: u8,
    pub piece: u8,
}

/// A line collision primitive.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub start: [f32; 3],
    pub end: [f32; 3],
}

/// An axis-aligned box collision primitive.
#[derive(Debug, Clone, PartialEq)]
pub struct Box3 {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub surface: u8,
    pub piece: u8,
}

/// A triangle mesh face.
#[derive(Debug, Clone, PartialEq)]
pub struct Triangle {
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub surface: u8,
}

/// A complete collision model extracted from a `.col` record.
#[derive(Debug, Clone, PartialEq)]
pub struct ColModel {
    pub name: String,
    pub bound_radius: f32,
    pub bound_center: [f32; 3],
    pub bound_min: [f32; 3],
    pub bound_max: [f32; 3],
    pub spheres: Vec<Sphere>,
    pub lines: Vec<Line>,
    pub boxes: Vec<Box3>,
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<Triangle>,
}

/// Parse a `.col` file (or an IMG entry chunk) into a list of collision models.
///
/// Stops cleanly when fewer than 8 bytes remain or the next 4 bytes are all zero.
pub fn parse(data: &[u8]) -> Result<Vec<ColModel>> {
    let mut models = Vec::new();
    let mut r = Reader::new(data);

    loop {
        if r.remaining() < 8 {
            break;
        }

        let magic_pos = r.pos();
        let magic_bytes = r.bytes(4, "magic")?;
        let size_pos = r.pos();
        let size = r.u32("record size")?;

        // Check for end-of-data marker (all zeros in magic)
        if magic_bytes == [0, 0, 0, 0] {
            break;
        }

        // Check for valid magic "COLL"
        if magic_bytes != b"COLL" {
            return Err(Error::Invalid(format!(
                "invalid magic at offset {magic_pos}: expected COLL, found {:?}",
                magic_bytes
            )));
        }

        // Check if the record fits in the remaining data
        let body_start = r.pos();
        let body_end = body_start
            .checked_add(size as usize)
            .ok_or_else(|| Error::Invalid(format!("record size overflow at offset {size_pos}")))?;

        if body_end > data.len() {
            return Err(Error::Truncated("record body", body_start));
        }

        let body_data = &data[body_start..body_end];
        let mut body_reader = Reader::new(body_data);
        r.skip(size as usize)?;

        let model = parse_model(&mut body_reader)?;
        models.push(model);
    }

    Ok(models)
}

fn parse_model(r: &mut Reader) -> Result<ColModel> {
    let name = r.fixed_str(24, "model name")?;
    let name_of_model = name.clone();

    let bound_radius = r.f32("bound radius")?;
    let bound_center = r.vec3("bound center")?;
    let bound_min = r.vec3("bound min")?;
    let bound_max = r.vec3("bound max")?;

    // Helper to read a count where only low 16 bits matter, treating negative as 0
    let read_count = |r: &mut Reader| -> Result<usize> {
        // 4-byte field, the game only uses the low i16.
        let raw = r.i16("count field")?;
        r.skip(2)?;
        if raw < 0 { Ok(0) } else { Ok(raw as usize) }
    };

    // 1. Spheres: 20 bytes each (f32, vec3, u8, u8, 2 pad)
    let sphere_count = read_count(r)?;
    let mut spheres = Vec::with_capacity(sphere_count);
    for _ in 0..sphere_count {
        let radius = r.f32("sphere radius")?;
        let center = r.vec3("sphere center")?;
        let surface = r.u8("sphere surface")?;
        let piece = r.u8("sphere piece")?;
        r.skip(2)?;
        spheres.push(Sphere {
            radius,
            center,
            surface,
            piece,
        });
    }

    // 2. Lines: 24 bytes each (vec3, vec3)
    let line_count = read_count(r)?;
    let mut lines = Vec::with_capacity(line_count);
    for _ in 0..line_count {
        let start = r.vec3("line start")?;
        let end = r.vec3("line end")?;
        lines.push(Line { start, end });
    }

    // 3. Boxes: 28 bytes each (vec3, vec3, u8, u8, 2 pad)
    let box_count = read_count(r)?;
    let mut boxes = Vec::with_capacity(box_count);
    for _ in 0..box_count {
        let min = r.vec3("box min")?;
        let max = r.vec3("box max")?;
        let surface = r.u8("box surface")?;
        let piece = r.u8("box piece")?;
        r.skip(2)?;
        boxes.push(Box3 {
            min,
            max,
            surface,
            piece,
        });
    }

    // 4. Vertices: 12 bytes each (vec3)
    let vertex_count = read_count(r)?;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        let v = r.vec3("vertex")?;
        vertices.push(v);
    }

    // 5. Triangles: 16 bytes each (i32, i32, i32, u8, 3 pad)
    let tri_count = read_count(r)?;
    let mut triangles = Vec::with_capacity(tri_count);
    for _ in 0..tri_count {
        let a = r.i32("triangle a")?;
        let b = r.i32("triangle b")?;
        let c = r.i32("triangle c")?;
        let surface = r.u8("triangle surface")?;
        r.skip(3)?;
        // Validate indices
        let validate_idx = |idx: i32, name: &str| -> Result<u32> {
            if idx < 0 {
                Err(Error::Invalid(format!(
                    "{name_of_model}: triangle index {name} is negative: {idx}"
                )))
            } else {
                let idx_u = idx as u32;
                if idx_u >= vertices.len() as u32 {
                    Err(Error::Invalid(format!(
                        "{name_of_model}: triangle index {name} ({idx_u}) out of range: vertices.len()={}",
                        vertices.len()
                    )))
                } else {
                    Ok(idx_u)
                }
            }
        };
        let a_u = validate_idx(a, "a")?;
        let b_u = validate_idx(b, "b")?;
        let c_u = validate_idx(c, "c")?;
        triangles.push(Triangle {
            a: a_u,
            b: b_u,
            c: c_u,
            surface,
        });
    }

    // Ignore any remaining bytes in the body
    Ok(ColModel {
        name,
        bound_radius,
        bound_center,
        bound_min,
        bound_max,
        spheres,
        lines,
        boxes,
        vertices,
        triangles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v3(buf: &mut Vec<u8>, v: &[f32; 3]) {
        for x in v {
            buf.extend_from_slice(&x.to_le_bytes());
        }
    }

    /// Serialise a ColModel back into a COLL record bytes.
    fn serialize_model(model: &ColModel) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header: Magic "COLL"
        buf.extend_from_slice(b"COLL");

        // We don't know the size yet, so write a placeholder
        let size_offset = buf.len();
        buf.extend_from_slice(&[0u8; 4]); // Placeholder for size

        // Body
        // Name
        let mut name_bytes = [0u8; 24];
        let name_slice = model.name.as_bytes();
        let len = name_slice.len().min(24);
        name_bytes[..len].copy_from_slice(&name_slice[..len]);
        buf.extend_from_slice(&name_bytes);

        // Bounds
        buf.extend_from_slice(&model.bound_radius.to_le_bytes());
        v3(&mut buf, &model.bound_center);
        v3(&mut buf, &model.bound_min);
        v3(&mut buf, &model.bound_max);

        // Spheres
        buf.extend_from_slice(&(model.spheres.len() as i32).to_le_bytes());
        for s in &model.spheres {
            buf.extend_from_slice(&s.radius.to_le_bytes());
            v3(&mut buf, &s.center);
            buf.push(s.surface);
            buf.push(s.piece);
            buf.extend_from_slice(&[0u8; 2]);
        }

        // Lines
        buf.extend_from_slice(&(model.lines.len() as i32).to_le_bytes());
        for l in &model.lines {
            v3(&mut buf, &l.start);
            v3(&mut buf, &l.end);
        }

        // Boxes
        buf.extend_from_slice(&(model.boxes.len() as i32).to_le_bytes());
        for b in &model.boxes {
            v3(&mut buf, &b.min);
            v3(&mut buf, &b.max);
            buf.push(b.surface);
            buf.push(b.piece);
            buf.extend_from_slice(&[0u8; 2]);
        }

        // Vertices
        buf.extend_from_slice(&(model.vertices.len() as i32).to_le_bytes());
        for v in &model.vertices {
            v3(&mut buf, v);
        }

        // Triangles
        buf.extend_from_slice(&(model.triangles.len() as i32).to_le_bytes());
        for t in &model.triangles {
            buf.extend_from_slice(&(t.a as i32).to_le_bytes());
            buf.extend_from_slice(&(t.b as i32).to_le_bytes());
            buf.extend_from_slice(&(t.c as i32).to_le_bytes());
            buf.push(t.surface);
            buf.extend_from_slice(&[0u8; 3]);
        }

        // Update size
        let body_size = buf.len() - 8; // Exclude magic (4) and size field (4)
        let size_bytes = (body_size as u32).to_le_bytes();
        buf[size_offset..size_offset + 4].copy_from_slice(&size_bytes);

        buf
    }

    /// Helper to create a minimal valid ColModel with one of each element type.
    fn make_sample_model() -> ColModel {
        ColModel {
            name: "TestModel".to_string(),
            bound_radius: 10.0,
            bound_center: [0.0, 0.0, 0.0],
            bound_min: [-10.0, -10.0, -10.0],
            bound_max: [10.0, 10.0, 10.0],
            spheres: vec![Sphere {
                radius: 1.0,
                center: [0.0, 0.0, 0.0],
                surface: 1,
                piece: 2,
            }],
            lines: vec![Line {
                start: [0.0, 0.0, 0.0],
                end: [1.0, 1.0, 1.0],
            }],
            boxes: vec![Box3 {
                min: [0.0, 0.0, 0.0],
                max: [1.0, 1.0, 1.0],
                surface: 3,
                piece: 4,
            }],
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            triangles: vec![Triangle {
                a: 0,
                b: 1,
                c: 2,
                surface: 5,
            }],
        }
    }

    #[test]
    fn round_trip() {
        let model = make_sample_model();
        let bytes = serialize_model(&model);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], model);
    }

    #[test]
    fn two_records() {
        let a = make_sample_model();
        let mut b = make_sample_model();
        b.name = "Second".into();
        b.spheres.clear();
        let bytes = [serialize_model(&a), serialize_model(&b)].concat();
        assert_eq!(parse(&bytes).unwrap(), vec![a, b]);
    }

    #[test]
    fn trailing_zero_padding_accepted() {
        let model = make_sample_model();
        let mut bytes = serialize_model(&model);
        // Append 16 bytes of zeros
        bytes.extend_from_slice(&[0u8; 16]);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], model);
    }

    #[test]
    fn triangle_index_out_of_range_rejected() {
        let mut model = make_sample_model();
        // Make a triangle pointing to a non-existent vertex
        model.triangles = vec![Triangle {
            a: 0,
            b: 1,
            c: 100, // Out of range
            surface: 1,
        }];
        let bytes = serialize_model(&model);
        let result = parse(&bytes);
        assert!(result.is_err());
        match result {
            Err(Error::Invalid(msg)) => {
                assert!(msg.contains("c"));
            }
            other => panic!("Expected Error::Invalid, got {:?}", other),
        }
    }
}
