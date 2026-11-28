//! GameCube and Wii disc images: the game ID in the disc header.
//!
//! The header starts with the 6-character game ID (`GM8E01`: game code `GM8E`, maker
//! `01`); a Wii disc has the magic word 0x5D1C9EA3 at 0x18, a GameCube disc 0xC2339F3D
//! at 0x1C. Where the header sits depends on the container (Dolphin's DiscIO):
//! ISO/GCM at 0, CISO after its 0x8000-byte block map, WBFS at 0x200 (its first disc's
//! header copy), RVZ/WIA at 0x58 (the `disc_t` header copy after the 0x48-byte file
//! header). GCZ stores the header compressed and isn't read.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disc {
    /// `GM8E01`.
    pub id: String,
    pub wii: bool,
}

impl Disc {
    /// The 4-character game code (`GM8E`), used in GCI names and Wii title IDs.
    pub fn code(&self) -> &str {
        &self.id[..4]
    }

    /// Dolphin's GameCube memory card region folder for this game.
    pub fn gc_region(&self) -> &'static str {
        gc_region(self.code())
    }

    /// The Wii save's title ID low half: the game code as hex (`RMCE` → `524d4345`).
    pub fn wii_title(&self) -> String {
        hex::encode(self.code().as_bytes())
    }
}

/// Dolphin's GameCube memory card region folder (`GC/<region>/Card A`) for a game code:
/// its last letter is the country.
pub fn gc_region(code: &str) -> &'static str {
    match code.as_bytes().get(3) {
        Some(b'E' | b'N' | b'Z') => "USA",
        Some(b'J' | b'W' | b'K' | b'Q' | b'T' | b'C') => "JAP",
        _ => "EUR",
    }
}

/// A plausible 4-character game code (RomM reports the container's magic, `CISO`, for
/// some images, so that is refused).
pub fn valid_code(code: &str) -> bool {
    code.len() == 4
        && code != "CISO"
        && code
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn header_offset(ext: &str) -> Option<u64> {
    Some(match ext {
        "iso" | "gcm" => 0,
        "ciso" => 0x8000,
        "wbfs" => 0x200,
        "rvz" | "wia" => 0x58,
        _ => return None,
    })
}

/// Parse a raw disc header (at least 0x20 bytes).
pub fn parse_header(h: &[u8]) -> Option<Disc> {
    let id = h.get(..6)?;
    if !id
        .iter()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return None;
    }
    let word = |at: usize| {
        h.get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let wii = word(0x18) == Some(0x5D1C_9EA3);
    let gc = word(0x1C) == Some(0xC233_9F3D);
    (wii || gc).then(|| Disc {
        id: String::from_utf8_lossy(id).into_owned(),
        wii,
    })
}

/// Read the disc header of the image at `path` (following an `.m3u` to its first
/// disc). None if the format isn't readable or the header isn't a disc header.
pub fn read(path: &Path) -> Option<Disc> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if ext == "m3u" {
        let text = std::fs::read_to_string(path).ok()?;
        let first = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#'))?;
        let disc = path.parent()?.join(first);
        return (disc.extension().and_then(|e| e.to_str()) != Some("m3u"))
            .then(|| read(&disc))
            .flatten();
    }
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(header_offset(&ext)?)).ok()?;
    let mut h = [0u8; 0x20];
    f.read_exact(&mut h).ok()?;
    parse_header(&h)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn header(id: &str, wii: bool) -> Vec<u8> {
        let mut h = vec![0u8; 0x20];
        h[..6].copy_from_slice(id.as_bytes());
        if wii {
            h[0x18..0x1C].copy_from_slice(&0x5D1C_9EA3u32.to_be_bytes());
        } else {
            h[0x1C..0x20].copy_from_slice(&0xC233_9F3Du32.to_be_bytes());
        }
        h
    }

    #[test]
    fn reads_ids_from_each_container() {
        let d = tempfile::tempdir().unwrap();
        for (name, at, id, wii) in [
            ("Metroid Prime.iso", 0usize, "GM8E01", false),
            ("Batman.ciso", 0x8000, "GBME7D", false),
            ("Mario Kart Wii.wbfs", 0x200, "RMCE01", true),
            ("Zelda.rvz", 0x58, "RZDP01", true),
        ] {
            let mut b = vec![0u8; at];
            b.extend(header(id, wii));
            b.extend([0u8; 64]);
            std::fs::write(d.path().join(name), b).unwrap();
            let disc = read(&d.path().join(name)).unwrap();
            assert_eq!((disc.id.as_str(), disc.wii), (id, wii), "{name}");
        }
        // A playlist points at its first disc.
        std::fs::write(d.path().join("Game.m3u"), "#EXTM3U\nMetroid Prime.iso\n").unwrap();
        assert_eq!(read(&d.path().join("Game.m3u")).unwrap().id, "GM8E01");
        // Not a disc header, or a format that can't be read.
        std::fs::write(d.path().join("junk.iso"), [0u8; 64]).unwrap();
        assert_eq!(read(&d.path().join("junk.iso")), None);
        assert_eq!(read(&d.path().join("x.gcz")), None);
    }

    #[test]
    fn regions_and_title_ids() {
        let disc = |id: &str| Disc {
            id: id.into(),
            wii: false,
        };
        assert_eq!(disc("GM8E01").gc_region(), "USA");
        assert_eq!(disc("GM8P01").gc_region(), "EUR");
        assert_eq!(disc("GM8J01").gc_region(), "JAP");
        // RomM's save_target for Mario Kart Wii (RMCE01).
        assert_eq!(disc("RMCE01").wii_title(), "524d4345");
    }
}
