<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let state = $state({
    phase: "loading",
    intensity: 0,
    color_temp_k: 6500,
    brightness_pct: 100,
    paused: false,
    sunrise: "--:--",
    sunset: "--:--",
  });

  let interval;

  async function refresh() {
    try {
      state = await invoke("get_app_state");
    } catch (e) {
      console.error("Failed to get state:", e);
    }
  }

  async function togglePause() {
    await invoke("toggle_pause");
    refresh();
  }

  async function jumpNight() {
    const goNight = state.phase === "day" || state.phase === "morning";
    await invoke("jump_to_night", { night: goNight });
    refresh();
  }

  onMount(() => {
    refresh();
    interval = setInterval(refresh, 2000);
    return () => clearInterval(interval);
  });

  const phaseIcon = $derived(
    state.phase === "day" ? "☀️" :
    state.phase === "night" ? "🌙" :
    state.phase === "evening" ? "🌅" :
    state.phase === "morning" ? "🌄" : "⏳"
  );
</script>

<main class="container">
  <h1>{phaseIcon} Lum</h1>
  <p class="subtitle">Sun-aware screen warmth &amp; brightness</p>

  <div class="status-card">
    <div class="status-row">
      <span class="label">Phase</span>
      <span class="value">{state.phase}{state.paused ? " (paused)" : ""}</span>
    </div>
    <div class="status-row">
      <span class="label">Color temp</span>
      <span class="value">{state.color_temp_k}K</span>
    </div>
    <div class="status-row">
      <span class="label">Brightness</span>
      <span class="value">{state.brightness_pct}%</span>
    </div>
    <div class="status-row">
      <span class="label">Intensity</span>
      <span class="value">{Math.round(state.intensity * 100)}%</span>
    </div>
  </div>

  <div class="sun-times">
    <span>↑ {state.sunrise}</span>
    <span>↓ {state.sunset}</span>
  </div>

  <div class="actions">
    <button onclick={togglePause}>
      {state.paused ? "Resume" : "Pause"}
    </button>
    <button onclick={jumpNight}>
      {state.phase === "day" || state.phase === "morning" ? "Jump to Night" : "Jump to Day"}
    </button>
    <a href="/settings" class="settings-link">Settings</a>
  </div>
</main>

<style>
  :root {
    font-family: "Segoe UI", system-ui, -apple-system, sans-serif;
    font-size: 15px;
    line-height: 1.5;
    color: #1a1a2e;
    background-color: #f8f7f4;
  }

  .container {
    margin: 0;
    padding: 2.5rem 2rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 0.75rem;
  }

  h1 {
    font-size: 1.6rem;
    font-weight: 600;
    margin: 0;
  }

  .subtitle {
    color: #666;
    margin: 0 0 1rem;
    font-size: 0.9rem;
  }

  .status-card {
    width: 100%;
    max-width: 280px;
    padding: 1rem 1.25rem;
    border-radius: 12px;
    background: #fff;
    box-shadow: 0 1px 6px rgba(0, 0, 0, 0.07);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .status-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .label {
    color: #888;
    font-size: 0.85rem;
  }

  .value {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .sun-times {
    display: flex;
    gap: 1.5rem;
    font-size: 0.85rem;
    color: #999;
    margin-top: 0.25rem;
  }

  .actions {
    display: flex;
    gap: 0.75rem;
    margin-top: 1rem;
  }

  button {
    padding: 0.5rem 1.25rem;
    border: none;
    border-radius: 8px;
    background: #2d2d3a;
    color: #fff;
    font-size: 0.85rem;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.2s;
  }

  button:hover {
    background: #44445a;
  }

  button:active {
    background: #1a1a28;
  }

  @media (prefers-color-scheme: dark) {
    :root {
      color: #e8e6e3;
      background-color: #1a1a2e;
    }

    .subtitle {
      color: #aaa;
    }

    .status-card {
      background: #2a2a3e;
      box-shadow: 0 1px 6px rgba(0, 0, 0, 0.3);
    }

    .label {
      color: #999;
    }

    .sun-times {
      color: #777;
    }

    button {
      background: #3a3a50;
    }

    button:hover {
      background: #4a4a66;
    }
  }

  .settings-link {
    display: block;
    text-align: center;
    color: #646cff;
    text-decoration: none;
    font-size: 0.85rem;
    margin-top: 0.75rem;
  }

  .settings-link:hover {
    text-decoration: underline;
  }
</style>
