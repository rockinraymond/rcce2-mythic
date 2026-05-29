//! `rcce-data` — parsers for the on-disk project files an RCCE2 client must
//! read. The GUE editor keeps writing these formats; this crate reads them
//! unchanged so the Rust client is a true drop-in. No format is ever modified.
//!
//! See `docs/rust-client/PLAN.md` for the full porting plan. Phase 1 covers the
//! indexed media catalogs; B3D meshes, area `.dat`, and `Accounts.dat` saves
//! land next in this crate.

pub mod catalog;
pub mod reader;

pub use catalog::{MeshCatalog, MeshEntry, ParsedCatalog, CATALOG_SLOTS};
pub use reader::{BlitzReader, ReadError};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Repo root, three levels up from this crate's manifest
    /// (`client-rs/crates/rcce-data` → worktree root). The real `data/` tree
    /// lives there.
    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root above client-rs/crates/rcce-data")
            .to_path_buf()
    }

    #[test]
    fn blitz_reader_byte_orders() {
        // 0x01 byte, short 0x0002 LE, int 0x00000003 LE, float 1.0 LE.
        let mut buf = vec![0x01u8];
        buf.extend_from_slice(&2i16.to_le_bytes());
        buf.extend_from_slice(&3i32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        // length-prefixed string "hi": 4-byte LE len + bytes.
        buf.extend_from_slice(&2i32.to_le_bytes());
        buf.extend_from_slice(b"hi");

        let mut r = BlitzReader::new(&buf);
        assert_eq!(r.read_byte().unwrap(), 1);
        assert_eq!(r.read_short().unwrap(), 2);
        assert_eq!(r.read_int().unwrap(), 3);
        assert_eq!(r.read_float().unwrap(), 1.0);
        assert_eq!(r.read_string(260).unwrap(), "hi");
        assert!(r.eof());
    }

    #[test]
    fn reader_rejects_overlong_string() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&999i32.to_le_bytes()); // claims 999 bytes
        let mut r = BlitzReader::new(&buf);
        assert!(matches!(
            r.read_string(260),
            Err(ReadError::StringTooLong { len: 999, max: 260 })
        ));
    }

    #[test]
    fn reader_negative_or_zero_string_is_empty() {
        for len in [0i32, -1i32] {
            let buf = len.to_le_bytes();
            let mut r = BlitzReader::new(&buf);
            assert_eq!(r.read_string(260).unwrap(), "");
        }
    }

    /// Ground-truth test: parse the real `Meshes.dat` shipped in `data/`.
    /// Skips (does not fail) when the file is absent so the suite still runs
    /// in checkouts without the data tree.
    #[test]
    fn parse_real_meshes_dat() {
        let path = repo_root().join("data/Game Data/Meshes.dat");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("skipping: {} not present", path.display());
            return;
        };

        let parsed = MeshCatalog::parse(&bytes).expect("Meshes.dat should parse");
        let cat = &parsed.value;

        // The file is index (262_140 bytes) + records, so it must exceed the
        // bare index and contain at least one populated slot.
        assert!(
            bytes.len() >= CATALOG_SLOTS * 4,
            "file smaller than the 65535-slot index"
        );
        assert!(
            !cat.entries.is_empty(),
            "expected at least one mesh entry in the shipped catalog"
        );

        // Every decoded entry must be sane: a non-empty filename, finite
        // scale, no path traversal (the loader rejects `..` at Media.bb:832).
        for e in &cat.entries {
            assert!(!e.filename.is_empty(), "id {} has empty filename", e.id);
            assert!(e.scale.is_finite(), "id {} has non-finite scale", e.id);
            assert!(
                !e.filename.contains(".."),
                "id {} filename has traversal: {}",
                e.id,
                e.filename
            );
        }

        eprintln!(
            "parsed {} mesh entries ({} slots skipped); first: {:?}",
            cat.entries.len(),
            parsed.skipped.len(),
            cat.entries.first()
        );
    }
}
