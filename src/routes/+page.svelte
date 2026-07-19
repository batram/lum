<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  const emptyState = {
    phase: "loading", intensity: 0,
    scheduled_color_temp_k: 6500, scheduled_brightness_pct: 100,
    color_temp_k: 6500, brightness_pct: 100,
    sunrise: "--:--", sunset: "--:--",
    next_transition_label: "Calculating schedule", next_transition_time: "--:--",
    automatic: true, effects_off: false, app_bypassed: false,
    brightness_offset_pct: 0, temperature_offset_k: 0,
    adjustment_expires_at: null,
  };

  let state = $state({ ...emptyState });
  let brightness = $state(100);
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
        Math.abs(next.brightness_pct - pendingAdjustment.brightness) <= 1 &&
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
          brightness_offset_pct: state.brightness_offset_pct,
          temperature_offset_k: state.temperature_offset_k,
          adjustment_expires_at: state.adjustment_expires_at ?? state.next_transition_time,
        } : {}),
        ...(pendingAutomatic ? { automatic: pendingAutomatic.value } : {}),
      };
      if (!loaded || (!interacting && !pendingAdjustment)) {
        brightness = next.brightness_pct;
        temperature = next.color_temp_k;
      }
      loaded = true;
      error = "";
    } catch (reason) {
      error = `Lum could not read the display state: ${reason}`;
    }
  }

  function queueAdjustment() {
    const desiredBrightness = Number(brightness);
    const desiredTemperature = Number(temperature);
    const brightnessOffset = Math.round(desiredBrightness - state.scheduled_brightness_pct);
    const temperatureOffset = Math.round(desiredTemperature - state.scheduled_color_temp_k);
    pendingAdjustment = {
      brightness: desiredBrightness,
      temperature: desiredTemperature,
      expires: Date.now() + 4000,
    };
    state = {
      ...state,
      automatic: true,
      effects_off: false,
      brightness_offset_pct: brightnessOffset,
      temperature_offset_k: temperatureOffset,
    };
    clearTimeout(adjustmentTimer);
    const version = ++requestVersion;
    adjustmentTimer = setTimeout(() => {
      adjustmentTimer = null;
      try {
        invoke("set_temporary_adjustments", {
          brightnessOffsetPct: brightnessOffset,
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
      brightness: state.scheduled_brightness_pct,
      temperature: state.scheduled_color_temp_k,
      expires: Date.now() + 4000,
    };
    pendingAutomatic = { value: true, expires: Date.now() + 4000 };
    brightness = state.scheduled_brightness_pct;
    temperature = state.scheduled_color_temp_k;
    state = { ...state, automatic: true, brightness_offset_pct: 0, temperature_offset_k: 0 };
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

  let adjusted = $derived(state.brightness_offset_pct !== 0 || state.temperature_offset_k !== 0);
  let statusLine = $derived(
    state.effects_off ? "Effects are off" :
    state.app_bypassed ? "Paused for this application" :
    !state.automatic ? "Holding your current appearance" :
    `${titleCase(state.phase)} mode · ${state.next_transition_label.toLowerCase()} at ${state.next_transition_time}`
  );
</script>

<svelte:head><title>Lum quick controls</title></svelte:head>

<main class="panel" class:loading={!loaded}>
  <header>
    <div class="brand" aria-label="Lum">
      <span class="mark">◒</span>
      <span>Lum</span>
    </div>
    <button class="icon-button" type="button" aria-label="Open settings" title="Settings" onclick={openSettings}>⚙</button>
  </header>

  <section class="hero" aria-live="polite">
    <div class="orb" class:night={state.intensity > 0.45}><span></span></div>
    <div>
      <h1>{state.effects_off ? "Neutral display" : `${titleCase(state.phase)} light`}</h1>
      <p>{statusLine}</p>
    </div>
  </section>

  <section class="controls" aria-label="Temporary display adjustments">
    <label>
      <span class="label-row"><span>Brightness</span><output>{brightness}%</output></span>
      <input aria-label="Temporary brightness" type="range" min="0" max="100" step="1" bind:value={brightness} onpointerdown={() => { interacting = true; }} oninput={queueAdjustment} disabled={!loaded || state.effects_off} />
    </label>
    <label>
      <span class="label-row"><span>Warmth</span><output>{temperature}K</output></span>
      <input class="warmth" aria-label="Temporary color temperature" type="range" min="1800" max="10000" step="100" bind:value={temperature} onpointerdown={() => { interacting = true; }} oninput={queueAdjustment} disabled={!loaded || state.effects_off} />
      <span class="scale"><span>Warmer</span><span>Cooler</span></span>
    </label>
  </section>

  <section class="mode-row">
    <div>
      <strong>Automatic</strong>
      <span>{state.automatic ? "Following your sun schedule" : "Current appearance is held"}</span>
    </div>
    <button type="button" class="switch" class:on={state.automatic} role="switch" aria-checked={state.automatic} aria-label="Follow sun schedule automatically" onclick={toggleAutomatic}><span></span></button>
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
  :global(body) { margin: 0; overflow: hidden; font-family: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif; background: transparent; color: #f5f6fa; }
  :global(button), :global(input) { font: inherit; }
  .panel { height: 100vh; padding: 7px 11px 6px; border: 1px solid rgba(255,255,255,.13); border-radius: 13px; background: linear-gradient(155deg, rgba(35,37,48,.985), rgba(24,25,33,.99)); box-shadow: 0 14px 40px rgba(0,0,0,.4); transition: opacity .15s ease; }
  .panel.loading { opacity: .72; }
  header { display: flex; align-items: center; justify-content: space-between; }
  .brand { display: flex; align-items: center; gap: 6px; font-size: 13px; font-weight: 650; letter-spacing: -.01em; }
  .mark { display: grid; place-items: center; width: 19px; height: 19px; color: #ffc85b; font-size: 18px; transform: rotate(-18deg); }
  button { border: 0; cursor: pointer; color: inherit; }
  .icon-button { display: grid; place-items: center; width: 32px; height: 30px; margin: -1px -2px -1px 0; border-radius: 8px; background: rgba(255,255,255,.045); color: #c7ccda; font-size: 17px; line-height: 1; }
  .icon-button:hover, .icon-button:focus-visible { background: rgba(255,255,255,.08); color: #fff; outline: none; }
  .hero { display: flex; align-items: center; gap: 9px; padding: 5px 2px 6px; }
  .orb { width: 31px; height: 31px; flex: 0 0 auto; border-radius: 50%; background: linear-gradient(145deg,#fff0bd,#ffc455); box-shadow: 0 0 20px rgba(255,190,77,.18); overflow: hidden; }
  .orb span { display: block; width: 100%; height: 100%; border-radius: 50%; background: #272934; transform: translate(23px,-6px); transition: transform .35s ease; }
  .orb:not(.night) span { transform: translate(35px,-10px); }
  h1 { margin: 0 0 1px; font-size: 16px; line-height: 1.12; letter-spacing: -.02em; }
  .hero p { margin: 0; color: #aeb3c0; font-size: 10.5px; line-height: 1.2; }
  .controls { display: grid; gap: 7px; padding: 7px 11px 6px; border: 1px solid rgba(255,255,255,.075); border-radius: 10px; background: rgba(255,255,255,.035); }
  .controls label { display: grid; gap: 2px; }
  .label-row { display: flex; justify-content: space-between; align-items: baseline; font-size: 11.5px; color: #d7d9e0; }
  output { color: #fff; font-size: 11.5px; font-weight: 650; font-variant-numeric: tabular-nums; }
  input[type="range"] { width: 100%; height: 4px; margin: 3px 0; accent-color: #79a8ff; cursor: pointer; }
  input[type="range"].warmth { accent-color: #f1a65c; }
  input:disabled { opacity: .42; cursor: default; }
  .scale { display: flex; justify-content: space-between; margin-top: -3px; color: #777d8c; font-size: 8.5px; line-height: 1; }
  .mode-row { display: flex; justify-content: space-between; align-items: center; padding: 6px 3px 3px; }
  .mode-row > div { display: grid; gap: 1px; }
  .mode-row strong { font-size: 11.5px; font-weight: 600; }
  .mode-row div span { color: #8f95a4; font-size: 9.5px; }
  .switch { position: relative; width: 38px; height: 22px; padding: 2px; border-radius: 20px; background: #4a4e5b; transition: background .16s ease; }
  .switch span { display: block; width: 18px; height: 18px; border-radius: 50%; background: #e6e8ed; box-shadow: 0 1px 4px rgba(0,0,0,.35); transition: transform .16s ease; }
  .switch.on { background: #6f9ce8; }
  .switch.on span { transform: translateX(16px); background: white; }
  .switch:focus-visible { outline: 2px solid #9ec1ff; outline-offset: 3px; }
  footer { min-height: 18px; }
  .adjustment-note, .solar { display: flex; align-items: center; justify-content: space-between; min-height: 18px; padding: 0 3px; color: #9298a7; font-size: 9.5px; }
  .adjustment-note button { padding: 3px 7px; border-radius: 6px; background: rgba(121,168,255,.12); color: #a9c8ff; font-size: 9.5px; }
  .reset-wide { width: 100%; padding: 4px; border-radius: 7px; background: rgba(121,168,255,.13); color: #b8d1ff; font-size: 10px; }
  .error { margin: 4px; color: #ffaaa3; font-size: 11px; }
  @media (prefers-reduced-motion: reduce) { * { transition: none !important; } }
</style>
