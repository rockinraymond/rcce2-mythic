//! Blitz3D `.b3d` mesh parser — the format the GUE editor writes and the engine
//! loads via native `LoadMesh`. Standard B3D: a `BB3D` magic + version, then
//! nested length-tagged chunks (`TEXS`/`BRUS`/`NODE`/`MESH`/`VRTS`/`TRIS`/
//! `BONE`/`ANIM`/`KEYS`). All values little-endian; node names are NUL-terminated.
//!
//! This extracts geometry (positions + optional normals/UVs + triangle indices)
//! per mesh, with each mesh's node translation applied so a model assembles in
//! place — enough to render. Skeleton/animation (`BONE`/`ANIM`) are skipped for
//! now (added when skeletal animation lands).

use crate::reader::{BlitzReader, ReadError};

/// One mesh's geometry, ready for upload to the GPU.
#[derive(Debug, Default, Clone)]
pub struct B3dMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Triangle vertex indices (3 per triangle).
    pub indices: Vec<u32>,
    /// Brush index this mesh defaults to (`-1` = none). Resolved into `texture`.
    pub brush_id: i32,
    /// Texture filename for this mesh (from its brush's first texture slot), as
    /// stored in the B3D (often a stale absolute author path — resolve by
    /// basename against the project's texture dirs).
    pub texture: Option<String>,
}

/// A parsed `.b3d` model: all meshes, flattened (node translation applied).
#[derive(Debug, Default, Clone)]
pub struct B3dModel {
    pub meshes: Vec<B3dMesh>,
    /// `TEXS` texture filenames, by index.
    pub textures: Vec<String>,
    /// `BRUS` brushes — each brush's texture-slot indices into `textures`
    /// (`-1` = empty slot).
    pub brushes: Vec<Vec<i32>>,
    /// Every `NODE`'s name and its accumulated translation in model space (same
    /// space as the flattened vertices). Used to find attach points like the
    /// `"Head"` joint for hair/beard/hat. Translation-only (node scale/rotation
    /// are not yet composed — fine for static bind-pose attachment).
    pub joints: Vec<(String, [f32; 3])>,
}

impl B3dModel {
    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(|m| m.positions.len()).sum()
    }

    /// Accumulated model-space position of the first node whose name matches
    /// `name` (case-insensitive), e.g. `"Head"`. `None` if absent.
    pub fn joint_pos(&self, name: &str) -> Option<[f32; 3]> {
        self.joints
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, p)| *p)
    }
    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(|m| m.indices.len() / 3).sum()
    }

    /// Axis-aligned bounds `(min, max)` over all vertices. Returns zeros for an
    /// empty model.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        let mut any = false;
        for mesh in &self.meshes {
            for p in &mesh.positions {
                any = true;
                for k in 0..3 {
                    min[k] = min[k].min(p[k]);
                    max[k] = max[k].max(p[k]);
                }
            }
        }
        if any {
            (min, max)
        } else {
            ([0.0; 3], [0.0; 3])
        }
    }

    /// Parse a whole `.b3d` file image.
    pub fn parse(data: &[u8]) -> Result<B3dModel, ReadError> {
        let mut r = BlitzReader::new(data);
        let magic = r.read_tag()?;
        if &magic != b"BB3D" {
            return Err(ReadError::StringTooLong { len: 0, max: 0 }); // reuse as "bad magic"
        }
        let size = r.read_int()? as usize;
        let end = r.position() + size;
        let _version = r.read_int()?;

        let mut model = B3dModel::default();
        parse_chunks(&mut r, end.min(data.len()), [0.0; 3], &mut model)?;
        model.resolve_textures();
        Ok(model)
    }

    /// Fill each mesh's `texture` from its brush's first texture slot.
    fn resolve_textures(&mut self) {
        for mesh in &mut self.meshes {
            if mesh.brush_id < 0 {
                continue;
            }
            let Some(brush) = self.brushes.get(mesh.brush_id as usize) else {
                continue;
            };
            // First non-empty texture slot.
            if let Some(&tex_id) = brush.iter().find(|&&t| t >= 0) {
                if let Some(name) = self.textures.get(tex_id as usize) {
                    mesh.texture = Some(name.clone());
                }
            }
        }
    }
}

/// Read a `[tag:4][size:i32]` chunk header.
fn chunk_header(r: &mut BlitzReader) -> Result<([u8; 4], usize), ReadError> {
    let tag = r.read_tag()?;
    let size = r.read_int()?.max(0) as usize;
    Ok((tag, size))
}

/// Walk sibling chunks until `end`. `offset` is the accumulated node translation.
fn parse_chunks(
    r: &mut BlitzReader,
    end: usize,
    offset: [f32; 3],
    model: &mut B3dModel,
) -> Result<(), ReadError> {
    while r.position() + 8 <= end {
        let (tag, size) = chunk_header(r)?;
        let chunk_end = (r.position() + size).min(end);
        match &tag {
            b"TEXS" => parse_texs(r, chunk_end, model)?,
            b"BRUS" => parse_brus(r, chunk_end, model)?,
            b"NODE" => parse_node(r, chunk_end, offset, model)?,
            _ => {}
        }
        r.seek(chunk_end)?;
    }
    Ok(())
}

/// `TEXS`: a sequence of textures, each `file(cstr) · flags(i32) · blend(i32) ·
/// xpos,ypos,xscale,yscale,rotation(f32×5)`.
fn parse_texs(r: &mut BlitzReader, end: usize, model: &mut B3dModel) -> Result<(), ReadError> {
    while r.position() < end {
        let file = r.read_cstr(1024)?;
        let _flags = r.read_int()?;
        let _blend = r.read_int()?;
        for _ in 0..5 {
            r.read_float()?;
        }
        model.textures.push(file);
    }
    Ok(())
}

/// `BRUS`: `n_texs(i32)` then brushes, each `name(cstr) · rgba(f32×4) ·
/// shininess(f32) · blend(i32) · fx(i32) · texture_id(i32 × n_texs)`.
fn parse_brus(r: &mut BlitzReader, end: usize, model: &mut B3dModel) -> Result<(), ReadError> {
    let n_texs = r.read_int()?.clamp(0, 8) as usize;
    while r.position() < end {
        let _name = r.read_cstr(256)?;
        for _ in 0..4 {
            r.read_float()?; // rgba
        }
        let _shininess = r.read_float()?;
        let _blend = r.read_int()?;
        let _fx = r.read_int()?;
        let mut tex_ids = Vec::with_capacity(n_texs);
        for _ in 0..n_texs {
            tex_ids.push(r.read_int()?);
        }
        model.brushes.push(tex_ids);
    }
    Ok(())
}

/// `NODE`: name(cstr) · position(3f) · scale(3f) · rotation(4f) · sub-chunks.
fn parse_node(
    r: &mut BlitzReader,
    end: usize,
    parent_offset: [f32; 3],
    model: &mut B3dModel,
) -> Result<(), ReadError> {
    let name = r.read_cstr(256)?;
    let px = r.read_float()?;
    let py = r.read_float()?;
    let pz = r.read_float()?;
    let _sx = r.read_float()?;
    let _sy = r.read_float()?;
    let _sz = r.read_float()?;
    let _rw = r.read_float()?;
    let _rx = r.read_float()?;
    let _ry = r.read_float()?;
    let _rz = r.read_float()?;
    let offset = [
        parent_offset[0] + px,
        parent_offset[1] + py,
        parent_offset[2] + pz,
    ];
    if !name.is_empty() {
        model.joints.push((name, offset));
    }

    while r.position() + 8 <= end {
        let (tag, size) = chunk_header(r)?;
        let chunk_end = (r.position() + size).min(end);
        match &tag {
            b"MESH" => {
                for mesh in parse_mesh(r, chunk_end, offset)? {
                    if !mesh.positions.is_empty() && !mesh.indices.is_empty() {
                        model.meshes.push(mesh);
                    }
                }
            }
            b"NODE" => parse_node(r, chunk_end, offset, model)?,
            _ => {} // BONE / KEYS / ANIM — skipped for now
        }
        r.seek(chunk_end)?;
    }
    Ok(())
}

/// `MESH`: meshBrush(i32) · `VRTS`(shared vertices) · `TRIS`(one or more, each a
/// brush_id + index list). Emits one [`B3dMesh`] per TRIS group (so each can
/// carry its own texture), sharing the mesh's vertex data.
fn parse_mesh(r: &mut BlitzReader, end: usize, offset: [f32; 3]) -> Result<Vec<B3dMesh>, ReadError> {
    let mesh_brush = r.read_int()?;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut groups: Vec<(i32, Vec<u32>)> = Vec::new();

    while r.position() + 8 <= end {
        let (tag, size) = chunk_header(r)?;
        let chunk_end = (r.position() + size).min(end);
        match &tag {
            b"VRTS" => parse_vrts(r, chunk_end, offset, &mut positions, &mut normals, &mut uvs)?,
            b"TRIS" => {
                let brush = r.read_int()?;
                let mut indices = Vec::new();
                while r.position() + 12 <= chunk_end {
                    let a = r.read_int()? as u32;
                    let b = r.read_int()? as u32;
                    let c = r.read_int()? as u32;
                    indices.extend_from_slice(&[a, b, c]);
                }
                groups.push((brush, indices));
            }
            _ => {}
        }
        r.seek(chunk_end)?;
    }

    let mut out = Vec::with_capacity(groups.len());
    for (brush, indices) in groups {
        if indices.is_empty() {
            continue;
        }
        let brush_id = if brush >= 0 { brush } else { mesh_brush };
        out.push(B3dMesh {
            positions: positions.clone(),
            normals: normals.clone(),
            uvs: uvs.clone(),
            indices,
            brush_id,
            texture: None,
        });
    }
    Ok(out)
}

/// `VRTS`: flags(i32) · tex_coord_sets(i32) · tex_coord_set_size(i32) · vertices.
/// flags&1 = normals present, flags&2 = vertex colors present.
fn parse_vrts(
    r: &mut BlitzReader,
    end: usize,
    offset: [f32; 3],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
) -> Result<(), ReadError> {
    let flags = r.read_int()?;
    let tex_coord_sets = r.read_int()?.clamp(0, 8) as usize;
    let tex_coord_set_size = r.read_int()?.clamp(0, 4) as usize;
    let has_normal = flags & 1 != 0;
    let has_color = flags & 2 != 0;

    while r.position() < end {
        let x = r.read_float()?;
        let y = r.read_float()?;
        let z = r.read_float()?;
        positions.push([x + offset[0], y + offset[1], z + offset[2]]);

        if has_normal {
            let nx = r.read_float()?;
            let ny = r.read_float()?;
            let nz = r.read_float()?;
            normals.push([nx, ny, nz]);
        }
        if has_color {
            let _ = (r.read_float()?, r.read_float()?, r.read_float()?, r.read_float()?);
        }
        let mut uv = [0.0f32; 2];
        for set in 0..tex_coord_sets {
            for comp in 0..tex_coord_set_size {
                let v = r.read_float()?;
                if set == 0 && comp < 2 {
                    uv[comp] = v;
                }
            }
        }
        if tex_coord_sets > 0 {
            uvs.push(uv);
        }
    }
    Ok(())
}
