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
}

/// A parsed `.b3d` model: all meshes, flattened (node translation applied).
#[derive(Debug, Default, Clone)]
pub struct B3dModel {
    pub meshes: Vec<B3dMesh>,
}

impl B3dModel {
    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(|m| m.positions.len()).sum()
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
        Ok(model)
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
            b"NODE" => parse_node(r, chunk_end, offset, model)?,
            _ => {}
        }
        r.seek(chunk_end)?;
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
    let _name = r.read_cstr(256)?;
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

    while r.position() + 8 <= end {
        let (tag, size) = chunk_header(r)?;
        let chunk_end = (r.position() + size).min(end);
        match &tag {
            b"MESH" => {
                let mesh = parse_mesh(r, chunk_end, offset)?;
                if !mesh.positions.is_empty() {
                    model.meshes.push(mesh);
                }
            }
            b"NODE" => parse_node(r, chunk_end, offset, model)?,
            _ => {} // BONE / KEYS / ANIM — skipped for now
        }
        r.seek(chunk_end)?;
    }
    Ok(())
}

/// `MESH`: brush_id(i32) · `VRTS` · `TRIS`(one or more).
fn parse_mesh(r: &mut BlitzReader, end: usize, offset: [f32; 3]) -> Result<B3dMesh, ReadError> {
    let _brush_id = r.read_int()?;
    let mut mesh = B3dMesh::default();
    while r.position() + 8 <= end {
        let (tag, size) = chunk_header(r)?;
        let chunk_end = (r.position() + size).min(end);
        match &tag {
            b"VRTS" => parse_vrts(r, chunk_end, offset, &mut mesh)?,
            b"TRIS" => parse_tris(r, chunk_end, &mut mesh)?,
            _ => {}
        }
        r.seek(chunk_end)?;
    }
    Ok(mesh)
}

/// `VRTS`: flags(i32) · tex_coord_sets(i32) · tex_coord_set_size(i32) · vertices.
/// flags&1 = normals present, flags&2 = vertex colors present.
fn parse_vrts(
    r: &mut BlitzReader,
    end: usize,
    offset: [f32; 3],
    mesh: &mut B3dMesh,
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
        mesh.positions
            .push([x + offset[0], y + offset[1], z + offset[2]]);

        if has_normal {
            let nx = r.read_float()?;
            let ny = r.read_float()?;
            let nz = r.read_float()?;
            mesh.normals.push([nx, ny, nz]);
        }
        if has_color {
            let _ = (r.read_float()?, r.read_float()?, r.read_float()?, r.read_float()?);
        }
        // Texture coordinate sets; keep the first set's first two as UV.
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
            mesh.uvs.push(uv);
        }
    }
    Ok(())
}

/// `TRIS`: brush_id(i32) · triangles (3 × i32 vertex indices each).
fn parse_tris(r: &mut BlitzReader, end: usize, mesh: &mut B3dMesh) -> Result<(), ReadError> {
    let _brush_id = r.read_int()?;
    while r.position() + 12 <= end {
        let a = r.read_int()? as u32;
        let b = r.read_int()? as u32;
        let c = r.read_int()? as u32;
        mesh.indices.extend_from_slice(&[a, b, c]);
    }
    Ok(())
}
