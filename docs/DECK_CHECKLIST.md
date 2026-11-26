# Steam Deck checklist (Phase 1 go/no-go)

Everything below was tested in a Linux container (WebKitGTK 2.52 under Xvfb, software
rendering, a mock RomM server). These checks need a real Deck. Please report the result of
each, and use Settings → Copy debug info for anything that fails.

## Install and run

- [ ] The AppImage starts in Desktop Mode (double-click in Dolphin).
- [ ] Pairing: the QR code opens RomM's approval page on your phone and approving signs
      the Deck in. (Your server's web UI must have the `/pair/device` page, which is in
      RomM 5.x.)
- [ ] Settings → Account says "credentials in the system keyring" in Desktop Mode. Note
      what it says in Gaming Mode after a restart.
- [ ] Settings → Add to Steam closes and restarts Steam, and Romulus appears under
      Non-Steam.

## Gaming Mode

- [ ] Romulus opens fullscreen with no window frame.
- [ ] **Controller**: D-pad, left stick, A/B/X/Y, bumpers, triggers and ☰ all work with the
      default Steam Input layout. If nothing responds: Settings → Controller input →
      "Browser Gamepad API" (using the touchscreen), and report which one works.
- [ ] Holding a direction scrolls smoothly at a steady pace (no jumps or stalls).
- [ ] Covers fade in while scrolling a large platform, with no visible stutter.
- [ ] Search: A on the field opens Steam's on-screen keyboard. If it doesn't, Steam + X
      should.
- [ ] Startup to the cached library takes about a second or less.

## Rendering

Romulus sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` by default, because it avoids blank windows
with AppImages on some systems. It may cost performance on the Deck's AMD GPU. To compare,
start once from Konsole with the faster renderer:

```sh
ROMULUS_DMABUF=1 ~/Applications/Romulus-x86_64.AppImage
```

- [ ] Note whether scrolling feels better or worse, and whether the window is blank.

## RetroDECK on the SD card (for Phase 2)

Please paste the output of:

```sh
ls ~/.var/app/net.retrodeck.retrodeck/config/retrodeck/
grep -E '^(rdhome|roms_folder|bios_folder|saves_folder|states_folder)' ~/.var/app/net.retrodeck.retrodeck/config/retrodeck/retrodeck.cfg
cat ~/.var/app/net.retrodeck.retrodeck/config/retrodeck/retrodeck.json 2>/dev/null | head -40
ls /run/media/
```
