//! "Add Romulus to Steam": one non-Steam shortcut for the app itself (never one per game).
//!
//! Ported from KlikCore2's `launcher/steam_shortcuts.rs`. Steam keeps shortcuts in
//! `userdata/<account>/config/shortcuts.vdf`, a binary key/value file. Steam reads it when
//! it starts and may write it back from memory, so it is only edited while Steam is closed.
//! In Desktop Mode Steam is closed and restarted around the edit; in Gaming Mode (where
//! closing Steam would close Romulus too) the user is asked to do it from Desktop Mode.

use crate::app::APP_NAME;
use std::path::{Path, PathBuf};

/// A binary VDF value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Map(Vec<(String, Value)>),
    Str(String),
    Int(u32),
    U64(u64),
}

fn read_cstr(b: &[u8], pos: &mut usize) -> Option<String> {
    let end = b.get(*pos..)?.iter().position(|&c| c == 0)? + *pos;
    let s = String::from_utf8_lossy(&b[*pos..end]).into_owned();
    *pos = end + 1;
    Some(s)
}

fn read_map(b: &[u8], pos: &mut usize) -> Option<Vec<(String, Value)>> {
    let mut out = Vec::new();
    loop {
        let t = *b.get(*pos)?;
        *pos += 1;
        if t == 8 {
            return Some(out);
        }
        let key = read_cstr(b, pos)?;
        let v = match t {
            0 => Value::Map(read_map(b, pos)?),
            1 => Value::Str(read_cstr(b, pos)?),
            2 => {
                let v = u32::from_le_bytes(b.get(*pos..*pos + 4)?.try_into().ok()?);
                *pos += 4;
                Value::Int(v)
            }
            7 => {
                let v = u64::from_le_bytes(b.get(*pos..*pos + 8)?.try_into().ok()?);
                *pos += 8;
                Value::U64(v)
            }
            _ => return None,
        };
        out.push((key, v));
    }
}

pub fn parse(b: &[u8]) -> Option<Vec<(String, Value)>> {
    let mut pos = 0;
    read_map(b, &mut pos)
}

fn write_map(m: &[(String, Value)], out: &mut Vec<u8>) {
    for (k, v) in m {
        out.push(match v {
            Value::Map(_) => 0,
            Value::Str(_) => 1,
            Value::Int(_) => 2,
            Value::U64(_) => 7,
        });
        out.extend_from_slice(k.as_bytes());
        out.push(0);
        match v {
            Value::Map(m) => write_map(m, out),
            Value::Str(s) => {
                out.extend_from_slice(s.as_bytes());
                out.push(0);
            }
            Value::Int(i) => out.extend_from_slice(&i.to_le_bytes()),
            Value::U64(i) => out.extend_from_slice(&i.to_le_bytes()),
        }
    }
    out.push(8);
}

pub fn serialize(root: &[(String, Value)]) -> Vec<u8> {
    let mut out = Vec::new();
    write_map(root, &mut out);
    out
}

/// The id Steam gives a non-Steam shortcut (also the name of its artwork files).
pub fn shortcut_app_id(exe: &str, name: &str) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(exe.as_bytes());
    h.update(name.as_bytes());
    h.finalize() | 0x8000_0000
}

fn quoted(p: &Path) -> String {
    format!("\"{}\"", p.display())
}

fn str_field(m: &[(String, Value)], key: &str) -> Option<String> {
    m.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| match v {
            Value::Str(s) => Some(s.trim().to_string()),
            _ => None,
        })
}

/// Does this shortcut entry start `exe` (Steam may store it with or without quotes)?
fn launches(m: &[(String, Value)], exe: &str) -> bool {
    let e = str_field(m, "Exe").unwrap_or_default();
    e == exe || format!("\"{e}\"") == exe
}

/// Add (or update) the Romulus shortcut in a `shortcuts.vdf` image. Returns the new file
/// and the shortcut's app id.
pub fn add_to_vdf(
    existing: Option<&[u8]>,
    exe: &Path,
    icon: Option<&Path>,
) -> Result<(Vec<u8>, u32), String> {
    let mut root = match existing {
        Some(b) if !b.is_empty() => parse(b).ok_or("Steam's shortcuts file couldn't be read")?,
        _ => Vec::new(),
    };
    if root.is_empty() {
        root.push(("shortcuts".into(), Value::Map(Vec::new())));
    }
    let Some((_, Value::Map(list))) = root
        .iter_mut()
        .find(|(k, _)| k.eq_ignore_ascii_case("shortcuts"))
    else {
        return Err("Steam's shortcuts file has an unexpected layout".into());
    };
    let exe_q = quoted(exe);
    let id = shortcut_app_id(&exe_q, APP_NAME);
    let entry = vec![
        ("appid".to_string(), Value::Int(id)),
        ("appname".into(), Value::Str(APP_NAME.into())),
        ("Exe".into(), Value::Str(exe_q.clone())),
        (
            "StartDir".into(),
            Value::Str(exe.parent().map(quoted).unwrap_or_default()),
        ),
        (
            "icon".into(),
            Value::Str(icon.map(|p| p.display().to_string()).unwrap_or_default()),
        ),
        ("ShortcutPath".into(), Value::Str(String::new())),
        ("LaunchOptions".into(), Value::Str(String::new())),
        ("IsHidden".into(), Value::Int(0)),
        ("AllowDesktopConfig".into(), Value::Int(1)),
        ("AllowOverlay".into(), Value::Int(1)),
        ("OpenVR".into(), Value::Int(0)),
        ("Devkit".into(), Value::Int(0)),
        ("DevkitGameID".into(), Value::Str(String::new())),
        ("DevkitOverrideAppID".into(), Value::Int(0)),
        ("LastPlayTime".into(), Value::Int(0)),
        ("FlatpakAppID".into(), Value::Str(String::new())),
        ("tags".into(), Value::Map(Vec::new())),
    ];
    let same = list
        .iter_mut()
        .find(|(_, v)| matches!(v, Value::Map(m) if launches(m, &exe_q)));
    match same {
        Some((_, v)) => *v = Value::Map(entry),
        None => {
            let next = list
                .iter()
                .filter_map(|(k, _)| k.parse::<usize>().ok())
                .max()
                .map_or(0, |n| n + 1);
            list.push((next.to_string(), Value::Map(entry)));
        }
    }
    Ok((serialize(&root), id))
}

fn steam_roots() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    vec![
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ]
}

/// The `userdata/<account>/config` folder of the account that last signed in to Steam.
pub fn user_config_dir() -> Option<PathBuf> {
    for root in steam_roots() {
        let userdata = root.join("userdata");
        // loginusers.vdf: "<SteamID64>" { ... "MostRecent" "1" ... }
        let mut recent = None;
        if let Ok(text) = std::fs::read_to_string(root.join("config").join("loginusers.vdf")) {
            let mut current: Option<u64> = None;
            for line in text.lines() {
                let parts: Vec<&str> = line.split('"').collect();
                if parts.len() == 3 && parts[1].len() == 17 {
                    current = parts[1].parse().ok();
                } else if parts.len() >= 5
                    && parts[1].eq_ignore_ascii_case("mostrecent")
                    && parts[3] == "1"
                {
                    recent = current;
                }
            }
        }
        if let Some(id64) = recent {
            let dir = userdata
                .join((id64.wrapping_sub(76_561_197_960_265_728) as u32).to_string())
                .join("config");
            if dir.is_dir() {
                return Some(dir);
            }
        }
        let newest = std::fs::read_dir(&userdata)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .chars()
                    .all(|c| c.is_ascii_digit())
                    && e.file_name() != "0"
            })
            .filter_map(|e| {
                Some((
                    e.path().join("config").metadata().ok()?.modified().ok()?,
                    e.path().join("config"),
                ))
            })
            .max_by_key(|(t, _)| *t);
        if let Some((_, dir)) = newest {
            return Some(dir);
        }
    }
    None
}

/// Is there already a shortcut that starts this copy of Romulus?
pub fn has_shortcut() -> bool {
    let Ok(exe) = updater::program_path() else {
        return false;
    };
    let Some(root) = user_config_dir()
        .and_then(|d| std::fs::read(d.join("shortcuts.vdf")).ok())
        .and_then(|b| parse(&b))
    else {
        return false;
    };
    let exe_q = quoted(&exe);
    root.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("shortcuts")
            && matches!(v, Value::Map(list) if list.iter().any(|(_, e)| matches!(e, Value::Map(m) if launches(m, &exe_q))))
    })
}

pub fn steam_running() -> bool {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            std::fs::read_to_string(e.path().join("comm"))
                .is_ok_and(|c| matches!(c.trim(), "steam" | "steamwebhelper"))
        })
}

/// Running inside Steam's Gaming Mode (where closing Steam would close Romulus too).
pub fn in_gaming_mode() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.eq_ignore_ascii_case("gamescope"))
        || std::env::var_os("SteamGamepadUI").is_some()
}

/// Ask Steam to close and wait (up to ~40 s) until it has.
fn shutdown_steam() -> bool {
    let _ = std::process::Command::new("steam").arg("-shutdown").spawn();
    for _ in 0..80 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if !steam_running() {
            // Give it a moment to finish writing its files.
            std::thread::sleep(std::time::Duration::from_secs(1));
            return true;
        }
    }
    false
}

fn start_steam() {
    let _ = std::process::Command::new("steam")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

const ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");

/// Add Romulus to Steam (blocking: may wait for Steam to close). `data_dir` holds the icon
/// file the shortcut points to. Returns a message for the user.
pub fn add_romulus(data_dir: &Path) -> Result<String, String> {
    let exe = updater::program_path().map_err(|e| e.to_string())?;
    if updater::is_dev_build() {
        return Err("development builds can't be added to Steam; use the AppImage".into());
    }
    if in_gaming_mode() {
        return Err("switch to Desktop Mode and add Romulus from there (Steam has to be closed while its shortcut list is edited)".into());
    }
    let config = user_config_dir().ok_or("no Steam account found on this device")?;
    let icon = data_dir.join("steam-icon.png");
    let icon = std::fs::write(&icon, ICON_PNG).ok().map(|_| icon);

    let restart = steam_running();
    if restart && !shutdown_steam() {
        return Err("Steam didn't close; close it and try again".into());
    }
    let result = (|| {
        let file = config.join("shortcuts.vdf");
        let existing = std::fs::read(&file).ok();
        let (bytes, _id) = add_to_vdf(existing.as_deref(), &exe, icon.as_deref())?;
        if let Some(old) = &existing {
            // Keep the user's original list once, before we ever touched it.
            let backup = config.join("shortcuts.vdf.romulus-backup");
            if !backup.exists() {
                std::fs::write(&backup, old).map_err(|e| e.to_string())?;
            }
        }
        let tmp = config.join("shortcuts.vdf.romulus-tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &file).map_err(|e| e.to_string())
    })();
    if restart {
        start_steam();
    }
    result?;
    tracing::info!("added {} to Steam", exe.display());
    Ok(format!(
        "{APP_NAME} was added to Steam. You'll find it under Non-Steam in your library."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_idempotent_add() {
        let other = vec![(
            "shortcuts".to_string(),
            Value::Map(vec![(
                "0".into(),
                Value::Map(vec![
                    ("appname".into(), Value::Str("Other".into())),
                    ("Exe".into(), Value::Str("\"/usr/bin/other\"".into())),
                    ("LastPlayTime".into(), Value::Int(5)),
                ]),
            )]),
        )];
        let bytes = serialize(&other);
        assert_eq!(parse(&bytes).unwrap(), other);

        let exe = Path::new("/home/deck/Applications/Romulus-x86_64.AppImage");
        let (once, id) = add_to_vdf(Some(&bytes), exe, None).unwrap();
        let (twice, id2) = add_to_vdf(Some(&once), exe, None).unwrap();
        assert_eq!(id, id2);
        assert_eq!(
            once, twice,
            "adding again replaces the entry instead of duplicating it"
        );
        let Value::Map(list) = &parse(&twice).unwrap()[0].1 else {
            panic!()
        };
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].0, "1");
        assert!(id & 0x8000_0000 != 0);
    }

    #[test]
    fn empty_file_gets_a_list() {
        let (bytes, _) = add_to_vdf(None, Path::new("/x/Romulus.AppImage"), None).unwrap();
        let root = parse(&bytes).unwrap();
        assert_eq!(root[0].0, "shortcuts");
    }
}
