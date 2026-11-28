# Flatpak

The release workflow builds `Romulus-x86_64.flatpak` from the Tauri `.deb`
(`io.github.jackhiles.romulus.yml`), on the GNOME runtime from Flathub.

Install it for your user (on a Steam Deck there is no password for a system-wide
install), in Desktop Mode's Konsole:

    flatpak install --user ~/Downloads/Romulus-x86_64.flatpak

The GNOME runtime comes from Flathub with it. Settings → Add to Steam then adds a
shortcut that runs `flatpak run io.github.jackhiles.romulus`.

Updates work like the AppImage's: Romulus downloads the new bundle, checks its SHA-256
and installs it over itself (`flatpak install --reinstall --bundle`); settings and the
library cache in `~/.var/app/io.github.jackhiles.romulus` stay. The Flatpak keeps its own
settings, so pair it with RomM again the first time.
