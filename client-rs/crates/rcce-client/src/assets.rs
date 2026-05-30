//! Data-driven asset resolution: actor template id → base mesh id (`Actors.dat`)
//! → `.b3d` path (`Meshes.dat` catalog) → parsed [`B3dModel`], with caching.
//! Reads the same files the GUE editor writes, so the client draws each actor
//! as its real model.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use rcce_data::{texture, ActorCatalog, B3dModel, Image, MeshCatalog, TextureCatalog};

pub struct AssetStore {
    data_root: PathBuf,
    actors: ActorCatalog,
    meshes: MeshCatalog,
    textures: TextureCatalog,
    cache: HashMap<u16, Option<Rc<B3dModel>>>,
}

impl AssetStore {
    /// `data_root` is the project `data/` directory (containing `Server Data/`,
    /// `Game Data/`, `Meshes/`).
    pub fn load(data_root: impl Into<PathBuf>) -> Result<Self, String> {
        let data_root = data_root.into();
        let actors_bytes = std::fs::read(data_root.join("Server Data/Actors.dat"))
            .map_err(|e| format!("Actors.dat: {e}"))?;
        let mesh_bytes = std::fs::read(data_root.join("Game Data/Meshes.dat"))
            .map_err(|e| format!("Meshes.dat: {e}"))?;
        let actors = ActorCatalog::parse(&actors_bytes).map_err(|e| format!("Actors.dat: {e}"))?;
        let meshes = MeshCatalog::parse(&mesh_bytes)
            .map_err(|e| format!("Meshes.dat: {e}"))?
            .value;
        // Texture catalog (for real actor skins). Non-fatal if absent.
        let textures = std::fs::read(data_root.join("Game Data/Textures.dat"))
            .ok()
            .and_then(|b| TextureCatalog::parse(&b).ok())
            .map(|p| p.value)
            .unwrap_or_default();
        Ok(Self {
            data_root,
            actors,
            meshes,
            textures,
            cache: HashMap::new(),
        })
    }

    /// Template gender-mode (`Actors.dat` `Genders`) for every actor template,
    /// keyed by id. The packet decoder needs this to know whether a P_NewActor
    /// carries a gender byte (only when mode == 0).
    pub fn template_genders(&self) -> HashMap<u16, u8> {
        self.actors
            .templates
            .iter()
            .map(|(&id, t)| (id, t.genders))
            .collect()
    }

    /// Resolve a texture-catalog id to an on-disk path under `data/Textures/`.
    fn skin_path(&self, id: u16) -> Option<PathBuf> {
        if id == 65535 {
            return None;
        }
        let entry = self.textures.get(id)?;
        let p = self
            .data_root
            .join("Textures")
            .join(entry.filename.replace('\\', "/"));
        p.exists().then_some(p)
    }

    /// The base body model for an actor template + gender (0 male / 1 female).
    pub fn actor_model(&mut self, template_id: u16, gender: u8) -> Option<Rc<B3dModel>> {
        let mesh_id = self.actors.mesh_for(template_id, gender)?;
        self.mesh_model(mesh_id)
    }

    /// The actor's in-world render scale, matching the engine
    /// (`Actors3D.bb:45`): `0.05 × LoadedMeshScales[mesh] × Actor.Scale`.
    /// Positions stay in raw world units. Falls back to `0.05` if a stored
    /// scale is non-positive.
    pub fn actor_render_scale(&self, template_id: u16, gender: u8) -> Option<f32> {
        let mesh_id = self.actors.mesh_for(template_id, gender)?;
        let mesh = self.meshes.get(mesh_id)?;
        let actor = self.actors.templates.get(&template_id)?;
        let ms = if mesh.scale > 0.0 { mesh.scale } else { 1.0 };
        let as_ = if actor.scale > 0.0 { actor.scale } else { 1.0 };
        Some(0.05 * ms * as_)
    }

    /// Textures for an actor's model, one per mesh (aligned to
    /// `actor_model(...).meshes`). Each mesh's B3D texture filename is resolved
    /// by basename against the mesh's own directory and the project texture
    /// trees, then decoded (BMP/PNG/JPG). `None` where unresolved/undecodable.
    pub fn actor_textures(
        &mut self,
        template_id: u16,
        gender: u8,
        face_sel: u8,
        body_sel: u8,
    ) -> Vec<Option<Image>> {
        let Some(mesh_id) = self.actors.mesh_for(template_id, gender) else {
            return Vec::new();
        };

        // Real skins from this actor's chosen body/face texture selection
        // (0..4). These replace the b3d's embedded UV-guide textures.
        let fi = (face_sel as usize).min(4);
        let bi = (body_sel as usize).min(4);
        let (face_skin, body_skin) = match self.actors.templates.get(&template_id) {
            Some(t) => {
                let (faces, bodies) = if gender == 1 {
                    (t.female_face_ids, t.female_body_ids)
                } else {
                    (t.male_face_ids, t.male_body_ids)
                };
                (self.skin_path(faces[fi]), self.skin_path(bodies[bi]))
            }
            None => (None, None),
        };

        // Fallback search roots for the b3d's own textures.
        let mut roots = Vec::new();
        if let Some(entry) = self.meshes.get(mesh_id) {
            let rel = entry.filename.replace('\\', "/");
            if let Some(dir) = self.data_root.join("Meshes").join(&rel).parent() {
                roots.push(dir.to_path_buf());
            }
        }
        roots.push(self.data_root.join("Textures"));
        roots.push(self.data_root.join("Meshes"));

        let Some(model) = self.mesh_model(mesh_id) else {
            return Vec::new();
        };
        model
            .meshes
            .iter()
            .map(|m| {
                // Surface type from the b3d texture name: head/face vs body.
                let is_face = m
                    .texture
                    .as_deref()
                    .map(texture::basename)
                    .map(|b| {
                        let l = b.to_ascii_lowercase();
                        l.contains("head") || l.contains("face")
                    })
                    .unwrap_or(false);
                let skin = if is_face { &face_skin } else { &body_skin };
                // Prefer the real actor skin; fall back to the b3d's texture.
                skin.as_ref()
                    .and_then(|p| texture::load(p))
                    .or_else(|| {
                        m.texture
                            .as_ref()
                            .and_then(|name| texture::find_texture(&roots, name))
                            .and_then(|p| texture::load(&p))
                    })
            })
            .collect()
    }

    /// Textures for a plain scenery mesh, one per sub-mesh (aligned to
    /// `mesh_model(mesh_id).meshes`). Resolves each sub-mesh's own B3D texture
    /// name against the mesh directory and the project texture trees. If
    /// `retexture_id` is a real texture-catalog id (not 65535), it overrides
    /// every sub-mesh (the engine's scenery `TextureID` retexture).
    pub fn scenery_textures(&mut self, mesh_id: u16, retexture_id: u16) -> Vec<Option<Image>> {
        // Optional whole-mesh retexture from the area file's TextureID.
        let retex = self
            .skin_path(retexture_id)
            .and_then(|p| texture::load(&p));

        // Search roots: the mesh's own directory, then the texture trees.
        let mut roots = Vec::new();
        if let Some(entry) = self.meshes.get(mesh_id) {
            let rel = entry.filename.replace('\\', "/");
            if let Some(dir) = self.data_root.join("Meshes").join(&rel).parent() {
                roots.push(dir.to_path_buf());
            }
        }
        roots.push(self.data_root.join("Textures"));
        roots.push(self.data_root.join("Meshes"));

        let Some(model) = self.mesh_model(mesh_id) else {
            return Vec::new();
        };
        model
            .meshes
            .iter()
            .map(|m| {
                if let Some(img) = &retex {
                    return Some(img.clone());
                }
                m.texture
                    .as_ref()
                    .and_then(|name| texture::find_texture(&roots, name))
                    .and_then(|p| texture::load(&p))
            })
            .collect()
    }

    /// A model by mesh-catalog id, cached (including negative cache for misses).
    pub fn mesh_model(&mut self, mesh_id: u16) -> Option<Rc<B3dModel>> {
        if let Some(cached) = self.cache.get(&mesh_id) {
            return cached.clone();
        }
        let result = self
            .meshes
            .get(mesh_id)
            .and_then(|entry| {
                let path = self
                    .data_root
                    .join("Meshes")
                    .join(entry.filename.replace('\\', "/"));
                std::fs::read(path).ok()
            })
            .and_then(|bytes| B3dModel::parse(&bytes).ok())
            .map(Rc::new);
        self.cache.insert(mesh_id, result.clone());
        result
    }
}
