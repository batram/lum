# Lum — Feature Plan (v1)

Windows 10 tray app that fades screen warmth & brightness with the sun.

## Tech stack

* **Backend**: Rust + Tauri 2.x (`windows-rs` for WinAPI calls)
* **Frontend**: Svelte (SVG/canvas for curve editor & map picker)
* **Distribution**: Portable exe (no installer)
* **Settings**: Human-readable JSON in `%AppData%\Lum\settings.json` with a `"version": 1` field for forward-compatible schema migration

## Rendering engines (user-selectable in Settings)

Two mutually exclusive engines; only one active at a time.

### 1. Gamma ramps (default)

`SetDeviceGammaRamp` reprograms the GPU hardware LUT directly —
no overlay, no window, ~zero performance cost, instant, per-monitor,
fully smooth fades.

Caveats handled by app:

* LUT resets on reboot / driver reset → reapply on launch & resume-from-sleep
* Conflicts with native Night Light → detect and prompt user to disable it
* Exclusive-fullscreen apps can grab the LUT → per-app pause list

### 2. Night Light (registry-based)

Controls the built-in Windows Night Light via `CloudStore` registry keys
(`HKCU\Software\Microsoft\Windows\CurrentVersion\CloudStore\Store\...`).

* Enable/disable + color temperature + schedule written to registry
* Requires Night Light to be available (not disabled by GPO)
* Less granular than gamma ramps (no per-monitor, no brightness control)
* Useful on machines where gamma ramps are reset by other software

### Engine switching & fallback

* Settings toggle: "Rendering engine: Gamma ramps | Night Light"
* On switch, previous engine is cleanly reset (identity LUT / NL disabled)
* If gamma-ramp write fails repeatedly → suggest Night Light as fallback
* On first run, detect if Night Light is active → prompt to disable (gamma mode) or offer to keep it (NL mode)

## Core features (v1 scope)

1. **Sun-based scheduling**

   * Location via **rudimentary world map picker** in the UI
     (click → lat/long). Manual lat/long text input as fallback.
     No Windows location API.

   * Local sunrise/sunset computation (no runtime network dependency)

   * Configurable offsets + fade duration (or civil twilight)

2. **Fade engine with custom curves**

   * **Separate curves for evening (sunset) and morning (sunrise)**

   * Curve maps transition progress (0→1) → intensity (0=day, 1=night)

   * Bezier curve editor + presets (linear, ease-in-out, ease-out, stepped)

   * Scrub preview: drag along time axis → effect applied live

   * Drives color temp (Kelvin endpoints, e.g. 6500K→3400K) AND brightness
     (linked or separate curves)

   * **Tick rate**: engine recalculates every 1 second with interpolation
     between ticks for smooth visual transitions

3. **DDC/CI monitor brightness**

   * VCP 0x10 brightness / 0x12 contrast per physical monitor

   * Multi-monitor: individual or ganged control

   * Software-dim overlay fallback for non-DDC/CI displays

   * **Failure handling**: per-monitor capability probe on startup;
     if DDC/CI times out or returns errors → auto-fallback to software
     overlay for that monitor; retry DDC/CI on next app launch

   * **Error surfacing**: failed monitors shown in a "Status / Issues"
     section in Settings with per-monitor status badges

4. **Dark/light theme switch**

   * `AppsUseLightTheme` + `SystemUsesLightTheme` registry + broadcast

   * Bindable to schedule and hotkeys

   * **Theme sync indicator**: tray/UI shows live theme state

5. **System tray**

   * Lives in tray; window close = hide

   * Menu: state, pause, day/night jump, theme toggle, **boost**, settings, quit

   * Tooltip with full state summary

6. **Global hotkeys**

   * Capture-style binding UI

   * Actions: dim down, dim up, pause toggle, theme toggle, day/night jump,
     **boost** (temporary full-bright ~2 min, then resume fade)

7. **Per-application pause list**

   * Fade/brightness effects suspended while a listed app is focused
     (e.g. Photoshop, games, video players)

8. **User settings panel**

   * Everything is user-controllable and changeable

   * Rendering engine toggle (gamma ramps vs. Night Light)

   * Per-monitor status & capability display

9. **Auto-start with Windows**

   * Portable: registers via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
     (no admin required) or Startup folder shortcut

   * Also required to restore gamma LUT after reboot

10. **Conflict detection**

    * On startup, detect known gamma-controlling processes
      (f.lux, Iris, LightBulb, etc.) → warn user of potential LUT conflicts

    * Offer to add them to the per-app pause list as a quick action

11. **Portable cleanup behavior**

    * On graceful quit → reset gamma to identity LUT / disable Night Light
    * Abrupt removal (delete folder) may leave warm tint until reboot or
      manual Night Light toggle — documented in-app
    * "Reset all settings" menu item clears JSON + registry Run key

## Additional v1 details (from review pass)

* **Reapply gamma ramps after sleep/lock/resume** (drivers may reset LUT)

* **Native Night Light detection** on first run → prompt to disable

* **First-run wizard**: map pick, NL check, quick fade test

* **Manual override policy**: if user changes brightness/theme outside
  the app, manual input wins for a configurable period

* **Per-monitor mapping UI**: which displays get gamma, which get DDC/CI

* **Brightness floor/ceiling limits** (never fully dark)

## Not in v1

See TODO.md (pause-for-X, fullscreen detect, presets, multi-schedule,
split brightness timing, export/import, Win11, HDR, i18n).

## Resolved decisions

* [x] Frontend stack: **Svelte** (SVG/canvas for curve editor + map picker)
* [x] Distribution: **Portable exe** (no installer, no admin required)
* [x] Settings: **JSON in `%AppData%\Lum\settings.json`** with schema version field
* [x] Night Light: **Full switchable engine** alongside gamma ramps
* [ ] Rust toolchain not yet installed on this machine (cargo missing)

