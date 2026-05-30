//! Data-driven asset resolution: actor template id → base mesh id (`Actors.dat`)
//! → `.b3d` path (`Meshes.dat` catalog) → parsed [`B3dModel`], with caching.
//! Reads the same files the GUE editor writes, so the client draws each actor
//! as its real model.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use rcce_data::{ActorCatalog, B3dModel, MeshCatalog};

pub struct AssetStore {
    data_root: PathBuf,
    actors: ActorCatalog,
    meshes: MeshCatalog,
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
        Ok(Self {
            data_root,
            actors,
            meshes,
            cache: HashMap::new(),
        })
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
