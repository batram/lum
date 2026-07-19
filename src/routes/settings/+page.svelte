<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import MapPicker from "$lib/MapPicker.svelte";
  import Curves from "$lib/Curves.svelte";

  let settings = $state(null);
  let monitors = $state([]);
  let autostart = $state(false);
  let newApp = $state("");
  let monitorBrightness = $state([]);
  let saveIndicator = $state("");

  let debounceTimer = null;
  let brightnessInterval = null;

  onMount(async () => {
    settings = await invoke("get_settings");
    monitors = await invoke("get_monitors");
    autostart = await invoke("get_autostart");

    // Read initial brightness from hardware
    await pollBrightness();

    // Poll monitor brightness every 3 seconds
    brightnessInterval = setInterval(pollBrightness, 3000);

    return () => {
      clearInterval(brightnessInterval);
    };
  });

  async function pollBrightness() {
    try {
      const levels = await invoke("get_all_brightness");
      monitorBrightness = levels.map((v, i) =>
        v != null ? v : (monitorBrightness[i] ?? 100)
      );
    } catch {
      // DDC/CI read failed silently
    }
  }

  // Auto-save settings with debounce whenever settings change
  $effect(() => {
    if (!settings) return;
    // Touch all reactive properties so this effect re-runs on any change
    JSON.stringify(settings);

    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      await invoke("save_settings", { settings });
      saveIndicator = "saved";
      setTimeout(() => (saveIndicator = ""), 1500);
    }, 500);
  });

  async function toggleAutostart() {
    autostart = await invoke("toggle_autostart");
  }

  function addApp() {
    const name = newApp.trim().toLowerCase();
    if (name && !settings.pause_apps.includes(name)) {
      settings.pause_apps = [...settings.pause_apps, name.endsWith(".exe") ? name : name + ".exe"];
    }
    newApp = "";
  }

  function removeApp(app) {
    settings.pause_apps = settings.pause_apps.filter((a) => a !== app);
  }

  // DDC/CI: set monitor brightness immediately on slider input
  function onMonitorBrightness(index, value) {
    monitorBrightness[index] = value;
    invoke("set_monitor_brightness", { index, percent: value });
  }
</script>

<main class="container">
  <header>
    <a href="/" class="back">← Status</a>
    <h1>Settings</h1>
  </header>

  {#if settings}
    <!-- 24h Curves Overview -->
    <section class="curves-section">
      <h2>24-Hour Schedule</h2>
      <p class="hint">Drag the circles on the curves to adjust brightness and color temperature</p>
      <Curves {settings} onThemeChange={() => {}} />
    </section>

    <!-- Rendering Engine -->
    <section>
      <h2>Rendering Engine</h2>
      <div class="row">
        <label>
          <input type="radio" bind:group={settings.engine} value="gamma_ramps" />
          Gamma Ramps (recommended)
        </label>
        <label>
          <input type="radio" bind:group={settings.engine} value="night_light" />
          Night Light (registry)
        </label>
      </div>
    </section>

    <!-- Location -->
    <section>
      <h2>Location</h2>
      <MapPicker
        lat={settings.location.latitude}
        lng={settings.location.longitude}
        onPick={(la, ln) => {
          settings.location.latitude = la;
          settings.location.longitude = ln;
        }}
      />
    </section>

    <!-- Fade Timing -->
    <section>
      <h2>Fade Timing</h2>
      <div class="grid">
        <label>
          Evening offset (min before sunset)
          <input type="number" bind:value={settings.fade.evening_offset_min} />
        </label>
        <label>
          Morning offset (min after sunrise)
          <input type="number" bind:value={settings.fade.morning_offset_min} />
        </label>
        <label>
          Fade duration (minutes)
          <input type="number" min="5" max="180" bind:value={settings.fade.fade_duration_min} />
        </label>
      </div>
      <label class="checkbox">
        <input type="checkbox" bind:checked={settings.fade.use_civil_twilight} />
        Use civil twilight (-6°) instead of fixed offsets
      </label>
    </section>

    <!-- Color Temperature -->
    <section>
      <h2>Color Temperature</h2>
      <div class="grid">
        <label>
          Day temperature ({settings.color.day_temp_k}K)
          <input type="range" min="4000" max="10000" step="100" bind:value={settings.color.day_temp_k} />
        </label>
        <label>
          Night temperature ({settings.color.night_temp_k}K)
          <input type="range" min="1800" max="5000" step="100" bind:value={settings.color.night_temp_k} />
        </label>
      </div>
    </section>

    <!-- Brightness -->
    <section>
      <h2>Brightness</h2>
      <div class="grid">
        <label>
          Day brightness ({settings.brightness.day_percent}%)
          <input type="range" min="30" max="100" bind:value={settings.brightness.day_percent} />
        </label>
        <label>
          Night brightness ({settings.brightness.night_percent}%)
          <input type="range" min="10" max="100" bind:value={settings.brightness.night_percent} />
        </label>
      </div>
      <label class="checkbox">
        <input type="checkbox" bind:checked={settings.brightness.linked_to_color} />
        Link brightness curve to color curve
      </label>
    </section>

    <!-- Theme -->
    <section>
      <h2>Windows Theme</h2>
      <label class="checkbox">
        <input type="checkbox" bind:checked={settings.theme.auto_switch} />
        Auto-switch dark/light theme with the sun
      </label>
      <label class="checkbox">
        <input type="checkbox" bind:checked={settings.theme.dark_at_night} />
        Use dark theme at night
      </label>
    </section>

    <!-- Per-App Pause List -->
    <section>
      <h2>Per-App Pause List</h2>
      <p class="hint">Effects pause while these apps are focused.</p>
      <ul class="app-list">
        {#each settings.pause_apps as app}
          <li>
            <span>{app}</span>
            <button class="remove" onclick={() => removeApp(app)}>×</button>
          </li>
        {/each}
      </ul>
      <div class="add-row">
        <input
          type="text"
          placeholder="e.g. photoshop.exe"
          bind:value={newApp}
          onkeydown={(e) => e.key === "Enter" && addApp()}
        />
        <button onclick={addApp}>Add</button>
      </div>
    </section>

    <!-- Monitors -->
    <section>
      <h2>Monitors (DDC/CI)</h2>
      {#if monitors.length === 0}
        <p class="hint">No DDC/CI-capable monitors detected.</p>
      {:else}
        <ul class="monitor-list">
          {#each monitors as mon, i}
            <li>
              <div class="mon-header">
                <span class="mon-name">{mon.description || `Monitor ${mon.index + 1}`}</span>
                <span class="badge" class:ok={mon.supports_brightness} class:no={!mon.supports_brightness}>
                  {mon.supports_brightness ? `${mon.brightness_min}–${mon.brightness_max}` : "No DDC/CI"}
                </span>
              </div>
              {#if mon.supports_brightness}
                <div class="mon-slider">
                  <input
                    type="range"
                    min="0"
                    max="100"
                    value={monitorBrightness[i] ?? 100}
                    oninput={(e) => onMonitorBrightness(i, +e.target.value)}
                  />
                  <span class="mon-val">{monitorBrightness[i] ?? 100}%</span>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <!-- Auto-start -->
    <section>
      <h2>Startup</h2>
      <label class="checkbox">
        <input type="checkbox" checked={autostart} onchange={toggleAutostart} />
        Start Lum with Windows
      </label>
    </section>

    <!-- Auto-save indicator -->
    {#if saveIndicator}
      <div class="save-toast">✓ Saved</div>
    {/if}
  {:else}
    <p>Loading settings...</p>
  {/if}
</main>

<style>
  .container {
    padding: 1.5rem 2rem;
    max-width: 860px;
    margin: 0 auto;
    font-family: "Segoe UI", system-ui, sans-serif;
    font-size: 14px;
    color: #1a1a2e;
  }

  .curves-section {
    grid-column: 1 / -1;
  }

  header {
    display: flex;
    align-items: center;
    gap: 1rem;
    margin-bottom: 1.5rem;
  }

  .back {
    color: #646cff;
    text-decoration: none;
    font-size: 0.85rem;
  }

  h1 {
    font-size: 1.3rem;
    margin: 0;
  }

  h2 {
    font-size: 0.9rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #888;
    margin: 0 0 0.6rem;
  }

  section {
    margin-bottom: 1.5rem;
    padding-bottom: 1.25rem;
    border-bottom: 1px solid #eee;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
  }

  .grid label,
  .row label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.82rem;
    color: #555;
  }

  .row {
    display: flex;
    gap: 1.5rem;
  }

  .row label {
    flex-direction: row;
    align-items: center;
    gap: 0.4rem;
  }

  input[type="number"],
  input[type="text"] {
    padding: 0.4rem 0.6rem;
    border: 1px solid #ddd;
    border-radius: 6px;
    font-size: 0.85rem;
    width: 100%;
  }

  input[type="range"] {
    width: 100%;
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.5rem;
    font-size: 0.85rem;
    color: #555;
  }

  .hint {
    font-size: 0.8rem;
    color: #999;
    margin: 0 0 0.5rem;
  }

  .app-list {
    list-style: none;
    padding: 0;
    margin: 0 0 0.5rem;
  }

  .app-list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.35rem 0.6rem;
    background: #f5f5f5;
    border-radius: 6px;
    margin-bottom: 0.3rem;
    font-size: 0.85rem;
  }

  .remove {
    background: none;
    border: none;
    color: #e44;
    font-size: 1.1rem;
    cursor: pointer;
    padding: 0 0.3rem;
  }

  .add-row {
    display: flex;
    gap: 0.5rem;
  }

  .add-row input {
    flex: 1;
  }

  .add-row button {
    padding: 0.4rem 1rem;
    border: none;
    border-radius: 6px;
    background: #2d2d3a;
    color: #fff;
    font-size: 0.82rem;
    cursor: pointer;
  }

  .monitor-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .monitor-list li {
    padding: 0.5rem 0;
  }

  .mon-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .mon-slider {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin-top: 0.35rem;
  }

  .mon-slider input[type="range"] {
    flex: 1;
  }

  .mon-val {
    font-size: 0.8rem;
    color: #666;
    min-width: 2.5rem;
    text-align: right;
  }

  .mon-name {
    font-size: 0.85rem;
  }

  .badge {
    font-size: 0.75rem;
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
  }

  .badge.ok {
    background: #e6f9e6;
    color: #2a7a2a;
  }

  .badge.no {
    background: #fde8e8;
    color: #a33;
  }

  .save-toast {
    position: fixed;
    bottom: 1.5rem;
    left: 50%;
    transform: translateX(-50%);
    background: #2d2d3a;
    color: #fff;
    padding: 0.4rem 1.2rem;
    border-radius: 6px;
    font-size: 0.82rem;
    animation: fadeIn 0.2s ease;
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateX(-50%) translateY(4px); }
    to { opacity: 1; transform: translateX(-50%) translateY(0); }
  }

  @media (prefers-color-scheme: dark) {
    .container {
      color: #e8e6e3;
    }

    h2 {
      color: #999;
    }

    section {
      border-color: #333;
    }

    .grid label,
    .row label,
    .checkbox {
      color: #bbb;
    }

    input[type="number"],
    input[type="text"] {
      background: #2a2a3e;
      border-color: #444;
      color: #eee;
    }

    .app-list li {
      background: #2a2a3e;
    }

    .badge.ok {
      background: #1a3a1a;
      color: #6d6;
    }

    .badge.no {
      background: #3a1a1a;
      color: #d66;
    }

    .add-row button {
      background: #3a3a50;
    }

    .mon-val {
      color: #aaa;
    }

    .save-toast {
      background: #3a3a50;
    }
  }
</style>
