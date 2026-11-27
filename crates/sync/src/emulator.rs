//! Which emulator RetroDECK will use for a game, read the way RetroDECK reads it
//! (its `run_game.sh`): the game's `<altemulator>` in the ES-DE gamelist, else the
//! system's `<alternativeEmulator>`, else the first `<command>` for the system in
//! `es_systems.xml`.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Emulator {
    /// A RetroArch core, e.g. `swanstation`.
    RetroArch {
        core: String,
    },
    Pcsx2,
    /// Anything else, by its ES-DE label (e.g. "DuckStation (Standalone)").
    Other {
        label: String,
    },
}

impl Emulator {
    /// The `emulator` name used for saves on RomM (as decky-romm-sync used them).
    pub fn romm_name(&self) -> String {
        match self {
            Emulator::RetroArch { core } => format!("retroarch-{core}"),
            Emulator::Pcsx2 => "pcsx2".into(),
            Emulator::Other { label } => label
                .to_lowercase()
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|w| !w.is_empty() && *w != "standalone")
                .collect::<Vec<_>>()
                .join("-"),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Emulator::RetroArch { core } => format!("RetroArch ({core})"),
            Emulator::Pcsx2 => "PCSX2".into(),
            Emulator::Other { label } => label.clone(),
        }
    }

    fn from_command(label: &str, command: &str) -> Self {
        if command.contains("%EMULATOR_RETROARCH%")
            && let Some(rest) = command.split("%CORE_RETROARCH%/").nth(1)
        {
            let core = rest.split("_libretro").next().unwrap_or_default();
            if !core.is_empty() {
                return Emulator::RetroArch {
                    core: core.to_string(),
                };
            }
        }
        if command.contains("%EMULATOR_PCSX2%") {
            return Emulator::Pcsx2;
        }
        Emulator::Other {
            label: label.to_string(),
        }
    }
}

/// Where ES-DE's configuration is.
#[derive(Debug, Clone)]
pub struct EsDe {
    /// RetroDECK's bundled `es_systems.xml` (Linux).
    pub systems: PathBuf,
    /// The user's custom `es_systems.xml`, which can override systems.
    pub custom_systems: Option<PathBuf>,
    /// `<retrodeck>/ES-DE/gamelists`.
    pub gamelists: PathBuf,
}

impl EsDe {
    /// RetroDECK's locations for a user with home `home` and RetroDECK folder `rd_home`.
    pub fn retrodeck(home: &Path, rd_home: &Path) -> Self {
        let rel = "net.retrodeck.retrodeck/current/active/files/retrodeck/components/es-de/share/es-de/resources/systems/linux/es_systems.xml";
        let user = home.join(".local/share/flatpak/app").join(rel);
        let systems = if user.is_file() {
            user
        } else {
            Path::new("/var/lib/flatpak/app").join(rel)
        };
        let custom = home
            .join(".var/app/net.retrodeck.retrodeck/config/ES-DE/custom_systems/es_systems.xml");
        Self {
            systems,
            custom_systems: custom.is_file().then_some(custom),
            gamelists: rd_home.join("ES-DE/gamelists"),
        }
    }
}

fn commands(xml: &str, system: &str) -> Vec<(String, String)> {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return Vec::new();
    };
    doc.descendants()
        .filter(|n| n.has_tag_name("system"))
        .find(|n| {
            n.children()
                .any(|c| c.has_tag_name("name") && c.text() == Some(system))
        })
        .map(|sys| {
            sys.children()
                .filter(|c| c.has_tag_name("command"))
                .map(|c| {
                    (
                        c.attribute("label").unwrap_or_default().to_string(),
                        c.text().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The label chosen in the gamelist: the game's own, else the system's.
fn chosen_label(gamelist: &str, game_rel: &str) -> Option<String> {
    // ES-DE writes <alternativeEmulator> beside <gameList>: two top-level elements, so
    // wrap the file (minus its XML declaration) in one root before parsing.
    let body = match gamelist.trim_start().strip_prefix("<?xml") {
        Some(rest) => rest.split_once("?>").map_or("", |(_, b)| b),
        None => gamelist,
    };
    let wrapped = format!("<romulus>{body}</romulus>");
    let doc = roxmltree::Document::parse(&wrapped).ok()?;
    let base = game_rel.rsplit('/').next().unwrap_or(game_rel);
    let wanted = [format!("./{game_rel}"), format!("./{base}")];
    let per_game = doc
        .descendants()
        .filter(|n| n.has_tag_name("game"))
        .find(|g| {
            g.children().any(|c| {
                c.has_tag_name("path")
                    && c.text()
                        .is_some_and(|t| wanted.iter().any(|w| w == t.trim()))
            })
        })
        .and_then(|g| {
            g.children()
                .find(|c| c.has_tag_name("altemulator"))
                .and_then(|c| c.text())
        })
        .map(|t| t.trim().to_string());
    per_game.or_else(|| {
        doc.descendants()
            .find(|n| n.has_tag_name("alternativeEmulator"))
            .and_then(|a| {
                a.children()
                    .find(|c| c.has_tag_name("label"))
                    .and_then(|c| c.text())
            })
            .map(|t| t.trim().to_string())
    })
}

/// The emulator for `game_rel` (path relative to the system folder) in `system`.
pub fn resolve(es: &EsDe, system: &str, game_rel: &str) -> Option<Emulator> {
    let mut cmds = es
        .custom_systems
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|x| commands(&x, system))
        .unwrap_or_default();
    if cmds.is_empty() {
        cmds = commands(&std::fs::read_to_string(&es.systems).ok()?, system);
    }
    let label = std::fs::read_to_string(es.gamelists.join(system).join("gamelist.xml"))
        .ok()
        .and_then(|g| chosen_label(&g, game_rel));
    let (l, c) = label
        .and_then(|l| cmds.iter().find(|(cl, _)| *cl == l).cloned())
        .or_else(|| cmds.first().cloned())?;
    Some(Emulator::from_command(&l, &c))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from RetroDECK 0.10.10b's es_systems.xml (docs/DECK_FINDINGS.md).
    const SYSTEMS: &str = r#"<systemList>
      <system><name>psx</name>
        <command label="SwanStation">%EMULATOR_RETROARCH% -L %CORE_RETROARCH%/swanstation_libretro.so %ROM%</command>
        <command label="Beetle PSX">%EMULATOR_RETROARCH% -L %CORE_RETROARCH%/mednafen_psx_libretro.so %ROM%</command>
        <command label="DuckStation (Standalone)">%EMULATOR_DUCKSTATION% -batch %ROM%</command>
      </system>
      <system><name>ps2</name>
        <command label="PCSX2 (Standalone)">%EMULATOR_PCSX2% -batch %ROM%</command>
      </system>
    </systemList>"#;

    fn setup(gamelist: Option<&str>) -> (tempfile::TempDir, EsDe) {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("es_systems.xml"), SYSTEMS).unwrap();
        let gl = d.path().join("gamelists");
        if let Some(g) = gamelist {
            std::fs::create_dir_all(gl.join("psx")).unwrap();
            std::fs::write(gl.join("psx/gamelist.xml"), g).unwrap();
        }
        let es = EsDe {
            systems: d.path().join("es_systems.xml"),
            custom_systems: None,
            gamelists: gl,
        };
        (d, es)
    }

    #[test]
    fn default_is_the_first_command() {
        let (_d, es) = setup(None);
        let e = resolve(&es, "psx", "Game.chd").unwrap();
        assert_eq!(
            e,
            Emulator::RetroArch {
                core: "swanstation".into()
            }
        );
        assert_eq!(e.romm_name(), "retroarch-swanstation");
        assert_eq!(resolve(&es, "ps2", "Game.iso").unwrap(), Emulator::Pcsx2);
        assert_eq!(resolve(&es, "nope", "x"), None);
    }

    #[test]
    fn system_and_game_choices_win() {
        let (_d, es) = setup(Some(
            r#"<?xml version="1.0"?><alternativeEmulator><label>Beetle PSX</label></alternativeEmulator>
            <gameList><game><path>./Metal Gear Solid (USA) (Rev 1).m3u</path><altemulator>DuckStation (Standalone)</altemulator></game></gameList>"#,
        ));
        assert_eq!(
            resolve(&es, "psx", "Other.chd").unwrap(),
            Emulator::RetroArch {
                core: "mednafen_psx".into()
            }
        );
        let mgs = resolve(
            &es,
            "psx",
            "Metal Gear Solid (USA) (Rev 1)/Metal Gear Solid (USA) (Rev 1).m3u",
        )
        .unwrap();
        assert_eq!(
            mgs,
            Emulator::Other {
                label: "DuckStation (Standalone)".into()
            }
        );
        assert_eq!(mgs.romm_name(), "duckstation");
    }
}
