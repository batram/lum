<script>
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import MapPicker from "$lib/MapPicker.svelte";
  import Curves from "$lib/Curves.svelte";

  const sections = [
    ["overview", "⌂", "Overview"],
    ["schedule", "◴", "Schedule"],
    ["exceptions", "⊘", "Exceptions"],
    ["hotkeys", "⌨", "Hotkeys"],
    ["general", "⚙", "General"],
    ["developer", "◇", "Developer"],
  ];

  const hotkeyRows = [
    ["toggle_pause", "Toggle pause", "Pause or resume Lum’s scheduled effects."],
    ["brighter", "Brighter", "Increase current brightness by 5%."],
    ["darker", "Darker", "Decrease current brightness by 5%."],
    ["toggle_theme", "Toggle Windows theme", "Switch between Windows light and dark themes."],
    ["toggle_day_night", "Toggle day / night", "Switch directly between Lum’s day and night appearance."],
    ["boost", "Boost", "Restore full, neutral brightness."],
  ];

  const defaultHotkeys = {
    toggle_pause: "Alt+Pause", brighter: "Alt+Up", darker: "Alt+Down",
    toggle_theme: "Alt+F5", toggle_day_night: "Alt+F6", boost: "Alt+F7",
  };

  let active = $state("overview");
  let settings = $state(null);
  let state = $state(null);
  let monitors = $state([]);
  let autostart = $state(false);
  let saveStatus = $state("Loading…");
  let loadError = $state("");
  let appError = $state("");
  let newApp = $state("");
  let editingLocation = $state(false);
  let ready = $state(false);
  let lastSaved = "";
  let saveTimer;

  onMount(() => {
    let disposed = false;
    (async () => {
      try {
        const [loadedSettings, liveState, monitorResult, autostartResult] = await Promise.all([
          invoke("get_settings"), invoke("get_app_state"),
          invoke("get_monitors").catch(() => []), invoke("get_autostart").catch(() => false),
        ]);
        if (disposed) return;
        settings = loadedSettings;
        state = liveState;
        monitors = monitorResult;
        autostart = autostartResult;
        lastSaved = JSON.stringify(loadedSettings);
        ready = true;
        saveStatus = "Saved";
      } catch (reason) {
        loadError = `Lum could not load settings: ${reason}`;
      }
    })();
    const stateInterval = setInterval(async () => {
      try { state = await invoke("get_app_state"); } catch { /* status is supplementary */ }
    }, 1000);
    return () => {
      disposed = true;
      clearInterval(stateInterval); clearTimeout(saveTimer);
    };
  });

  $effect(() => {
    if (!ready || !settings) return;
    const serialized = JSON.stringify(settings);
    if (serialized === lastSaved) return;
    clearTimeout(saveTimer);
    saveStatus = "Saving…";
    saveTimer = setTimeout(async () => {
      try {
        await invoke("save_settings", { settings: JSON.parse(serialized) });
        lastSaved = serialized;
        saveStatus = "Saved";
      } catch (reason) {
        saveStatus = `Couldn’t save · ${reason}`;
      }
    }, 420);
  });

  function addApp() {
    appError = "";
    let value = newApp.trim().toLowerCase();
    if (!value) return;
    if (!value.endsWith(".exe")) value += ".exe";
    if (!/^[^\\/:*?"<>|]+\.exe$/.test(value)) { appError = "Enter a valid Windows executable name."; return; }
    if (settings.pause_apps.some((item) => item.toLowerCase() === value)) { appError = "That application is already listed."; return; }
    settings.pause_apps.push(value);
    newApp = "";
  }

  function removeApp(value) {
    settings.pause_apps = settings.pause_apps.filter((item) => item !== value);
  }

  async function toggleAutostart() {
    autostart = await invoke("toggle_autostart");
  }

  function pickLocation(latitude, longitude) {
    settings.location.latitude = latitude;
    settings.location.longitude = longitude;
  }

  function captureHotkey(event, field) {
    if (event.key === "Tab") return;
    if (event.key === "Escape") { event.currentTarget.blur(); return; }
    event.preventDefault();
    if (event.key === "Backspace" || event.key === "Delete") {
      settings.hotkeys[field] = "";
      return;
    }
    if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) return;
    const modifiers = [];
    if (event.ctrlKey) modifiers.push("Ctrl");
    if (event.altKey) modifiers.push("Alt");
    if (event.shiftKey) modifiers.push("Shift");
    if (event.metaKey) modifiers.push("Win");
    const numpadKey = event.code.startsWith("Numpad") && event.code !== "NumpadEnter" ? event.code : null;
    if (!modifiers.length && !numpadKey) return;
    const aliases = {
      ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
      " ": "Space", PageUp: "PageUp", PageDown: "PageDown",
    };
    const key = numpadKey ?? aliases[event.key] ?? (event.key.length === 1 ? event.key.toUpperCase() : event.key);
    settings.hotkeys[field] = [...modifiers, key].join("+");
  }

  function navigate(section) { active = section; }
  function phaseName(value) { return value ? value[0].toUpperCase() + value.slice(1) : "Current"; }
  let needsAttention = $derived(settings && (Math.abs(settings.location.latitude - 40.7128) < .0001 && Math.abs(settings.location.longitude + 74.006) < .0001));
</script>

<svelte:head><title>Lum Settings</title></svelte:head>

<div class="app-shell">
  <aside>
    <div class="logo"><span>◒</span><strong>Lum</strong></div>
    <nav aria-label="Settings sections">
      {#each sections as item}
        <button class:active={active === item[0]} type="button" onclick={() => navigate(item[0])}>
          <span aria-hidden="true">{item[1]}</span>{item[2]}
        </button>
      {/each}
    </nav>
    <div class="save-state" class:error={saveStatus.startsWith("Couldn’t")}><i></i>{saveStatus}</div>
  </aside>

  <main>
    {#if loadError}
      <div class="notice danger" role="alert"><strong>Settings unavailable</strong><span>{loadError}</span></div>
    {:else if !settings}
      <div class="loading">Loading Lum…</div>
    {:else}
      {#if active === "overview"}
        <header><p>Settings</p><h1>Overview</h1><span>Your display, at a glance.</span></header>
        {#if needsAttention}
          <button class="setup-banner" type="button" onclick={() => { active = "general"; editingLocation = true; }}>
            <span class="setup-icon">⌖</span><span><strong>Confirm your location</strong><small>Lum is using the default location. Set yours for accurate sunrise and sunset times.</small></span><b>Set up →</b>
          </button>
        {/if}
        <section class="status-hero">
          <div class="status-orb" class:night={(state?.intensity ?? 0) > .45}></div>
          <div><span class="eyebrow">Right now</span><h2>{phaseName(state?.phase)} light</h2><p>{state?.automatic ? `${state?.next_transition_label} at ${state?.next_transition_time}` : "Holding your current appearance"}</p></div>
          <div class="live-values"><span><b>{state?.hardware_brightness_pct ?? "—"}%</b>Hardware</span><span><b>{state?.overlay_brightness_pct ?? "—"}%</b>Overlay</span><span><b>{state?.color_temp_k ?? "—"}K</b>Temperature</span></div>
        </section>
        <div class="overview-grid">
          <button class="summary-card" type="button" onclick={() => navigate("schedule")}><span class="card-icon blue">◴</span><span><strong>Sun schedule</strong><small>Sunrise {state?.sunrise} · Sunset {state?.sunset}</small></span><b>›</b></button>
          <button class="summary-card" type="button" onclick={() => navigate("schedule")}><span class="card-icon violet">▣</span><span><strong>{monitors.filter((monitor) => monitor.supports_brightness).length} of {monitors.length} displays</strong><small>Hardware brightness capability · configure in Schedule</small></span><b>›</b></button>
          <button class="summary-card" type="button" onclick={() => navigate("exceptions")}><span class="card-icon amber">⊘</span><span><strong>App exceptions</strong><small>{settings.pause_apps.length ? `${settings.pause_apps.length} configured` : "No apps configured"}</small></span><b>›</b></button>
        </div>
      {:else if active === "schedule"}
        <header><p>Settings</p><h1>Schedule</h1><span>Shape how your displays change through the day.</span></header>
        <Curves {settings} {monitors} />
        <section class="card">
          <div class="card-heading"><div><h2>Transition timing</h2><p>Fine-tune when gradual changes begin and end.</p></div></div>
          <div class="field-grid">
            <label><span>Evening offset</span><div class="number-field"><input type="number" min="-240" max="240" bind:value={settings.fade.evening_offset_min} /><em>min before sunset</em></div></label>
            <label><span>Morning offset</span><div class="number-field"><input type="number" min="-240" max="240" bind:value={settings.fade.morning_offset_min} /><em>min after sunrise</em></div></label>
            <label><span>Fade duration</span><div class="number-field"><input type="number" min="5" max="240" bind:value={settings.fade.fade_duration_min} /><em>minutes</em></div></label>
          </div>
          <label class="toggle-row"><span><strong>Use civil twilight</strong><small>Anchor display transitions and theme markers to civil dawn and dusk.</small></span><input type="checkbox" bind:checked={settings.fade.use_civil_twilight} /></label>
          <label class="toggle-row"><span><strong>Switch Windows theme automatically</strong><small>Drag the Light and Dark markers in the chart to adjust their solar offsets.</small></span><input type="checkbox" bind:checked={settings.theme.auto_switch} /></label>
        </section>
      {:else if active === "exceptions"}
        <header><p>Settings</p><h1>Exceptions</h1><span>Temporarily restore neutral color for color-sensitive work.</span></header>
        <section class="card">
          <div class="card-heading"><div><h2>Pause for applications</h2><p>Effects turn off while a listed application is focused and return automatically afterward.</p></div></div>
          <form class="add-app" onsubmit={(event) => { event.preventDefault(); addApp(); }}><input aria-label="Application executable" placeholder="Application name, e.g. photoshop.exe" bind:value={newApp} /><button type="submit">Add application</button></form>
          {#if appError}<p class="inline-error" role="alert">{appError}</p>{/if}
          <div class="app-list">
            {#each settings.pause_apps as app}
              <div><span class="app-icon">◇</span><strong>{app}</strong><button type="button" aria-label={`Remove ${app}`} onclick={() => removeApp(app)}>Remove</button></div>
            {:else}<div class="empty compact"><strong>No exceptions yet</strong><p>Add an application when accurate, neutral color is more important than the schedule.</p></div>{/each}
          </div>
        </section>
      {:else if active === "hotkeys"}
        <header><p>Settings</p><h1>Hotkeys</h1><span>Control Lum from anywhere with global keyboard shortcuts.</span></header>
        <section class="card">
          <div class="card-heading"><div><h2>Global shortcuts</h2><p>Click a field and press a shortcut. Backspace clears it; empty shortcuts are disabled.</p></div><button class="secondary" type="button" onclick={() => settings.hotkeys = { ...defaultHotkeys }}>Restore defaults</button></div>
          <div class="hotkey-list">
            {#each hotkeyRows as item}
              <label>
                <span><strong>{item[1]}</strong><small>{item[2]}</small></span>
                <input aria-label={`${item[1]} shortcut`} placeholder="Disabled" value={settings.hotkeys[item[0]]} onkeydown={(event) => captureHotkey(event, item[0])} oninput={(event) => settings.hotkeys[item[0]] = event.currentTarget.value} />
              </label>
            {/each}
          </div>
          <p class="hotkey-note">Numpad keys can be used alone. Other shortcuts must include Ctrl, Alt, Shift, or Win. Changes apply immediately after saving.</p>
        </section>
      {:else if active === "general"}
        <header><p>Settings</p><h1>General</h1><span>Location, Windows integration, and startup behavior.</span></header>
        <section class="card">
          <div class="card-heading"><div><h2>Location</h2><p>Lum uses coordinates only to calculate local solar times.</p></div><button class="secondary" type="button" onclick={() => editingLocation = !editingLocation}>{editingLocation ? "Done" : "Change location"}</button></div>
          <div class="location-summary"><span>⌖</span><div><strong>{settings.location.latitude.toFixed(2)}°, {settings.location.longitude.toFixed(2)}°</strong><small>Sunrise {state?.sunrise} · Sunset {state?.sunset}</small></div></div>
          {#if editingLocation}<div class="map-panel"><MapPicker lat={settings.location.latitude} lng={settings.location.longitude} onPick={pickLocation} /></div>{/if}
          <p class="map-credit">Map by Natural Earth: <a href="https://www.naturalearthdata.com/" onclick={(event) => { event.preventDefault(); openUrl(event.currentTarget.href); }}>naturalearthdata.com</a></p>
        </section>
        <section class="card rows">
          <label class="toggle-row"><span><strong>Start Lum with Windows</strong><small>Launch quietly in the notification area after sign-in.</small></span><input type="checkbox" checked={autostart} onchange={toggleAutostart} /></label>
        </section>
      {:else if active === "developer"}
        <header><p>Settings</p><h1>Developer</h1><span>Advanced quick-panel behavior and interaction experiments.</span></header>
        <section class="card">
          <div class="card-heading"><div><h2>Tray interaction</h2><p>Choose how Lum balances popup speed with double-click actions.</p></div></div>
          <label class="select-row">
            <span><strong>Tray click behavior</strong><small>
              {settings.developer.tray_click_behavior === "windows_timed"
                ? "Waits for the Windows double-click interval, avoiding popup flashes."
                : settings.developer.tray_click_behavior === "immediate_with_settings"
                  ? "Opens immediately; a double-click switches to Settings and may briefly show the popup."
                  : "Opens immediately; double-click has no separate action."}
            </small></span>
            <select bind:value={settings.developer.tray_click_behavior} aria-label="Tray click behavior">
              <option value="immediate">Immediate popup</option>
              <option value="immediate_with_settings">Immediate + double-click Settings</option>
              <option value="windows_timed">Windows double-click timing</option>
            </select>
          </label>
          <label class="toggle-row"><span><strong>Close popup when focus is lost</strong><small>Turn this off to keep the quick panel open while using other windows.</small></span><input type="checkbox" bind:checked={settings.developer.close_on_focus_loss} /></label>
        </section>
      {/if}
    {/if}
  </main>
</div>

<style>
  :global(*) { box-sizing: border-box; }
  :global(html) { color-scheme: dark; background: #171820; }
  :global(body) { margin: 0; font-family: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif; color: #eef0f5; background: #171820; }
  :global(button), :global(input) { font: inherit; }
  :global(button) { color: inherit; }
  .app-shell { display: grid; grid-template-columns: 220px minmax(0,1fr); min-height: 100vh; }
  aside { position: sticky; top: 0; display: flex; flex-direction: column; height: 100vh; padding: 24px 14px 16px; border-right: 1px solid #2d303a; background: #1d1e27; }
  .logo { display: flex; align-items: center; gap: 10px; padding: 0 12px 27px; font-size: 17px; }
  .logo span { color: #ffc45c; font-size: 26px; transform: rotate(-16deg); }
  nav { display: grid; gap: 3px; }
  nav button { display: flex; align-items: center; gap: 12px; width: 100%; padding: 10px 12px; border: 0; border-radius: 9px; background: transparent; color: #a7abb7; text-align: left; cursor: pointer; font-size: 13px; }
  nav button span { width: 18px; color: #858b99; font-size: 16px; text-align: center; }
  nav button:hover { background: #252732; color: #f0f1f5; }
  nav button.active { background: #2c3040; color: white; }
  nav button.active span { color: #8cb4ff; }
  .save-state { display: flex; align-items: center; gap: 7px; margin: auto 12px 0; color: #7f8593; font-size: 11px; }
  .save-state i { width: 6px; height: 6px; border-radius: 50%; background: #67c796; }
  .save-state.error { color: #ffaaa3; }.save-state.error i { background: #ef7770; }
  main { width: 100%; max-width: 1040px; padding: 42px clamp(28px,5vw,64px) 72px; }
  header { margin-bottom: 25px; }
  header p { margin: 0 0 7px; color: #7e9edb; font-size: 11px; font-weight: 650; letter-spacing: .08em; text-transform: uppercase; }
  h1 { margin: 0 0 7px; font-size: 28px; line-height: 1.15; letter-spacing: -.035em; }
  header > span { color: #9297a4; font-size: 13px; }
  h2 { margin: 0; font-size: 15px; letter-spacing: -.01em; }
  .setup-banner { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 14px; width: 100%; margin-bottom: 16px; padding: 15px; border: 1px solid rgba(226,172,80,.26); border-radius: 12px; background: rgba(226,172,80,.075); text-align: left; cursor: pointer; }
  .setup-banner > span:nth-child(2) { display: grid; gap: 3px; }.setup-banner small { color: #a7a18f; }.setup-banner b { color: #efbd68; font-size: 12px; }.setup-icon { font-size: 20px; color: #efbd68; }
  .status-hero { display: grid; grid-template-columns: auto 1fr auto; gap: 17px; align-items: center; padding: 22px; border: 1px solid #323540; border-radius: 15px; background: linear-gradient(145deg,#242630,#20212a); }
  .status-orb { width: 52px; height: 52px; border-radius: 50%; background: radial-gradient(circle at 34% 33%,#fff0b8,#e8a746 65%,#9d6324); box-shadow: 0 0 30px rgba(234,170,68,.13); }.status-orb.night { background: radial-gradient(circle at 34% 33%,#f8e6ab,#dca348 60%,#272934 62%); }
  .eyebrow { color: #858b99; font-size: 10px; text-transform: uppercase; letter-spacing: .08em; }.status-hero h2 { margin: 3px 0 5px; font-size: 20px; }.status-hero p { margin: 0; color: #969ba8; font-size: 12px; }
  .live-values { display: flex; gap: 24px; }.live-values span { display: grid; gap: 4px; color: #7f8491; font-size: 10px; }.live-values b { color: #eef0f5; font-size: 16px; font-variant-numeric: tabular-nums; }
  .overview-grid { display: grid; gap: 9px; margin-top: 14px; }
  .summary-card { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 13px; padding: 14px 16px; border: 1px solid #2d303a; border-radius: 12px; background: #20222b; text-align: left; cursor: pointer; }.summary-card:hover { border-color: #414653; background: #232630; }.summary-card > span:nth-child(2) { display:grid;gap:3px;}.summary-card small { color:#858b98; }.summary-card > b { color:#777e8d;font-size:20px; }
  .card-icon { display:grid;place-items:center;width:34px;height:34px;border-radius:9px;background:#26354a;color:#8eb8ff;}.card-icon.violet{background:#322d47;color:#b9a6f5}.card-icon.amber{background:#3a3025;color:#efb96b}
  .card { margin-top: 14px; padding: 20px; border: 1px solid #30333e; border-radius: 14px; background: #20222b; }
  .card-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 19px; }.card-heading p { margin: 5px 0 0; color: #858b98; font-size: 12px; }
  .field-grid { display:grid;grid-template-columns:repeat(3,1fr);gap:12px; }.field-grid > label { display:grid;gap:7px;color:#aeb3bf;font-size:11px; }.number-field { display:flex;align-items:center;border:1px solid #3a3e49;border-radius:9px;background:#1a1b22;overflow:hidden; }.number-field input { width:72px;padding:9px;border:0;background:transparent;color:white;outline:none; }.number-field em { color:#737987;font-size:10px;font-style:normal; }
  .toggle-row { display:flex;align-items:center;justify-content:space-between;gap:20px;padding-top:17px;border-top:1px solid #2e313b; }.toggle-row > span { display:grid;gap:4px; }.toggle-row strong {font-size:12px}.toggle-row small{color:#858b98}.toggle-row input{width:38px;height:20px;accent-color:#6f9ce8}.rows{display:grid;gap:18px}.rows .toggle-row:first-child{padding-top:0;border-top:0}
  .select-row{display:flex;align-items:center;justify-content:space-between;gap:28px;margin-bottom:17px}.select-row>span{display:grid;gap:4px;max-width:470px}.select-row strong{font-size:12px}.select-row small{color:#858b98;font-size:11px;line-height:1.45}.select-row select{min-width:250px;padding:9px 34px 9px 11px;border:1px solid #3a3e49;border-radius:9px;background:#191a21;color:#eef0f5;outline:none;font-size:11px}.select-row select:focus{border-color:#6c93d2;box-shadow:0 0 0 3px rgba(108,147,210,.12)}
  .empty{display:grid;justify-items:center;padding:30px 20px;color:#818795;text-align:center}.empty strong{margin-top:8px;color:#c4c8d0}.empty p{max-width:430px;margin:6px 0 0;font-size:11px}.empty.compact{padding:24px}.add-app{display:flex;gap:8px}.add-app input{flex:1;padding:10px 12px;border:1px solid #393d48;border-radius:9px;background:#191a21;color:white;outline:none}.add-app input:focus{border-color:#6c93d2}.add-app button,.secondary{padding:9px 13px;border:1px solid #424653;border-radius:9px;background:#2b2e38;cursor:pointer;font-size:11px}.inline-error{color:#ffaaa3;font-size:11px}.app-list{margin-top:13px}.app-list>div:not(.empty){display:flex;align-items:center;gap:11px;padding:12px 2px;border-top:1px solid #2e313b}.app-list strong{font-size:12px}.app-list button{margin-left:auto;border:0;background:transparent;color:#d78d89;cursor:pointer;font-size:11px}.app-icon{display:grid;place-items:center;width:28px;height:28px;border-radius:7px;background:#2b2e38;color:#959ba8}
  .location-summary{display:flex;align-items:center;gap:12px}.location-summary>span{display:grid;place-items:center;width:38px;height:38px;border-radius:10px;background:#2b3040;color:#8aaff0;font-size:18px}.location-summary>div{display:grid;gap:4px}.location-summary small{color:#858b98}.map-panel{margin-top:17px}.notice{display:grid;gap:5px;padding:16px;border-radius:12px}.notice.danger{background:#3b2327;color:#ffb5af}.loading{color:#8c929f}.error{color:#ffaaa3}
  .map-credit{margin:14px 0 0;color:#737a88;font-size:12px}.map-credit a{color:#8aaff0;text-decoration:none}.map-credit a:hover{text-decoration:underline}
  .hotkey-list{display:grid}.hotkey-list label{display:flex;align-items:center;justify-content:space-between;gap:24px;padding:13px 0;border-top:1px solid #2e313b}.hotkey-list label:first-child{padding-top:0;border-top:0}.hotkey-list label>span{display:grid;gap:4px}.hotkey-list strong{font-size:12px}.hotkey-list small{color:#858b98;font-size:11px}.hotkey-list input{width:160px;padding:9px 11px;border:1px solid #3a3e49;border-radius:9px;background:#191a21;color:#dce6fa;text-align:center;outline:none;font-size:12px;font-weight:600}.hotkey-list input:focus{border-color:#6c93d2;background:#1d2230;box-shadow:0 0 0 3px rgba(108,147,210,.12)}.hotkey-list input::placeholder{color:#696f7c;font-weight:400}.hotkey-note{margin:15px 0 0;padding-top:14px;border-top:1px solid #2e313b;color:#777d8a;font-size:10.5px}
  @media(max-width:820px){.app-shell{grid-template-columns:72px minmax(0,1fr)}aside{padding-inline:9px}.logo strong,nav button:not(.active){font-size:0}.logo{justify-content:center;padding-inline:0}.logo span{font-size:25px}nav button{justify-content:center;padding:11px}nav button span{font-size:17px}.save-state{font-size:0;justify-content:center}.field-grid{grid-template-columns:1fr}.status-hero{grid-template-columns:auto 1fr}.live-values{grid-column:1/-1;padding-left:69px}.select-row{align-items:stretch;flex-direction:column;gap:12px}.select-row select{width:100%}}
  @media(prefers-reduced-motion:reduce){*{transition:none!important;scroll-behavior:auto!important}}
</style>
