//! Blitz3D `.b3d` mesh parser — the format the GUE editor writes and the engine
//! loads via native `LoadMesh`. Standard B3D: a `BB3D` magic + version, then
//! nested length-tagged chunks (`TEXS`/`BRUS`/`NODE`/`MESH`/`VRTS`/`TRIS`/
//! `BONE`/`ANIM`/`KEYS`). All values little-endian; node names are NUL-terminated.
//!
//! This extracts geometry (positions + optional normals/UVs + triangle indices)
//! per mesh, with each mesh's full node bind-pose transform (translate · rotate
//! · scale, composed down the hierarchy) applied so a model assembles in place —
//! enough to render. Skeleton/animation (`BONE`/`ANIM`) are skipped for now
//! (added when skeletal animation lands).

use crate::reader::{BlitzReader, ReadError};

/// A 4×4 row-major matrix (`p' = M · [p,1]`). Just enough linear algebra to
/// compose the B3D node hierarchy's bind-pose transforms (translate · rotate ·
/// scale) without pulling in a math crate.
type Mat4 = [f32; 16];

const IDENTITY: Mat4 = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// `a · b` (row-major).
fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [0.0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[r * 4 + k] * b[k * 4 + c];
            }
            m[r * 4 + c] = s;
        }
    }
    m
}

/// Node local transform `T · R · S` from translation, quaternion `(w,x,y,z)`,
/// and per-axis scale.
fn trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> Mat4 {
    // Normalise the quaternion (b3d stores w,x,y,z).
    let (w, x, y, z) = (q[0], q[1], q[2], q[3]);
    let n = (w * w + x * x + y * y + z * z).sqrt();
    let (w, x, y, z) = if n > 1e-12 {
        (w / n, x / n, y / n, z / n)
    } else {
        (1.0, 0.0, 0.0, 0.0)
    };
    // Rotation 3×3.
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y - w * z),
        2.0 * (x * z + w * y),
        2.0 * (x * y + w * z),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z - w * x),
        2.0 * (x * z - w * y),
        2.0 * (y * z + w * x),
        1.0 - 2.0 * (x * x + y * y),
    ];
    // M = T · R · S : scale multiplies each rotation column; translation last col.
    [
        r[0] * s[0], r[1] * s[1], r[2] * s[2], t[0], //
        r[3] * s[0], r[4] * s[1], r[5] * s[2], t[1], //
        r[6] * s[0], r[7] * s[1], r[8] * s[2], t[2], //
        0.0, 0.0, 0.0, 1.0,
    ]
}

/// Transform a position (includes translation).
fn xform_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
        m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
        m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
    ]
}

/// Transform a direction (rotation + scale, no translation) — for normals.
fn xform_dir(m: &Mat4, v: [f32; 3]) -> [f32; 3] {
    [
        m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
        m[4] * v[0] + m[5] * v[1] + m[6] * v[2],
        m[8] * v[0] + m[9] * v[1] + m[10] * v[2],
    ]
}

/// Invert an affine matrix (upper-3×3 invertible + translation). Node matrices
/// are always affine, so this is exact and cheaper than a full 4×4 inverse.
/// Returns identity if the 3×3 is singular.
pub(crate) fn invert_affine(m: &Mat4) -> Mat4 {
    let a = [
        m[0], m[1], m[2], //
        m[4], m[5], m[6], //
        m[8], m[9], m[10],
    ];
    let det = a[0] * (a[4] * a[8] - a[5] * a[7]) - a[1] * (a[3] * a[8] - a[5] * a[6])
        + a[2] * (a[3] * a[7] - a[4] * a[6]);
    if det.abs() < 1e-20 {
        return IDENTITY;
    }
    let inv_det = 1.0 / det;
    // Inverse of the 3×3 (row-major).
    let i = [
        (a[4] * a[8] - a[5] * a[7]) * inv_det,
        (a[2] * a[7] - a[1] * a[8]) * inv_det,
        (a[1] * a[5] - a[2] * a[4]) * inv_det,
        (a[5] * a[6] - a[3] * a[8]) * inv_det,
        (a[0] * a[8] - a[2] * a[6]) * inv_det,
        (a[2] * a[3] - a[0] * a[5]) * inv_det,
        (a[3] * a[7] - a[4] * a[6]) * inv_det,
        (a[1] * a[6] - a[0] * a[7]) * inv_det,
        (a[0] * a[4] - a[1] * a[3]) * inv_det,
    ];
    let t = [m[3], m[7], m[11]];
    // -inv3x3 · t
    let nt = [
        -(i[0] * t[0] + i[1] * t[1] + i[2] * t[2]),
        -(i[3] * t[0] + i[4] * t[1] + i[5] * t[2]),
        -(i[6] * t[0] + i[7] * t[1] + i[8] * t[2]),
    ];
    [
        i[0], i[1], i[2], nt[0], //
        i[3], i[4], i[5], nt[1], //
        i[6], i[7], i[8], nt[2], //
        0.0, 0.0, 0.0, 1.0,
    ]
}

/// One animation keyframe for a bone (any of position/scale/rotation may be
/// absent, per the `KEYS` flags). Rotation is a quaternion `(w,x,y,z)`.
#[derive(Debug, Clone)]
pub struct B3dKey {
    pub frame: i32,
    pub position: Option<[f32; 3]>,
    pub scale: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
}

/// A skeleton bone/joint: a node in the B3D hierarchy with its bind-pose
/// transforms, the vertices it skins (by mesh vertex id + weight), and its
/// animation keyframes.
#[derive(Debug, Clone)]
pub struct B3dBone {
    pub name: String,
    /// Index of the parent bone in [`B3dModel::bones`], or `None` for a root.
    pub parent: Option<usize>,
    /// This node's own local transform (`T·R·S`) in the bind pose.
    pub local_bind: Mat4,
    /// Accumulated bind-pose world matrix.
    pub bind_world: Mat4,
    /// `bind_world⁻¹` — maps a vertex from model space into this bone's space.
    pub inverse_bind: Mat4,
    /// `(vertex_id, weight)` into the (single) mesh's vertex array.
    pub weights: Vec<(u32, f32)>,
    /// Per-bone animation keyframes (sorted by frame as stored).
    pub keys: Vec<B3dKey>,
}

/// Top-level animation header (`ANIM`).
#[derive(Debug, Clone, Copy, Default)]
pub struct B3dAnim {
    pub frames: i32,
    pub fps: f32,
}

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
    /// Skeleton — every `NODE` in hierarchy order (parents before children),
    /// with bind-pose transforms, skin weights, and animation keyframes. Empty
    /// for unskinned meshes.
    pub bones: Vec<B3dBone>,
    /// Animation header (`ANIM`): total frames + fps. `None` if not animated.
    pub anim: Option<B3dAnim>,
}

impl B3dModel {
    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(|m| m.positions.len()).sum()
    }

    /// Bind-pose world position of the first bone/node whose name matches
    /// `name` (case-insensitive), e.g. `"Head"`. `None` if absent.
    pub fn joint_pos(&self, name: &str) -> Option<[f32; 3]> {
        self.bones
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name))
            .map(|b| [b.bind_world[3], b.bind_world[7], b.bind_world[11]])
    }

    /// Total skin-weight count across all bones (diagnostic).
    pub fn weight_count(&self) -> usize {
        self.bones.iter().map(|b| b.weights.len()).sum()
    }
    /// Total keyframe count across all bones (diagnostic).
    pub fn keyframe_count(&self) -> usize {
        self.bones.iter().map(|b| b.keys.len()).sum()
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
        parse_chunks(&mut r, end.min(data.len()), &IDENTITY, &mut model)?;
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

/// Walk sibling chunks until `end`. `parent` is the accumulated node matrix.
fn parse_chunks(
    r: &mut BlitzReader,
    end: usize,
    parent: &Mat4,
    model: &mut B3dModel,
) -> Result<(), ReadError> {
    while r.position() + 8 <= end {
        let (tag, size) = chunk_header(r)?;
        let chunk_end = (r.position() + size).min(end);
        match &tag {
            b"TEXS" => parse_texs(r, chunk_end, model)?,
            b"BRUS" => parse_brus(r, chunk_end, model)?,
            b"NODE" => parse_node(r, chunk_end, parent, None, model)?,
            b"ANIM" => parse_anim(r, model)?,
            _ => {}
        }
        r.seek(chunk_end)?;
    }
    Ok(())
}

/// `ANIM`: flags(i32) · frames(i32) · fps(f32).
fn parse_anim(r: &mut BlitzReader, model: &mut B3dModel) -> Result<(), ReadError> {
    let _flags = r.read_int()?;
    let frames = r.read_int()?;
    let fps = r.read_float()?;
    model.anim = Some(B3dAnim { frames, fps });
    Ok(())
}

/// `BONE`: a sequence of `{vertex_id(i32), weight(f32)}` for the bone's node.
fn parse_bone(r: &mut BlitzReader, end: usize) -> Result<Vec<(u32, f32)>, ReadError> {
    let mut w = Vec::new();
    while r.position() + 8 <= end {
        let vid = r.read_int()? as u32;
        let weight = r.read_float()?;
        w.push((vid, weight));
    }
    Ok(w)
}

/// `KEYS`: flags(i32) then per key `frame(i32)` + optional position(3f, flag&1)
/// + scale(3f, flag&2) + rotation(4f w,x,y,z, flag&4).
fn parse_keys(r: &mut BlitzReader, end: usize) -> Result<Vec<B3dKey>, ReadError> {
    let flags = r.read_int()?;
    let (has_pos, has_scale, has_rot) = (flags & 1 != 0, flags & 2 != 0, flags & 4 != 0);
    let mut keys = Vec::new();
    while r.position() + 4 <= end {
        let frame = r.read_int()?;
        let position = if has_pos {
            Some([r.read_float()?, r.read_float()?, r.read_float()?])
        } else {
            None
        };
        let scale = if has_scale {
            Some([r.read_float()?, r.read_float()?, r.read_float()?])
        } else {
            None
        };
        let rotation = if has_rot {
            Some([r.read_float()?, r.read_float()?, r.read_float()?, r.read_float()?])
        } else {
            None
        };
        keys.push(B3dKey {
            frame,
            position,
            scale,
            rotation,
        });
    }
    Ok(keys)
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

/// `NODE`: name(cstr) · position(3f) · scale(3f) · rotation(4f, w,x,y,z) ·
/// sub-chunks. Composes the node's bind-pose matrix onto `parent`.
fn parse_node(
    r: &mut BlitzReader,
    end: usize,
    parent: &Mat4,
    parent_bone: Option<usize>,
    model: &mut B3dModel,
) -> Result<(), ReadError> {
    let name = r.read_cstr(256)?;
    let px = r.read_float()?;
    let py = r.read_float()?;
    let pz = r.read_float()?;
    let sx = r.read_float()?;
    let sy = r.read_float()?;
    let sz = r.read_float()?;
    let rw = r.read_float()?;
    let rx = r.read_float()?;
    let ry = r.read_float()?;
    let rz = r.read_float()?;
    let local = trs([px, py, pz], [rw, rx, ry, rz], [sx, sy, sz]);
    let world = mat_mul(parent, &local);

    // Every node is a skeleton joint; weights/keys filled from sub-chunks below.
    let this_bone = model.bones.len();
    model.bones.push(B3dBone {
        name,
        parent: parent_bone,
        local_bind: local,
        bind_world: world,
        inverse_bind: invert_affine(&world),
        weights: Vec::new(),
        keys: Vec::new(),
    });

    while r.position() + 8 <= end {
        let (tag, size) = chunk_header(r)?;
        let chunk_end = (r.position() + size).min(end);
        match &tag {
            b"MESH" => {
                for mesh in parse_mesh(r, chunk_end, &world)? {
                    if !mesh.positions.is_empty() && !mesh.indices.is_empty() {
                        model.meshes.push(mesh);
                    }
                }
            }
            b"NODE" => parse_node(r, chunk_end, &world, Some(this_bone), model)?,
            b"BONE" => {
                let w = parse_bone(r, chunk_end)?;
                model.bones[this_bone].weights = w;
            }
            b"KEYS" => {
                let k = parse_keys(r, chunk_end)?;
                model.bones[this_bone].keys = k;
            }
            b"ANIM" => parse_anim(r, model)?,
            _ => {} // unknown sub-chunk
        }
        r.seek(chunk_end)?;
    }
    Ok(())
}

/// `MESH`: meshBrush(i32) · `VRTS`(shared vertices) · `TRIS`(one or more, each a
/// brush_id + index list). Emits one [`B3dMesh`] per TRIS group (so each can
/// carry its own texture), sharing the mesh's vertex data.
fn parse_mesh(r: &mut BlitzReader, end: usize, world: &Mat4) -> Result<Vec<B3dMesh>, ReadError> {
    let mesh_brush = r.read_int()?;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut groups: Vec<(i32, Vec<u32>)> = Vec::new();

    while r.position() + 8 <= end {
        let (tag, size) = chunk_header(r)?;
        let chunk_end = (r.position() + size).min(end);
        match &tag {
            b"VRTS" => parse_vrts(r, chunk_end, world, &mut positions, &mut normals, &mut uvs)?,
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
    world: &Mat4,
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
        positions.push(xform_point(world, [x, y, z]));

        if has_normal {
            let nx = r.read_float()?;
            let ny = r.read_float()?;
            let nz = r.read_float()?;
            normals.push(xform_dir(world, [nx, ny, nz]));
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

#[cfg(test)]
mod mat_tests {
    use super::*;

    #[test]
    fn identity_is_neutral() {
        let p = xform_point(&IDENTITY, [3.0, -2.0, 5.0]);
        assert_eq!(p, [3.0, -2.0, 5.0]);
    }

    #[test]
    fn trs_rotates_then_translates() {
        // 90deg about Y (quat w=cos45,x=0,y=sin45,z=0) maps +X -> -Z, then
        // translate by (10,0,0). So (1,0,0) -> (10,0,-1).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let m = trs([10.0, 0.0, 0.0], [s, 0.0, s, 0.0], [1.0, 1.0, 1.0]);
        let p = xform_point(&m, [1.0, 0.0, 0.0]);
        assert!((p[0] - 10.0).abs() < 1e-4, "x={}", p[0]);
        assert!(p[1].abs() < 1e-4, "y={}", p[1]);
        assert!((p[2] + 1.0).abs() < 1e-4, "z={}", p[2]);
        // Directions ignore translation.
        let d = xform_dir(&m, [1.0, 0.0, 0.0]);
        assert!((d[2] + 1.0).abs() < 1e-4 && d[0].abs() < 1e-4);
    }

    #[test]
    fn nested_matrices_compose() {
        // Parent translates +X by 5, child translates +X by 3 in parent frame.
        let parent = trs([5.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [1.0; 3]);
        let child = trs([3.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [1.0; 3]);
        let world = mat_mul(&parent, &child);
        assert_eq!(xform_point(&world, [0.0, 0.0, 0.0]), [8.0, 0.0, 0.0]);
    }
}
