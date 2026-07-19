<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  const emptyState = {
    phase: "loading", intensity: 0,
    scheduled_color_temp_k: 6500, scheduled_hardware_brightness_pct: 100, scheduled_overlay_brightness_pct: 100,
    color_temp_k: 6500, hardware_brightness_pct: 100, overlay_brightness_pct: 100,
    sunrise: "--:--", sunset: "--:--",
    next_transition_label: "Calculating schedule", next_transition_time: "--:--",
    automatic: true, effects_off: false, app_bypassed: false,
    hardware_offset_pct: 0, overlay_offset_pct: 0, temperature_offset_k: 0,
    adjustment_expires_at: null,
  };

  let state = $state({ ...emptyState });
  let hardwareBrightness = $state(100);
  let overlayBrightness = $state(100);
  let temperature = $state(6500);
  let loaded = $state(false);
  let error = $state("");
  let adjustmentTimer;
  let interacting = false;
  let pendingAdjustment = null;
  let pendingAutomatic = null;
  let requestVersion = 0;

  onMount(() => {
    const onKey = (event) => event.key === "Escape" && invoke("hide_quick_panel");
    const finishInteraction = () => { interacting = false; };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerup", finishInteraction);
    window.addEventListener("pointercancel", finishInteraction);
    refresh();
    const interval = setInterval(refresh, 1000);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerup", finishInteraction);
      window.removeEventListener("pointercancel", finishInteraction);
      clearInterval(interval);
      clearTimeout(adjustmentTimer);
    };
  });

  async function refresh() {
    try {
      const next = await invoke("get_app_state");
      const confirmed = pendingAdjustment &&
        Math.abs(next.hardware_brightness_pct - pendingAdjustment.hardware) <= 1 &&
        Math.abs(next.overlay_brightness_pct - pendingAdjustment.overlay) <= 1 &&
        Math.abs(next.color_temp_k - pendingAdjustment.temperature) <= 150;
      const expired = pendingAdjustment && Date.now() > pendingAdjustment.expires;
      if (confirmed || expired) pendingAdjustment = null;
      if (pendingAutomatic && (next.automatic === pendingAutomatic.value || Date.now() > pendingAutomatic.expires)) {
        pendingAutomatic = null;
      }
      state = {
        ...next,
        ...(pendingAdjustment ? {
          automatic: state.automatic,
          effects_off: state.effects_off,
          hardware_offset_pct: state.hardware_offset_pct,
          overlay_offset_pct: state.overlay_offset_pct,
          temperature_offset_k: state.temperature_offset_k,
          adjustment_expires_at: state.adjustment_expires_at ?? state.next_transition_time,
        } : {}),
        ...(pendingAutomatic ? { automatic: pendingAutomatic.value } : {}),
      };
      if (!loaded || (!interacting && !pendingAdjustment)) {
        hardwareBrightness = next.hardware_brightness_pct;
        overlayBrightness = next.overlay_brightness_pct;
        temperature = next.color_temp_k;
      }
      loaded = true;
      error = "";
    } catch (reason) {
      error = `Lum could not read the display state: ${reason}`;
    }
  }

  function queueAdjustment() {
    const desiredHardware = Number(hardwareBrightness);
    const desiredOverlay = Number(overlayBrightness);
    const desiredTemperature = Number(temperature);
    const hardwareOffset = Math.round(desiredHardware - state.scheduled_hardware_brightness_pct);
    const overlayOffset = Math.round(desiredOverlay - state.scheduled_overlay_brightness_pct);
    const temperatureOffset = Math.round(desiredTemperature - state.scheduled_color_temp_k);
    pendingAdjustment = {
      hardware: desiredHardware,
      overlay: desiredOverlay,
      temperature: desiredTemperature,
      expires: Date.now() + 4000,
    };
    state = {
      ...state,
      automatic: true,
      effects_off: false,
      hardware_offset_pct: hardwareOffset,
      overlay_offset_pct: overlayOffset,
      temperature_offset_k: temperatureOffset,
    };
    clearTimeout(adjustmentTimer);
    const version = ++requestVersion;
    adjustmentTimer = setTimeout(() => {
      adjustmentTimer = null;
      try {
        invoke("set_temporary_adjustments", {
          hardwareOffsetPct: hardwareOffset,
          overlayOffsetPct: overlayOffset,
          temperatureOffsetK: temperatureOffset,
        }).catch((reason) => {
          if (version !== requestVersion) return;
          pendingAdjustment = null;
          error = `Adjustment failed: ${reason}`;
          refresh();
        });
      } catch (reason) {
        error = `Adjustment failed: ${reason}`;
      }
    }, 120);
  }

  function toggleAutomatic() {
    const previous = state.automatic;
    const automatic = !previous;
    pendingAutomatic = { value: automatic, expires: Date.now() + 4000 };
    state = { ...state, automatic };
    invoke("set_automatic", { automatic }).catch((reason) => {
      pendingAutomatic = null;
      state = { ...state, automatic: previous };
      error = `Could not change automatic mode: ${reason}`;
    });
  }

  function resetSchedule() {
    clearTimeout(adjustmentTimer);
    requestVersion += 1;
    adjustmentTimer = null;
    pendingAdjustment = {
      hardware: state.scheduled_hardware_brightness_pct,
      overlay: state.scheduled_overlay_brightness_pct,
      temperature: state.scheduled_color_temp_k,
      expires: Date.now() + 4000,
    };
    pendingAutomatic = { value: true, expires: Date.now() + 4000 };
    hardwareBrightness = state.scheduled_hardware_brightness_pct;
    overlayBrightness = state.scheduled_overlay_brightness_pct;
    temperature = state.scheduled_color_temp_k;
    state = { ...state, automatic: true, hardware_offset_pct: 0, overlay_offset_pct: 0, temperature_offset_k: 0 };
    Promise.all([
      invoke("reset_temporary_adjustments"),
      invoke("set_automatic", { automatic: true }),
    ]).catch((reason) => {
      pendingAdjustment = null;
      pendingAutomatic = null;
      error = `Could not return to the schedule: ${reason}`;
      refresh();
    });
  }

  async function openSettings() {
    await invoke("open_settings_window");
  }

  function titleCase(value) {
    return value ? value[0].toUpperCase() + value.slice(1) : "Lum";
  }

  let adjusted = $derived(state.hardware_offset_pct !== 0 || state.overlay_offset_pct !== 0 || state.temperature_offset_k !== 0);
  let statusLine = $derived(
    state.effects_off ? "Effects are off" :
    state.app_bypassed ? "Paused for this application" :
    !state.automatic ? "Holding your current appearance" :
    `${titleCase(state.phase)} mode · ${state.next_transition_label.toLowerCase()} at ${state.next_transition_time}`
  );
</script>

<svelte:head><title>Lum quick controls</title></svelte:head>

<main class="panel" class:loading={!loaded}>
  <header class="topbar">
    <section class="mode-row">
      <div><strong>Automatic</strong><span>{state.automatic ? "Following sun schedule" : "Appearance held"}</span></div>
      <button type="button" class="switch" class:on={state.automatic} role="switch" aria-checked={state.automatic} aria-label="Follow sun schedule automatically" onclick={toggleAutomatic}><span></span></button>
    </section>
    <button class="icon-button" type="button" aria-label="Open settings" title="Settings" onclick={openSettings}>⚙</button>
  </header>

  <section class="hero" aria-live="polite">
    <div class="orb" class:night={state.intensity > 0.45}><span></span><b>Lum</b></div>
    <div>
      <h1>{state.effects_off ? "Neutral display" : `${titleCase(state.phase)} light`}</h1>
      <p>{statusLine}</p>
    </div>
  </section>

  <section class="controls" aria-label="Temporary display adjustments">
    <label>
      <span class="label-row"><span>Monitor</span><output>{hardwareBrightness}%</output></span>
      <input aria-label="Temporary monitor brightness" type="range" min="0" max="100" step="1" bind:value={hardwareBrightness} onpointerdown={() => { interacting = true; }} oninput={queueAdjustment} disabled={!loaded || state.effects_off} />
    </label>
    <label>
      <span class="label-row"><span>Gamma</span><output>{overlayBrightness}%</output></span>
      <input class="overlay" aria-label="Temporary gamma brightness" type="range" min="5" max="100" step="1" bind:value={overlayBrightness} onpointerdown={() => { interacting = true; }} oninput={queueAdjustment} disabled={!loaded || state.effects_off} />
    </label>
    <label>
      <span class="label-row"><span>Warmth</span><output>{temperature}K</output></span>
      <input class="warmth" aria-label="Temporary color temperature" type="range" min="1800" max="10000" step="100" bind:value={temperature} onpointerdown={() => { interacting = true; }} oninput={queueAdjustment} disabled={!loaded || state.effects_off} />
    </label>
  </section>

  <footer>
    {#if adjusted}
      <div class="adjustment-note">
        <span>Adjusted until {state.adjustment_expires_at ?? state.next_transition_time}</span>
        <button type="button" onclick={resetSchedule}>Reset</button>
      </div>
    {:else if !state.automatic}
      <button type="button" class="reset-wide" onclick={resetSchedule}>Return to schedule</button>
    {:else}
      <div class="solar"><span>↑ {state.sunrise}</span><span>↓ {state.sunset}</span></div>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </footer>
</main>

<style>
  :global(*) { box-sizing: border-box; }
  :global(html) { color-scheme: dark; background: transparent; }
  :global(body) { margin: 0; padding: 10px; overflow: hidden; font-family: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif; background: transparent; color: #f5f6fa; }
  :global(button), :global(input) { font: inherit; }
  .panel { height: calc(100vh - 20px); padding: 12px 15px 10px; border: 1px solid rgba(255,255,255,.14); border-radius: 16px; background: linear-gradient(155deg, rgba(35,37,48,.97), rgba(24,25,33,.975)); box-shadow: 0 6px 16px rgba(0,0,0,.3); transition: opacity .15s ease; }
  .panel.loading { opacity: .72; }
  .topbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  button { border: 0; cursor: pointer; color: inherit; }
  .icon-button { display: grid; place-items: center; width: 44px; height: 40px; flex:0 0 auto; margin: -2px -2px 0 0; border: 1px solid rgba(121,168,255,.22); border-radius: 11px; background: rgba(121,168,255,.11); color: #e5edff; font-size: 24px; line-height: 1; box-shadow:inset 0 1px rgba(255,255,255,.06); }
  .icon-button:hover, .icon-button:focus-visible { border-color: rgba(121,168,255,.5); background: rgba(121,168,255,.2); color: #fff; outline: none; }
  .hero { display: flex; align-items: center; gap: 13px; padding: 10px 2px 12px; }
  .orb { position:relative;width:48px;height:48px;flex:0 0 auto;border-radius:50%;background:linear-gradient(145deg,#fff0bd,#ffc455);box-shadow:0 0 20px rgba(255,190,77,.18);overflow:hidden }
  .orb span { display:block;width:100%;height:100%;border-radius:50%;overflow:hidden;background:#272934;transform:translate(34px,-8px);transition:transform .35s ease }
  .orb:not(.night) span { transform:translate(52px,-12px) }
  .orb b{position:absolute;inset:0;display:grid;place-items:center;color:#fff;font-size:11px;font-weight:750;letter-spacing:.02em;text-shadow:0 1px 4px rgba(0,0,0,.8)}
  h1 { margin: 0 0 1px; font-size: 16px; line-height: 1.12; letter-spacing: -.02em; }
  .hero p { margin: 0; color: #aeb3c0; font-size: 10.5px; line-height: 1.2; }
  .controls { display: grid; gap: 11px; padding: 11px 13px 10px; border: 1px solid rgba(255,255,255,.075); border-radius: 11px; background: rgba(255,255,255,.035); }
  .controls label { display: grid; gap: 6px; }
  .label-row { display: flex; justify-content: space-between; align-items: baseline; font-size: 11.5px; color: #d7d9e0; }
  output { color: #fff; font-size: 11.5px; font-weight: 650; font-variant-numeric: tabular-nums; }
  input[type="range"] { width: 100%; height: 6px; margin: 4px 0 2px; accent-color: #79a8ff; cursor: pointer; }
  input[type="range"].warmth { accent-color: #f1a65c; }
  input[type="range"].overlay { accent-color: #57c9c1; }
  input:disabled { opacity: .42; cursor: default; }
  .mode-row { display:flex;align-items:center;gap:9px;min-width:0;padding:0 }
  .mode-row > div { display:grid;gap:0 }
  .mode-row strong { font-size:10.5px;font-weight:650;line-height:1.15 }
  .mode-row div span { color:#858b9a;font-size:8.5px;line-height:1.15 }
  .switch { position:relative;width:34px;height:20px;flex:0 0 auto;padding:2px;border-radius:20px;background:#4a4e5b;transition:background .16s ease }
  .switch span { display:block;width:16px;height:16px;border-radius:50%;background:#e6e8ed;box-shadow:0 1px 4px rgba(0,0,0,.35);transition:transform .16s ease }
  .switch.on { background: #6f9ce8; }
  .switch.on span { transform: translateX(14px); background: white; }
  .switch:focus-visible { outline: 2px solid #9ec1ff; outline-offset: 3px; }
  footer { min-height: 18px; }
  .adjustment-note, .solar { display: flex; align-items: center; justify-content: space-between; min-height: 18px; padding: 0 3px; color: #9298a7; font-size: 9.5px; }
  .adjustment-note button { padding: 3px 7px; border-radius: 6px; background: rgba(121,168,255,.12); color: #a9c8ff; font-size: 9.5px; }
  .reset-wide { width: 100%; padding: 4px; border-radius: 7px; background: rgba(121,168,255,.13); color: #b8d1ff; font-size: 10px; }
  .error { margin: 4px; color: #ffaaa3; font-size: 11px; }
  @media (prefers-reduced-motion: reduce) { * { transition: none !important; } }
</style>
