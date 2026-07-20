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
  let persisting = $state(false);
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

  async function keepAdjustment() {
    if (persisting) return;
    persisting = true;
    error = "";
    clearTimeout(adjustmentTimer);
    adjustmentTimer = null;
    requestVersion += 1;
    const target = persistTarget;
    try {
      const settings = await invoke("get_settings");
      const hardware = Math.round(Number(hardwareBrightness));
      const overlay = Math.round(Number(overlayBrightness));
      const colorTemperature = Math.round(Number(temperature));
      if (target === "night") {
        settings.brightness.hardware_night_percent = hardware;
        settings.brightness.overlay_night_percent = overlay;
        settings.color.night_temp_k = colorTemperature;
      } else {
        settings.brightness.hardware_day_percent = hardware;
        settings.brightness.overlay_day_percent = overlay;
        settings.color.day_temp_k = colorTemperature;
      }
      await invoke("save_settings", { settings });
      await invoke("reset_temporary_adjustments");
      pendingAdjustment = null;
      state = { ...state, hardware_offset_pct: 0, overlay_offset_pct: 0, temperature_offset_k: 0, adjustment_expires_at: null };
      await refresh();
    } catch (reason) {
      error = `Could not keep ${target} settings: ${reason}`;
      queueAdjustment();
    } finally {
      persisting = false;
    }
  }

  async function openSettings() {
    await invoke("open_settings_window");
  }

  function titleCase(value) {
    return value ? value[0].toUpperCase() + value.slice(1) : "Lum";
  }

  let adjusted = $derived(state.hardware_offset_pct !== 0 || state.overlay_offset_pct !== 0 || state.temperature_offset_k !== 0);
  let persistTarget = $derived(state.intensity >= 0.5 ? "night" : "day");
  let temperaturePercent = $derived(((Number(temperature) - 1800) / 8200) * 100);
  let statusLine = $derived(
    state.effects_off ? "Effects are off" :
    state.app_bypassed ? "Paused for this application" :
    !state.automatic ? "Holding your current appearance" :
    `${titleCase(state.phase)} mode · ${state.next_transition_label.toLowerCase()} at ${state.next_transition_time}`
  );
</script>

<svelte:head><title>Lum quick controls</title></svelte:head>

<main class="panel" class:loading={!loaded}>
  <div class="solar-strip" aria-label={`Sunrise ${state.sunrise}, sunset ${state.sunset}`}>
    <span class="sunrise"><i>↑</i><small>Sunrise</small><strong>{state.sunrise}</strong></span>
    <span class="sunset"><small>Sunset</small><strong>{state.sunset}</strong><i>↓</i></span>
  </div>

  <section class="hero" aria-live="polite">
    <div class="orb" class:night={state.intensity > 0.45}><span></span></div>
    <div>
      <h1>Lum</h1>
      <p>{statusLine}</p>
    </div>
  </section>

  <section class="controls" aria-label="Temporary display adjustments">
    <label>
      <span class="label-row"><span>Monitor</span><output>{hardwareBrightness}%</output></span>
      <input style={`--value:${hardwareBrightness}%`} aria-label="Temporary monitor brightness" type="range" min="0" max="100" step="1" bind:value={hardwareBrightness} onpointerdown={() => { interacting = true; }} oninput={queueAdjustment} disabled={!loaded || state.effects_off} />
    </label>
    <label>
      <span class="label-row"><span>Gamma</span><output>{overlayBrightness}%</output></span>
      <input style={`--value:${overlayBrightness}%`} class="overlay" aria-label="Temporary gamma brightness" type="range" min="5" max="100" step="1" bind:value={overlayBrightness} onpointerdown={() => { interacting = true; }} oninput={queueAdjustment} disabled={!loaded || state.effects_off} />
    </label>
    <label>
      <span class="label-row"><span>Warmth</span><output>{temperature}K</output></span>
      <input style={`--value:${temperaturePercent}%`} class="warmth" aria-label="Temporary color temperature" type="range" min="1800" max="10000" step="100" bind:value={temperature} onpointerdown={() => { interacting = true; }} oninput={queueAdjustment} disabled={!loaded || state.effects_off} />
    </label>
  </section>

  {#if adjusted}
    <section class="adjustment-state" aria-live="polite">
      <div class="adjustment-note">
        <span>Adjusted until {state.adjustment_expires_at ?? state.next_transition_time}</span>
        <div class="adjustment-actions">
          <button type="button" class="reset-button" onclick={resetSchedule} disabled={persisting}>Reset</button>
          <button type="button" class="keep-button" aria-label={`Permanently save these values as the ${persistTarget} schedule preset`} title={`Permanently update the ${persistTarget} schedule preset`} onclick={keepAdjustment} disabled={persisting}>{persisting ? "Saving…" : `Save as ${persistTarget} preset`}</button>
        </div>
      </div>
    </section>
  {:else if !state.automatic}
    <section class="adjustment-state" aria-live="polite">
      <button type="button" class="reset-wide" onclick={resetSchedule}>Return to schedule</button>
    </section>
  {/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <footer class="bottom-bar">
    <section class="mode-row">
      <div><strong>Automatic</strong><span>{state.automatic ? "Following sun schedule" : "Appearance held"}</span></div>
      <button type="button" class="switch" class:on={state.automatic} role="switch" aria-checked={state.automatic} aria-label="Follow sun schedule automatically" onclick={toggleAutomatic}><span></span></button>
    </section>
    <button class="icon-button" type="button" aria-label="Open settings" title="Settings" onclick={openSettings}>⚙</button>
  </footer>
</main>

<style>
  :global(*) { box-sizing: border-box; }
  :global(html) { color-scheme: dark; background: transparent; }
  :global(body) { margin: 0; padding: 10px; overflow: hidden; font-family: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif; background: transparent; color: #f5f6fa; }
  :global(button), :global(input) { font: inherit; }
  .panel { position:relative;display:flex;flex-direction:column;height:calc(100vh - 20px);padding:11px 15px 10px;border:1px solid rgba(255,255,255,.14);border-radius:16px;background:linear-gradient(155deg,rgba(35,37,48,.97),rgba(24,25,33,.975));box-shadow:0 6px 16px rgba(0,0,0,.3);transition:opacity .15s ease }
  .panel.loading { opacity: .72; }
  button { border: 0; cursor: pointer; color: inherit; }
  .solar-strip{display:flex;align-items:center;justify-content:space-between;padding:0 2px 8px;border-bottom:1px solid rgba(255,255,255,.07);font-variant-numeric:tabular-nums}
  .solar-strip span{display:flex;align-items:center;gap:5px;font-size:11px}
  .solar-strip small{color:#8d93a2;font-size:9.5px}
  .solar-strip strong{font-size:11px;font-weight:650}
  .solar-strip i{font-size:14px;font-style:normal;line-height:1}
  .solar-strip .sunrise i,.solar-strip .sunrise strong{color:#f5bd59}
  .solar-strip .sunset i,.solar-strip .sunset strong{color:#f08a61}
  .hero{display:flex;align-items:center;gap:10px;padding:9px 2px 7px}
  .orb{position:relative;width:34px;height:34px;flex:0 0 auto;border-radius:50%;background:linear-gradient(145deg,#fff0bd,#ffc455);box-shadow:0 0 16px rgba(255,190,77,.16);overflow:hidden}
  .orb span{display:block;width:100%;height:100%;border-radius:50%;background:#272934;transform:translate(24px,-6px);transition:transform .35s ease}
  .orb:not(.night) span{transform:translate(38px,-9px)}
  h1{margin:0 0 2px;font-size:15px;line-height:1;letter-spacing:-.01em}
  .hero p{margin:0;color:#9da3b1;font-size:10px;line-height:1.2}
  .controls{display:grid;flex:1;grid-template-rows:repeat(3,1fr);gap:8px;padding:7px 1px 6px}
  .controls label{display:grid;align-content:center;gap:4px}
  .label-row{display:flex;justify-content:space-between;align-items:baseline;padding:0 1px;color:#d7d9e0;font-size:11.5px}
  output{color:#fff;font-size:11.5px;font-weight:650;font-variant-numeric:tabular-nums}
  input[type="range"]{--slider-color:#79a8ff;width:100%;height:22px;margin:0;appearance:none;background:transparent;cursor:pointer}
  input[type="range"]::-webkit-slider-runnable-track{height:5px;border-radius:5px;background:linear-gradient(90deg,color-mix(in srgb,var(--slider-color) 88%,white) 0 var(--value),rgba(255,255,255,.22) var(--value) 100%);box-shadow:inset 0 1px 2px rgba(0,0,0,.32)}
  input[type="range"]::-webkit-slider-thumb{width:16px;height:16px;margin-top:-5.5px;appearance:none;border:2px solid #dce7fb;border-radius:50%;background:var(--slider-color);box-shadow:0 2px 6px rgba(0,0,0,.45)}
  input[type="range"]:focus-visible{outline:none}
  input[type="range"]:focus-visible::-webkit-slider-thumb{box-shadow:0 0 0 3px rgba(255,255,255,.22),0 2px 6px rgba(0,0,0,.45)}
  input[type="range"].overlay{--slider-color:#57c9c1}
  input[type="range"].warmth{--slider-color:#f1a65c}
  input:disabled{opacity:.42;cursor:default}
  .adjustment-state{padding:2px 1px 5px}
  .adjustment-note{display:flex;align-items:center;justify-content:space-between;min-height:23px;color:#9298a7;font-size:9.5px}
  .adjustment-actions{display:flex;align-items:center;gap:5px}
  .adjustment-note button{padding:4px 8px;border-radius:7px;font-size:9.5px;white-space:nowrap}
  .reset-button{background:rgba(255,255,255,.055);color:#aab0bd}
  .keep-button{background:rgba(121,168,255,.16);color:#b9d2ff}
  .adjustment-note button:hover,.adjustment-note button:focus-visible{filter:brightness(1.22);outline:1px solid rgba(158,193,255,.45);outline-offset:1px}
  .adjustment-note button:disabled{opacity:.55;cursor:default;filter:none}
  .reset-wide{width:100%;padding:5px;border-radius:7px;background:rgba(121,168,255,.13);color:#b8d1ff;font-size:10px}
  .bottom-bar{display:flex;align-items:center;justify-content:space-between;gap:12px;margin-top:auto;padding-top:8px;border-top:1px solid rgba(255,255,255,.08)}
  .mode-row{display:flex;align-items:center;gap:9px;min-width:0}
  .mode-row>div{display:grid;gap:0}
  .mode-row strong{font-size:10.5px;font-weight:650;line-height:1.15}
  .mode-row div span{color:#858b9a;font-size:8.5px;line-height:1.15}
  .switch{position:relative;width:34px;height:20px;flex:0 0 auto;padding:2px;border-radius:20px;background:#4a4e5b;transition:background .16s ease}
  .switch span{display:block;width:16px;height:16px;border-radius:50%;background:#e6e8ed;box-shadow:0 1px 4px rgba(0,0,0,.35);transition:transform .16s ease}
  .switch.on{background:#6f9ce8}
  .switch.on span{transform:translateX(14px);background:white}
  .switch:focus-visible{outline:2px solid #9ec1ff;outline-offset:3px}
  .icon-button{display:grid;place-items:center;width:46px;height:42px;flex:0 0 auto;border:1px solid rgba(121,168,255,.3);border-radius:11px;background:rgba(121,168,255,.14);color:#e9f0ff;font-size:25px;line-height:1;box-shadow:inset 0 1px rgba(255,255,255,.08)}
  .icon-button:hover, .icon-button:focus-visible { border-color: rgba(121,168,255,.5); background: rgba(121,168,255,.2); color: #fff; outline: none; }
  .error{position:absolute;z-index:3;left:14px;right:14px;bottom:62px;margin:0;padding:7px 9px;border:1px solid rgba(255,120,112,.24);border-radius:8px;background:rgba(49,27,31,.96);color:#ffaaa3;font-size:10px;line-height:1.25;box-shadow:0 5px 15px rgba(0,0,0,.35)}
  @media (prefers-reduced-motion: reduce) { * { transition: none !important; } }
</style>
