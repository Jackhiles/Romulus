//! A clean environment for programs started on the host (flatpak, Steam, xdg-open).
//!
//! Inside a Flatpak, host programs go through `flatpak-spawn --host`, which starts them
//! outside the sandbox with only the variables passed along ([`FORWARD`]).
//!
//! An AppImage runs with variables pointing into its own mount: `LD_LIBRARY_PATH` with
//! its bundled GLib, `PATH`, `PYTHONHOME`, GTK and GIO module paths, and so on. A host
//! program inheriting them loads the wrong libraries; on the Deck `flatpak` failed with
//! "symbol lookup error ... g_once_init_leave_pointer". Every path into an AppImage
//! mount is dropped; what is left (Steam's variables, the display, D-Bus) is kept.

use std::ffi::OsString;
use std::process::Command;

/// Variables only the AppImage itself should see.
const APPIMAGE_ONLY: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "OWD",
    "ARGV0",
    "WEBKIT_DISABLE_DMABUF_RENDERER",
];

fn is_mount_path(p: &str) -> bool {
    p.contains("/.mount_")
}

/// The environment `vars` with AppImage paths removed. `None` means unset the variable.
pub fn cleaned<I>(vars: I) -> Vec<(String, Option<String>)>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut out = Vec::new();
    for (k, v) in vars {
        if APPIMAGE_ONLY.contains(&k.as_str()) {
            out.push((k, None));
        } else if is_mount_path(&v) {
            let kept: Vec<&str> = v
                .split(':')
                .filter(|p| !p.is_empty() && !is_mount_path(p))
                .collect();
            out.push((k, (!kept.is_empty()).then(|| kept.join(":"))));
        }
    }
    out
}

/// Apply [`cleaned`] for this process's environment to `cmd`.
pub fn clean(cmd: &mut Command) -> &mut Command {
    let vars = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
    for (k, v) in cleaned(vars) {
        match v {
            Some(v) => cmd.env(&k, OsString::from(v)),
            None => cmd.env_remove(&k),
        };
    }
    cmd
}

/// Is this process running in a Flatpak sandbox?
pub fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// Variables a host program started from the Flatpak needs: the display, and Steam's
/// game variables and overlay, so Gaming Mode shows the emulator's window as this
/// shortcut's game.
pub const FORWARD: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "GAMESCOPE_WAYLAND_DISPLAY",
    "SteamGameId",
    "SteamAppId",
    "SteamOverlayGameId",
    "SteamClientLaunch",
    "SteamEnv",
    "SteamGamepadUI",
    "SteamDeck",
    "STEAM_COMPAT_CLIENT_INSTALL_PATH",
    "LD_PRELOAD",
    "ENABLE_GAMESCOPE_WSI",
];

/// `flatpak-spawn --host` arguments that pass `vars` (name, value) along.
pub fn spawn_args<I>(vars: I) -> Vec<String>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut args = vec!["--host".to_string()];
    for (k, v) in vars {
        if FORWARD.contains(&k.as_str()) {
            args.push(format!("--env={k}={v}"));
        }
    }
    args
}

/// A [`Command`] for a host program, with the AppImage's paths removed (or, in a
/// Flatpak, started on the host).
pub fn host_command(program: &str) -> Command {
    if in_flatpak() {
        let vars = std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
        let mut c = Command::new("flatpak-spawn");
        c.args(spawn_args(vars)).arg(program);
        return c;
    }
    let mut c = Command::new(program);
    clean(&mut c);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpak_passes_only_the_display_and_steams_variables() {
        let args = spawn_args(vec![
            ("DISPLAY".to_string(), ":0".to_string()),
            ("SteamGameId".to_string(), "3786571840".to_string()),
            ("PATH".to_string(), "/app/bin:/usr/bin".to_string()),
            (
                "XDG_DATA_HOME".to_string(),
                "/home/deck/.var/app/x/data".to_string(),
            ),
        ]);
        assert_eq!(
            args,
            ["--host", "--env=DISPLAY=:0", "--env=SteamGameId=3786571840"]
        );
    }

    #[test]
    fn drops_appimage_paths_and_keeps_the_rest() {
        let m = "/tmp/.mount_RomuluAbC";
        let env = vec![
            (
                "LD_LIBRARY_PATH".to_string(),
                format!("{m}/usr/lib/:{m}/usr/lib/x86_64-linux-gnu/"),
            ),
            (
                "PATH".to_string(),
                format!("{m}/usr/bin/:/usr/local/bin:/usr/bin"),
            ),
            (
                "XDG_DATA_DIRS".to_string(),
                format!("{m}/usr/share/:/usr/share:/tmp/.mount_RomuluOld/usr/share"),
            ),
            ("PYTHONHOME".to_string(), format!("{m}/usr/")),
            ("APPDIR".to_string(), m.to_string()),
            (
                "WEBKIT_DISABLE_DMABUF_RENDERER".to_string(),
                "1".to_string(),
            ),
            (
                "LD_PRELOAD".to_string(),
                "/home/deck/.local/share/Steam/steamrt64/gameoverlayrenderer.so".to_string(),
            ),
            ("DISPLAY".to_string(), ":1".to_string()),
        ];
        let mut c = cleaned(env);
        c.sort();
        assert_eq!(
            c,
            vec![
                ("APPDIR".into(), None),
                ("LD_LIBRARY_PATH".into(), None),
                ("PATH".into(), Some("/usr/local/bin:/usr/bin".into())),
                ("PYTHONHOME".into(), None),
                ("WEBKIT_DISABLE_DMABUF_RENDERER".into(), None),
                ("XDG_DATA_DIRS".into(), Some("/usr/share".into())),
            ]
        );
    }
}
