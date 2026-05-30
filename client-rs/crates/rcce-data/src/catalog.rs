//! Indexed media catalogs: `Meshes.dat`, `Textures.dat`, `Sounds.dat`,
//! `Music.dat`.
//!
//! All four share the same shape (verified in `src/Modules/Media.bb`):
//!
//! ```text
//! [ index: 65535 little-endian i32 file offsets, one per ID 0..=65534 ]
//! [ data:  variable-length records, pointed at by the index           ]
//! ```
//!
//! Index slot `ID` holds the absolute file offset of that ID's record, or `0`
//! if the slot is unused (`GetMesh`/`GetTexture` both treat offset `0` as
//! "empty" — `Media.bb:808`). The per-record layout differs per catalog; see
//! each parser below. The index is read with `SeekFile F, ID * 4`
//! (`Media.bb:806`), so the header is exactly `65535 * 4 = 262_140` bytes.

use crate::reader::{BlitzReader, ReadError};

/// Number of addressable IDs (0..=65534). The index has one i32 per ID.
pub const CATALOG_SLOTS: usize = 65535;
const INDEX_BYTES: usize = CATALOG_SLOTS * 4;

/// One mesh record from `Meshes.dat`.
///
/// Layout per record (`Media.bb:817-823`, in order):
/// `IsAnim:u8, Scale:f32, X:f32, Y:f32, Z:f32, Shader:i16, Filename:str(260)`.
/// The on-disk filename is relative to `Data\Meshes\`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshEntry {
    pub id: u16,
    pub is_anim: bool,
    pub scale: f32,
    pub offset: [f32; 3],
    pub shader: i16,
    /// Path relative to `Data/Meshes/`, as stored (Windows separators).
    pub filename: String,
}

/// A parsed `Meshes.dat`: only the populated slots, in ascending ID order.
#[derive(Debug, Clone, Default)]
pub struct MeshCatalog {
    pub entries: Vec<MeshEntry>,
}

impl MeshCatalog {
    /// Parse a whole `Meshes.dat` image.
    ///
    /// Walks all 65535 index slots, follows each non-zero offset, and decodes
    /// the record. A record that fails to decode (truncated / bad length) is
    /// skipped rather than aborting the whole catalog — this matches the
    /// engine, where `GetMesh` simply returns 0 for a bad entry and the rest of
    /// the world still loads. Skips are returned for the caller to log.
    pub fn parse(data: &[u8]) -> Result<ParsedCatalog<MeshCatalog>, ReadError> {
        if data.len() < INDEX_BYTES {
            return Err(ReadError::UnexpectedEof {
                offset: 0,
                needed: INDEX_BYTES,
                available: data.len(),
            });
        }
        let mut entries = Vec::new();
        let mut skipped = Vec::new();

        for id in 0..CATALOG_SLOTS {
            let i = id * 4;
            let offset = i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            if offset == 0 {
                continue; // unused slot
            }
            if offset < 0 {
                skipped.push((id as u16, "negative offset"));
                continue;
            }
            match Self::parse_record(data, id as u16, offset as usize) {
                Ok(entry) => entries.push(entry),
                Err(_) => skipped.push((id as u16, "record decode failed")),
            }
        }

        Ok(ParsedCatalog {
            value: MeshCatalog { entries },
            skipped,
        })
    }

    fn parse_record(data: &[u8], id: u16, offset: usize) -> Result<MeshEntry, ReadError> {
        let mut r = BlitzReader::new(data);
        r.seek(offset)?;
        let is_anim = r.read_byte()? != 0;
        let scale = r.read_float()?;
        let x = r.read_float()?;
        let y = r.read_float()?;
        let z = r.read_float()?;
        let shader = r.read_short()?;
        let filename = r.read_string(260)?;
        Ok(MeshEntry {
            id,
            is_anim,
            scale,
            offset: [x, y, z],
            shader,
            filename,
        })
    }

    /// Look up an entry by ID (linear; entries are ID-sorted so callers that
    /// need random access should build a map). Returns `None` for empty slots.
    pub fn get(&self, id: u16) -> Option<&MeshEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Result of parsing a catalog: the value plus any slots that were skipped
/// because their record failed to decode (logged, not fatal — see `parse`).
#[derive(Debug, Clone)]
pub struct ParsedCatalog<T> {
    pub value: T,
    pub skipped: Vec<(u16, &'static str)>,
}

/// One texture record from `Textures.dat` (`Media.bb:904`): `Flags:i16` then a
/// length-prefixed `Filename` (relative to `Data/Textures/`). Same 65535-slot
/// i32 index as `Meshes.dat`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureEntry {
    pub id: u16,
    pub flags: i16,
    /// Path relative to `Data/Textures/`, as stored (Windows separators).
    pub filename: String,
}

/// A parsed `Textures.dat`: only populated slots, ascending by id.
#[derive(Debug, Clone, Default)]
pub struct TextureCatalog {
    pub entries: Vec<TextureEntry>,
}

impl TextureCatalog {
    pub fn parse(data: &[u8]) -> Result<ParsedCatalog<TextureCatalog>, ReadError> {
        if data.len() < INDEX_BYTES {
            return Err(ReadError::UnexpectedEof {
                offset: 0,
                needed: INDEX_BYTES,
                available: data.len(),
            });
        }
        let mut entries = Vec::new();
        let mut skipped = Vec::new();
        for id in 0..CATALOG_SLOTS {
            let i = id * 4;
            let offset = i32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            if offset <= 0 {
                if offset < 0 {
                    skipped.push((id as u16, "negative offset"));
                }
                continue;
            }
            let mut r = BlitzReader::new(data);
            let parsed = (|| {
                r.seek(offset as usize)?;
                let flags = r.read_short()?;
                let filename = r.read_string(260)?;
                Ok::<_, ReadError>(TextureEntry {
                    id: id as u16,
                    flags,
                    filename,
                })
            })();
            match parsed {
                Ok(e) => entries.push(e),
                Err(_) => skipped.push((id as u16, "record decode failed")),
            }
        }
        Ok(ParsedCatalog {
            value: TextureCatalog { entries },
            skipped,
        })
    }

    pub fn get(&self, id: u16) -> Option<&TextureEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}
