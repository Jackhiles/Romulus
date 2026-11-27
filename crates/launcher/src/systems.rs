//! RomM platform → RetroDECK system folder.
//!
//! RetroDECK uses ES-DE's folder names. Most RomM slugs are the same; this table covers
//! the ones that differ. A folder is only returned if it exists, because RetroDECK works
//! out the system (and emulator) from the folder a game is in.

use std::path::Path;

/// RomM slug → RetroDECK folder, where they differ.
const ALIASES: &[(&str, &str)] = &[
    ("dc", "dreamcast"),
    ("ngc", "gc"),
    ("3ds", "n3ds"),
    ("sms", "mastersystem"),
    ("sega32", "sega32x"),
    ("tg16", "pcengine"),
    ("turbografx-cd", "pcenginecd"),
    ("neo-geo-pocket", "ngp"),
    ("neo-geo-pocket-color", "ngpc"),
    ("neogeoaes", "neogeo"),
    ("neogeomvs", "neogeo"),
    ("neo-geo-cd", "neogeocd"),
    ("atari-jaguar", "atarijaguar"),
    ("lynx", "atarilynx"),
    ("jaguar", "atarijaguar"),
    ("win3x", "windows3x"),
];

/// The RetroDECK folder name for a platform, if one exists under `roms`.
pub fn folder_for(roms: &Path, fs_slug: &str, slug: &str) -> Option<String> {
    let mut candidates: Vec<&str> = ALIASES
        .iter()
        .filter(|(from, _)| *from == fs_slug || *from == slug)
        .map(|(_, to)| *to)
        .collect();
    candidates.push(fs_slug);
    candidates.push(slug);
    candidates
        .into_iter()
        .find(|c| !c.is_empty() && !c.contains(['/', '\\']) && *c != ".." && roms.join(c).is_dir())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_aliases_and_identity_only_when_the_folder_exists() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["dreamcast", "gc", "psx"] {
            std::fs::create_dir(dir.path().join(f)).unwrap();
        }
        assert_eq!(
            folder_for(dir.path(), "dc", "dc").as_deref(),
            Some("dreamcast")
        );
        assert_eq!(folder_for(dir.path(), "ngc", "ngc").as_deref(), Some("gc"));
        assert_eq!(folder_for(dir.path(), "psx", "psx").as_deref(), Some("psx"));
        assert_eq!(folder_for(dir.path(), "win", "win"), None);
        assert_eq!(folder_for(dir.path(), "..", ".."), None);
    }
}
