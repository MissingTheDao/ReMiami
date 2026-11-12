//! Vertex input described by data: which attributes a vertex has, in which format, where.
//!
//! New code for retro-wgpu. The shader for a layout is generated from it (see `shader`), so
//! any game's vertex structs can be drawn as they are in memory (D3D FVF, GX vertex
//! descriptors, PS2 unpacked GIF data, Pure3D prim buffers...).

/// What an attribute means to the shaders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Semantic {
    /// Object-space position (x, y, z), transformed by world, view and projection.
    Position,
    /// Screen-space position (x, y in pixels, z depth 0..1, rhw = 1/w): D3D's XYZRHW. Not
    /// transformed; lighting is off for it.
    PositionT,
    Normal,
    /// Diffuse colour.
    Color,
    /// Texture coordinate set 0..3.
    TexCoord(u8),
    /// Up to four skinning weights.
    BlendWeights,
    /// Up to four bone indices (into the draw's bone palette).
    BlendIndices,
}

/// Memory format of an attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AttrFormat {
    F32x2,
    F32x3,
    F32x4,
    /// Four bytes as 0..1 in memory order (colours as R, G, B, A bytes).
    Unorm8x4,
    /// Four bytes B, G, R, A in memory (D3DCOLOR): swizzled in the shader.
    Unorm8x4Bgra,
    /// Four bytes as integers (bone indices).
    U8x4,
    /// Four u16 as integers.
    U16x4,
    /// Four i16 / 32767 (quantised positions/normals/uvs as in GX or PS2 data).
    Snorm16x4,
    /// Two i16 / 32767.
    Snorm16x2,
}

impl AttrFormat {
    pub(crate) fn wgpu(self) -> wgpu::VertexFormat {
        use wgpu::VertexFormat as V;
        match self {
            AttrFormat::F32x2 => V::Float32x2,
            AttrFormat::F32x3 => V::Float32x3,
            AttrFormat::F32x4 => V::Float32x4,
            AttrFormat::Unorm8x4 | AttrFormat::Unorm8x4Bgra => V::Unorm8x4,
            AttrFormat::U8x4 => V::Uint8x4,
            AttrFormat::U16x4 => V::Uint16x4,
            AttrFormat::Snorm16x4 => V::Snorm16x4,
            AttrFormat::Snorm16x2 => V::Snorm16x2,
        }
    }

    pub(crate) fn components(self) -> u32 {
        match self {
            AttrFormat::F32x2 | AttrFormat::Snorm16x2 => 2,
            AttrFormat::F32x3 => 3,
            _ => 4,
        }
    }

    pub(crate) fn is_int(self) -> bool {
        matches!(self, AttrFormat::U8x4 | AttrFormat::U16x4)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Attribute {
    pub semantic: Semantic,
    pub format: AttrFormat,
    /// Byte offset in the vertex.
    pub offset: u32,
}

/// A vertex layout: attributes and the vertex stride.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VertexLayout {
    pub stride: u32,
    pub attributes: Vec<Attribute>,
}

impl VertexLayout {
    /// Attributes packed in the given order (offsets computed from the formats).
    pub fn packed(attrs: &[(Semantic, AttrFormat)]) -> VertexLayout {
        let mut offset = 0;
        let attributes = attrs
            .iter()
            .map(|&(semantic, format)| {
                let a = Attribute { semantic, format, offset };
                offset += match format {
                    AttrFormat::F32x2 => 8,
                    AttrFormat::F32x3 => 12,
                    AttrFormat::F32x4 => 16,
                    AttrFormat::U16x4 | AttrFormat::Snorm16x4 => 8,
                    AttrFormat::Unorm8x4 | AttrFormat::Unorm8x4Bgra | AttrFormat::U8x4 | AttrFormat::Snorm16x2 => 4,
                };
                a
            })
            .collect();
        VertexLayout { stride: offset, attributes }
    }

    pub fn get(&self, s: Semantic) -> Option<&Attribute> {
        self.attributes.iter().find(|a| a.semantic == s)
    }

    pub fn has(&self, s: Semantic) -> bool {
        self.get(s).is_some()
    }

    pub fn skinned(&self) -> bool {
        self.has(Semantic::BlendWeights) && self.has(Semantic::BlendIndices)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.has(Semantic::Position) == self.has(Semantic::PositionT) {
            return Err("a layout needs exactly one of Position and PositionT".into());
        }
        for a in &self.attributes {
            if let Semantic::TexCoord(n) = a.semantic
                && n > 3
            {
                return Err(format!("texture coordinate set {n} (0..3 supported)"));
            }
            let int_ok = a.semantic == Semantic::BlendIndices;
            if a.format.is_int() != int_ok {
                return Err(format!("{:?} can't use {:?}", a.semantic, a.format));
            }
            if a.semantic == Semantic::PositionT && a.format.components() != 4 {
                return Err("PositionT needs four components (x, y, z, rhw)".into());
            }
            if a.offset % 4 != 0 {
                return Err(format!("{:?}: offset {} not a multiple of 4", a.semantic, a.offset));
            }
        }
        let mut seen = self.attributes.iter().map(|a| a.semantic).collect::<Vec<_>>();
        seen.sort();
        if seen.windows(2).any(|w| w[0] == w[1]) {
            return Err("duplicate semantic".into());
        }
        if self.stride % 4 != 0 || self.stride == 0 {
            return Err(format!("stride {} must be a non-zero multiple of 4", self.stride));
        }
        Ok(())
    }

    /// Shader input location of a semantic (fixed, so shaders can be generated per layout).
    pub(crate) fn location(s: Semantic) -> u32 {
        match s {
            Semantic::Position | Semantic::PositionT => 0,
            Semantic::Normal => 1,
            Semantic::Color => 2,
            Semantic::TexCoord(n) => 3 + n as u32,
            Semantic::BlendWeights => 7,
            Semantic::BlendIndices => 8,
        }
    }
}
