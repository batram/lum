<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let { settings, monitors = [] } = $props();

  const W = 900;
  const H = 370;
  const PAD = { left: 78, right: 86, top: 92, bottom: 42 };
  const PLOT_W = W - PAD.left - PAD.right;
  const PLOT = { top: 92, height: 224 };
  const hours = Array.from({ length: 25 }, (_, index) => index);

  let svgEl = $state(null);
  let dragging = $state(null);
  let sunTimes = $state(null);
  let solarError = $state(false);
  let now = $state(new Date());
  let dateKey = $state(localDateKey(new Date()));
  let previewMinute = $state(null);
  let requestId = 0;
  let previewTimer;
  let queuedPreviewMinute = null;

  onMount(() => {
    const timer = setInterval(() => {
      now = new Date();
      const nextDateKey = localDateKey(now);
      if (nextDateKey !== dateKey) dateKey = nextDateKey;
    }, 1000);
    const cancel = () => stopPreview();
    const keydown = (event) => event.key === "Escape" && stopPreview();
    window.addEventListener("blur", cancel);
    window.addEventListener("keydown", keydown);
    return () => {
      clearInterval(timer);
      clearTimeout(previewTimer);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("keydown", keydown);
      invoke("set_schedule_preview", { minute: null }).catch(() => {});
    };
  });

  $effect(() => {
    const latitude = Number(settings.location.latitude);
    const longitude = Number(settings.location.longitude);
    if (dateKey && Number.isFinite(latitude) && Number.isFinite(longitude)) loadSunTimes(latitude, longitude);
  });

  async function loadSunTimes(latitude, longitude) {
    const id = ++requestId;
    try {
      const result = await invoke("get_sun_times", { latitude, longitude });
      if (id === requestId) { sunTimes = result; solarError = false; }
    } catch {
      if (id === requestId) solarError = true;
    }
  }

  function toMinute(value) {
    if (!/^\d{2}:\d{2}$/.test(value ?? "")) return 720;
    const [hour, minute] = value.split(":").map(Number);
    return hour * 60 + minute;
  }
  function localDateKey(value) { return `${value.getFullYear()}-${value.getMonth()}-${value.getDate()}`; }
  function wrapMinute(value) { return ((Math.round(value) % 1440) + 1440) % 1440; }
  function xForMinute(minute) { return PAD.left + ((minute >= 1440 ? 1440 : wrapMinute(minute)) / 1440) * PLOT_W; }
  function minuteForX(x) { return wrapMinute(((x - PAD.left) / PLOT_W) * 1440); }
  function yForPercent(value) { return PLOT.top + PLOT.height - (value / 100) * PLOT.height; }
  function yForKelvin(value) { return yForPercent(((value - 1800) / 8200) * 100); }
  function percentForY(y, floor) { return Math.round(Math.max(floor, Math.min(100, ((PLOT.top + PLOT.height - y) / PLOT.height) * 100))); }
  function kelvinForY(y) { return Math.round((1800 + Math.max(0, Math.min(1, (PLOT.top + PLOT.height - y) / PLOT.height)) * 8200) / 100) * 100; }
  function smoothstep(start, end, value) {
    if (end <= start) return value >= end ? 1 : 0;
    const t = Math.max(0, Math.min(1, (value - start) / (end - start)));
    return t * t * (3 - 2 * t);
  }

  function nightAmount(minute) {
    if (!sunTimes) return 0;
    const sunrise = toMinute(settings.fade.use_civil_twilight ? sunTimes.civil_dawn : sunTimes.sunrise);
    const sunset = toMinute(settings.fade.use_civil_twilight ? sunTimes.civil_dusk : sunTimes.sunset);
    const duration = Math.max(5, Number(settings.fade.fade_duration_min));
    const morningEnd = sunrise + Number(settings.fade.morning_offset_min);
    const morningStart = morningEnd - duration;
    const eveningEnd = sunset - Number(settings.fade.evening_offset_min);
    const eveningStart = eveningEnd - duration;
    if (minute < morningStart || minute >= eveningEnd) return 1;
    if (minute < morningEnd) return 1 - smoothstep(morningStart, morningEnd, minute);
    if (minute < eveningStart) return 0;
    return smoothstep(eveningStart, eveningEnd, minute);
  }

  function interpolate(day, night, minute) { return day + (night - day) * nightAmount(minute); }
  function hardwareAt(minute) { return interpolate(settings.brightness.hardware_day_percent, settings.brightness.hardware_night_percent, minute); }
  let minimumGammaPercent = $derived(Math.max(1, Math.min(100, Number(settings.developer.minimum_gamma_percent) || 10)));
  function gammaDisplayPercent(actual) {
    if (minimumGammaPercent >= 100) return 0;
    return Math.round(((Math.max(minimumGammaPercent, Math.min(100, actual)) - minimumGammaPercent) / (100 - minimumGammaPercent)) * 100);
  }
  function gammaActualPercent(display) {
    if (minimumGammaPercent >= 100) return 100;
    return Math.round(minimumGammaPercent + (Math.max(0, Math.min(100, display)) / 100) * (100 - minimumGammaPercent));
  }
  function overlayAt(minute) { return gammaDisplayPercent(interpolate(settings.brightness.overlay_day_percent, settings.brightness.overlay_night_percent, minute)); }
  function temperatureAt(minute) { return interpolate(settings.color.day_temp_k, settings.color.night_temp_k, minute); }
  function pathFor(getValue, getY) {
    let path = "";
    for (let minute = 0; minute <= 1440; minute += 5) path += `${minute ? " L" : "M"}${xForMinute(minute).toFixed(1)},${getY(getValue(minute)).toFixed(1)}`;
    return path;
  }

  let hardwarePath = $derived(sunTimes ? pathFor(hardwareAt, yForPercent) : "");
  let overlayPath = $derived(sunTimes ? pathFor(overlayAt, yForPercent) : "");
  let temperaturePath = $derived(sunTimes ? pathFor(temperatureAt, yForKelvin) : "");
  let currentMinute = $derived(now.getHours() * 60 + now.getMinutes() + now.getSeconds() / 60);
  let lightAnchor = $derived(toMinute(settings.fade.use_civil_twilight ? sunTimes?.civil_dawn : sunTimes?.sunrise));
  let darkAnchor = $derived(toMinute(settings.fade.use_civil_twilight ? sunTimes?.civil_dusk : sunTimes?.sunset));
  let lightMinute = $derived(wrapMinute(lightAnchor + Number(settings.theme.light_offset_min)));
  let darkMinute = $derived(wrapMinute(darkAnchor + Number(settings.theme.dark_offset_min)));
  let sunriseMinute = $derived(toMinute(sunTimes?.sunrise));
  let sunsetMinute = $derived(toMinute(sunTimes?.sunset));
  let morningEndMinute = $derived(lightAnchor + Number(settings.fade.morning_offset_min));
  let eveningEndMinute = $derived(darkAnchor - Number(settings.fade.evening_offset_min));
  let eveningStartMinute = $derived(eveningEndMinute - Math.max(5, Number(settings.fade.fade_duration_min)));
  let dayHandleMinute = $derived(wrapMinute((morningEndMinute + eveningStartMinute) / 2));
  let nightHandleMinute = $derived(wrapMinute((eveningEndMinute + morningEndMinute + 1440) / 2));
  let activeMinute = $derived(previewMinute ?? currentMinute);
  let activeHardware = $derived(Math.round(hardwareAt(activeMinute)));
  let activeOverlay = $derived(Math.round(overlayAt(activeMinute)));
  let activeTemperature = $derived(Math.round(temperatureAt(activeMinute) / 100) * 100);
  let previewThemeDark = $derived(themeDarkAt(activeMinute));
  let capableMonitors = $derived(monitors.filter((monitor) => monitor.supports_brightness));

  function themeDarkAt(minute) {
    if (lightMinute <= darkMinute) return minute < lightMinute || minute >= darkMinute;
    return minute >= darkMinute && minute < lightMinute;
  }
  function formatMinute(minute) { return `${String(Math.floor(wrapMinute(minute) / 60)).padStart(2, "0")}:${String(wrapMinute(minute) % 60).padStart(2, "0")}`; }
  function themeOffsetLabel(value, kind) {
    const anchor = settings.fade.use_civil_twilight
      ? (kind === "light" ? "civil dawn" : "civil dusk")
      : (kind === "light" ? "sunrise" : "sunset");
    const minutes = Math.abs(Number(value));
    if (minutes === 0) return `at ${anchor}`;
    return `${minutes} min ${value < 0 ? "before" : "after"} ${anchor}`;
  }

  function localPoint(event) {
    const point = svgEl.createSVGPoint();
    point.x = event.clientX; point.y = event.clientY;
    return point.matrixTransform(svgEl.getScreenCTM().inverse());
  }
  function startDrag(kind, event) {
    if (event.button !== 0) return;
    dragging = { kind, pointerId: event.pointerId };
    svgEl?.setPointerCapture(event.pointerId);
    if (kind === "preview") updatePreview(minuteForX(localPoint(event).x));
    event.preventDefault(); event.stopPropagation();
  }
  function pointerMove(event) {
    if (!dragging || !svgEl) return;
    const point = localPoint(event);
    const kind = dragging.kind;
    if (kind === "preview") { updatePreview(minuteForX(point.x)); return; }
    if (kind === "theme-light" || kind === "theme-dark") {
      const anchor = kind === "theme-light" ? lightAnchor : darkAnchor;
      let offset = minuteForX(point.x) - anchor;
      if (offset > 720) offset -= 1440;
      if (offset < -720) offset += 1440;
      if (kind === "theme-light") settings.theme.light_offset_min = Math.round(offset);
      else settings.theme.dark_offset_min = Math.round(offset);
      return;
    }
    if (kind === "hardware-day") settings.brightness.hardware_day_percent = percentForY(point.y, 0);
    if (kind === "hardware-night") settings.brightness.hardware_night_percent = percentForY(point.y, 0);
    if (kind === "overlay-day") settings.brightness.overlay_day_percent = gammaActualPercent(percentForY(point.y, 0));
    if (kind === "overlay-night") settings.brightness.overlay_night_percent = gammaActualPercent(percentForY(point.y, 0));
    if (kind === "temperature-day") settings.color.day_temp_k = kelvinForY(point.y);
    if (kind === "temperature-night") settings.color.night_temp_k = kelvinForY(point.y);
  }
  function stopDrag(event) {
    if (!dragging) return;
    const wasPreview = dragging.kind === "preview";
    if (svgEl?.hasPointerCapture(dragging.pointerId)) svgEl.releasePointerCapture(dragging.pointerId);
    dragging = null;
    if (wasPreview) stopPreview();
  }
  function updatePreview(minute) {
    previewMinute = minute;
    queuedPreviewMinute = minute;
    if (previewTimer) return;
    previewTimer = setTimeout(() => {
      previewTimer = null;
      const next = queuedPreviewMinute;
      queuedPreviewMinute = null;
      invoke("set_schedule_preview", { minute: next }).catch(() => {});
    }, 80);
  }
  function stopPreview() {
    if (previewMinute === null && dragging?.kind !== "preview") return;
    clearTimeout(previewTimer); previewTimer = null; queuedPreviewMinute = null;
    previewMinute = null;
    if (dragging?.kind === "preview") dragging = null;
    invoke("set_schedule_preview", { minute: null }).catch(() => {});
  }

  function handleValueKey(kind, event) {
    const direction = ["ArrowUp", "ArrowRight"].includes(event.key) ? 1 : ["ArrowDown", "ArrowLeft"].includes(event.key) ? -1 : 0;
    if (!direction && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    const percent = (value, floor) => event.key === "Home" ? floor : event.key === "End" ? 100 : Math.max(floor, Math.min(100, value + direction));
    const kelvin = (value) => event.key === "Home" ? 1800 : event.key === "End" ? 10000 : Math.max(1800, Math.min(10000, value + direction * 100));
    if (kind === "hardware-day") settings.brightness.hardware_day_percent = percent(settings.brightness.hardware_day_percent, 0);
    if (kind === "hardware-night") settings.brightness.hardware_night_percent = percent(settings.brightness.hardware_night_percent, 0);
    if (kind === "overlay-day") settings.brightness.overlay_day_percent = gammaActualPercent(percent(gammaDisplayPercent(settings.brightness.overlay_day_percent), 0));
    if (kind === "overlay-night") settings.brightness.overlay_night_percent = gammaActualPercent(percent(gammaDisplayPercent(settings.brightness.overlay_night_percent), 0));
    if (kind === "temperature-day") settings.color.day_temp_k = kelvin(settings.color.day_temp_k);
    if (kind === "temperature-night") settings.color.night_temp_k = kelvin(settings.color.night_temp_k);
  }
  function handleThemeKey(kind, event) {
    const direction = ["ArrowUp", "ArrowRight"].includes(event.key) ? 5 : ["ArrowDown", "ArrowLeft"].includes(event.key) ? -5 : 0;
    if (!direction && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    const current = kind === "theme-light" ? settings.theme.light_offset_min : settings.theme.dark_offset_min;
    const next = event.key === "Home" ? -720 : event.key === "End" ? 720 : Math.max(-720, Math.min(720, current + direction));
    if (kind === "theme-light") settings.theme.light_offset_min = next;
    else settings.theme.dark_offset_min = next;
  }
  function handlePreviewKey(event) {
    const direction = event.key === "ArrowRight" ? 15 : event.key === "ArrowLeft" ? -15 : 0;
    if (!direction && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    const next = event.key === "Home" ? 0 : event.key === "End" ? 1439 : wrapMinute((previewMinute ?? currentMinute) + direction);
    updatePreview(next);
  }
</script>

<section class="schedule-card">
  <div class="schedule-header">
    <div><span class="eyebrow">{previewMinute === null ? "Right now" : "Previewing"}</span><strong>{previewMinute === null ? now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }) : formatMinute(previewMinute)}</strong></div>
    <div class="preview-values"><span><i class="hardware"></i>{activeHardware}% hardware</span><span><i class="overlay"></i>{activeOverlay}% overlay</span><span><i class="temperature"></i>{activeTemperature}K</span><span>{previewThemeDark ? "Dark" : "Light"} theme</span></div>
  </div>
  {#if solarError}<p class="error">Could not calculate solar times for this location.</p>{/if}

  <svg bind:this={svgEl} viewBox={`0 0 ${W} ${H}`} class="chart" role="application" aria-label="24-hour hardware brightness, overlay dimming, and temperature schedule" onpointermove={pointerMove} onpointerup={stopDrag} onpointercancel={stopDrag}>
    <rect x={PAD.left} y={PLOT.top} width={PLOT_W} height={PLOT.height} class="lane-bg" />
    <line x1={PAD.left} y1={PLOT.top} x2={PAD.left} y2={PLOT.top + PLOT.height} class="axis-rail brightness" />
    <line x1={W - PAD.right} y1={PLOT.top} x2={W - PAD.right} y2={PLOT.top + PLOT.height} class="axis-rail temperature" />
    {#each hours as hour}
      {#if hour % 3 === 0}
        <line x1={xForMinute(hour * 60)} y1={PLOT.top} x2={xForMinute(hour * 60)} y2={PLOT.top + PLOT.height} class="grid vertical" />
        <text x={xForMinute(hour * 60)} y={H - 13} class="axis">{String(hour).padStart(2, "0")}:00</text>
      {/if}
    {/each}
    {#each [0, 25, 50, 75, 100] as value}
      <line x1={PAD.left} y1={yForPercent(value)} x2={W - PAD.right} y2={yForPercent(value)} class="grid horizontal" />
      <text x={PAD.left - 10} y={yForPercent(value) + 4} class="axis y">{value}%</text>
      <text x={W - PAD.right + 10} y={yForPercent(value) + 4} class="axis kelvin">{Math.round(1800 + value * 82)}K</text>
    {/each}
    <text x={W - PAD.right + 10} y={yForPercent(100) + 18} class="temperature-note">warm</text>
    <text x={W - PAD.right + 10} y={yForPercent(0) - 8} class="temperature-note">cool</text>
    <text x={PAD.left - 10} y={PLOT.top - 15} class="axis-title left">Brightness</text>
    <text x={W - PAD.right + 10} y={PLOT.top - 15} class="axis-title right">Temperature</text>
    <rect x={PAD.left} y={PLOT.top} width={PLOT_W} height={PLOT.height} class="preview-hit" role="slider" aria-label="Preview schedule time" aria-valuemin="0" aria-valuemax="1439" aria-valuenow={Math.round(activeMinute)} tabindex="0" onkeydown={handlePreviewKey} onkeyup={stopPreview} onpointerdown={(event) => startDrag("preview", event)} />

    {#if sunTimes}
      <path d={hardwarePath} class="curve hardware" />
      <path d={overlayPath} class="curve overlay" />
      <path d={temperaturePath} class="curve temperature" />

      {#each [
        ["hardware-night", "Hardware night brightness", nightHandleMinute, yForPercent(settings.brightness.hardware_night_percent), settings.brightness.hardware_night_percent, "hardware"],
        ["hardware-day", "Hardware day brightness", dayHandleMinute, yForPercent(settings.brightness.hardware_day_percent), settings.brightness.hardware_day_percent, "hardware"],
        ["overlay-night", "Overlay night brightness", wrapMinute(nightHandleMinute + 18), yForPercent(gammaDisplayPercent(settings.brightness.overlay_night_percent)), gammaDisplayPercent(settings.brightness.overlay_night_percent), "overlay"],
        ["overlay-day", "Overlay day brightness", wrapMinute(dayHandleMinute + 18), yForPercent(gammaDisplayPercent(settings.brightness.overlay_day_percent)), gammaDisplayPercent(settings.brightness.overlay_day_percent), "overlay"],
      ] as item}
        <g class={`handle ${item[5]}`} role="slider" aria-label={item[1]} aria-valuemin="0" aria-valuemax="100" aria-valuenow={item[4]} tabindex="0" onkeydown={(event) => handleValueKey(item[0], event)} onpointerdown={(event) => startDrag(item[0], event)}>
          <circle cx={xForMinute(item[2])} cy={item[3]} r="10" />
          {#if dragging?.kind === item[0]}<g class={`drag-value ${item[5]}`} transform={`translate(${xForMinute(item[2])} ${item[3]})`}><rect x="-23" y="-40" width="46" height="24" rx="7" /><text y="-24">{item[4]}%</text></g>{/if}
          <title>{item[1]}: {item[4]}%</title>
        </g>
      {/each}
      {#each [
        ["temperature-night", "Night temperature", wrapMinute(nightHandleMinute + 36), settings.color.night_temp_k],
        ["temperature-day", "Day temperature", wrapMinute(dayHandleMinute + 36), settings.color.day_temp_k],
      ] as item}
        <g class="handle temperature" role="slider" aria-label={item[1]} aria-valuemin="1800" aria-valuemax="10000" aria-valuenow={item[3]} tabindex="0" onkeydown={(event) => handleValueKey(item[0], event)} onpointerdown={(event) => startDrag(item[0], event)}>
          <circle cx={xForMinute(item[2])} cy={yForKelvin(item[3])} r="10" />
          {#if dragging?.kind === item[0]}<g class="drag-value temperature" transform={`translate(${xForMinute(item[2])} ${yForKelvin(item[3])})`}><rect x="-31" y="-40" width="62" height="24" rx="7" /><text y="-24">{item[3]}K</text></g>{/if}
          <title>{item[1]}: {item[3]}K</title>
        </g>
      {/each}

      <g class="solar-marker sunrise"><line x1={xForMinute(sunriseMinute)} y1={PLOT.top - 6} x2={xForMinute(sunriseMinute)} y2={PLOT.top + PLOT.height} /><circle cx={xForMinute(sunriseMinute)} cy={PLOT.top - 9} r="4" /><text x={xForMinute(sunriseMinute)} y={PLOT.top - 48}>Sunrise {formatMinute(sunriseMinute)}</text></g>
      <g class="solar-marker sunset"><line x1={xForMinute(sunsetMinute)} y1={PLOT.top - 6} x2={xForMinute(sunsetMinute)} y2={PLOT.top + PLOT.height} /><circle cx={xForMinute(sunsetMinute)} cy={PLOT.top - 9} r="4" /><text x={xForMinute(sunsetMinute)} y={PLOT.top - 48}>Sunset {formatMinute(sunsetMinute)}</text></g>

      {#if settings.theme.auto_switch}
        <g class="theme-marker light" role="slider" aria-label="Go light theme time" aria-valuemin="-720" aria-valuemax="720" aria-valuenow={settings.theme.light_offset_min} tabindex="0" onkeydown={(event) => handleThemeKey("theme-light", event)} onpointerdown={(event) => startDrag("theme-light", event)}><line class="marker-hit" x1={xForMinute(lightMinute)} y1={PLOT.top - 24} x2={xForMinute(lightMinute)} y2={PLOT.top + PLOT.height} /><line x1={xForMinute(lightMinute)} y1={PLOT.top - 6} x2={xForMinute(lightMinute)} y2={PLOT.top + PLOT.height} /><path d={`M${xForMinute(lightMinute)-7},${PLOT.top-8} L${xForMinute(lightMinute)+7},${PLOT.top-8} L${xForMinute(lightMinute)},${PLOT.top} Z`} /><text x={xForMinute(lightMinute)} y={PLOT.top - 29}>{dragging?.kind === "theme-light" ? `Go light · ${themeOffsetLabel(settings.theme.light_offset_min, "light")}` : "Go light"}</text><title>Go light · {themeOffsetLabel(settings.theme.light_offset_min, "light")}</title></g>
        <g class="theme-marker dark" role="slider" aria-label="Go dark theme time" aria-valuemin="-720" aria-valuemax="720" aria-valuenow={settings.theme.dark_offset_min} tabindex="0" onkeydown={(event) => handleThemeKey("theme-dark", event)} onpointerdown={(event) => startDrag("theme-dark", event)}><line class="marker-hit" x1={xForMinute(darkMinute)} y1={PLOT.top - 24} x2={xForMinute(darkMinute)} y2={PLOT.top + PLOT.height} /><line x1={xForMinute(darkMinute)} y1={PLOT.top - 6} x2={xForMinute(darkMinute)} y2={PLOT.top + PLOT.height} /><path d={`M${xForMinute(darkMinute)-7},${PLOT.top-8} L${xForMinute(darkMinute)+7},${PLOT.top-8} L${xForMinute(darkMinute)},${PLOT.top} Z`} /><text x={xForMinute(darkMinute)} y={PLOT.top - 29}>{dragging?.kind === "theme-dark" ? `Go dark · ${themeOffsetLabel(settings.theme.dark_offset_min, "dark")}` : "Go dark"}</text><title>Go dark · {themeOffsetLabel(settings.theme.dark_offset_min, "dark")}</title></g>
      {/if}
    {/if}

    <line x1={xForMinute(activeMinute)} y1={PLOT.top - 5} x2={xForMinute(activeMinute)} y2={PLOT.top + PLOT.height} class:preview={previewMinute !== null} class="time-cursor" />
    <circle cx={xForMinute(activeMinute)} cy={PLOT.top - 7} r="4" class:preview={previewMinute !== null} class="time-dot" />
  </svg>

  <div class="legend"><span><i class="hardware"></i>Hardware</span><span><i class="overlay"></i>Overlay</span><span><i class="temperature"></i>Temperature</span><span class="hint">Drag curves vertically · drag empty chart space to preview time</span></div>
  <details class="monitor-status"><summary><strong>{capableMonitors.length} of {monitors.length} displays</strong> support hardware brightness</summary><div>{#each monitors as monitor}<span><b>{monitor.description || `Display ${monitor.index + 1}`}</b><em class:supported={monitor.supports_brightness}>{monitor.supports_brightness ? "DDC/CI hardware + overlay" : "Overlay only"}</em></span>{:else}<p>No displays were reported. Overlay and temperature control remain available.</p>{/each}</div></details>
</section>

<style>
  .schedule-card{border:1px solid #353845;border-radius:15px;background:#20222b;overflow:hidden}.schedule-header{display:flex;justify-content:space-between;align-items:center;gap:18px;padding:17px 20px 5px}.schedule-header>div:first-child{display:grid;gap:3px}.eyebrow{color:#7e9edb;font-size:10px;font-weight:650;letter-spacing:.08em;text-transform:uppercase}.schedule-header strong{font-size:20px;font-variant-numeric:tabular-nums}.preview-values{display:flex;flex-wrap:wrap;justify-content:flex-end;gap:7px 14px;color:#a1a6b2;font-size:10.5px}.preview-values span,.legend span{display:flex;align-items:center;gap:6px}.preview-values i,.legend i{width:16px;height:3px;border-radius:4px}.hardware{background:#4d8df7}.overlay{background:#57c9c1}.temperature{background:#f09a55}.error{margin:5px 20px;color:#ffaaa3;font-size:11px}.chart{display:block;width:100%;height:auto;min-height:300px;touch-action:none;user-select:none}.lane-bg{fill:#1b1d25}.axis-rail{stroke-width:2;pointer-events:none}.axis-rail.brightness{stroke:#668fd4}.axis-rail.temperature{stroke:#f09a55}.grid{stroke:#30333e;stroke-width:1;pointer-events:none}.horizontal{stroke-dasharray:3 5}.axis{fill:#777e8d;font:10px system-ui;text-anchor:middle}.axis.y{fill:#829bc7;text-anchor:end}.axis.kelvin{fill:#c98a5b;text-anchor:start}.temperature-note{fill:#c98a5b;font:9px system-ui;text-anchor:start}.axis-title{font:600 11px system-ui}.axis-title.left{fill:#8da9dc;text-anchor:end}.axis-title.right{fill:#e69a61;text-anchor:start}.curve{fill:none;stroke-width:4;stroke-linecap:round;stroke-linejoin:round;pointer-events:none}.curve.hardware{stroke:#4d8df7}.curve.overlay{stroke:#57c9c1}.curve.temperature{stroke:#f09a55}.handle{cursor:ns-resize}.handle circle{fill:#20222b;stroke-width:4;filter:drop-shadow(0 2px 3px rgba(0,0,0,.35))}.handle.hardware circle{stroke:#4d8df7}.handle.overlay circle{stroke:#57c9c1}.handle.temperature circle{stroke:#f09a55}.handle:focus{outline:none}.handle:focus circle{filter:drop-shadow(0 0 6px rgba(160,198,255,.9))}.drag-value{pointer-events:none}.drag-value rect{fill:#272a34;stroke-width:1.5;filter:drop-shadow(0 3px 5px rgba(0,0,0,.4))}.drag-value text{fill:#f5f6f9;font:650 11px system-ui;text-anchor:middle}.drag-value.hardware rect{stroke:#4d8df7}.drag-value.overlay rect{stroke:#57c9c1}.drag-value.temperature rect{stroke:#f09a55}.theme-marker{cursor:ew-resize}.theme-marker line{stroke-width:1.5;stroke-dasharray:4 4}.theme-marker .marker-hit{stroke:transparent;stroke-width:22;stroke-dasharray:none;pointer-events:stroke}.theme-marker path{stroke:none}.theme-marker text,.solar-marker text{font:600 10px system-ui;text-anchor:middle}.theme-marker.light line:not(.marker-hit){stroke:#f3bc58}.theme-marker.light path,.theme-marker.light text{fill:#f3bc58}.theme-marker.dark line:not(.marker-hit){stroke:#a992ff}.theme-marker.dark path,.theme-marker.dark text{fill:#a992ff}.solar-marker{pointer-events:none}.solar-marker line{stroke-width:1;stroke-dasharray:2 5;opacity:.7}.solar-marker.sunrise line{stroke:#ffd27a}.solar-marker.sunrise circle,.solar-marker.sunrise text{fill:#ffd27a}.solar-marker.sunset line{stroke:#f08a61}.solar-marker.sunset circle,.solar-marker.sunset text{fill:#f08a61}.preview-hit{fill:transparent;cursor:ew-resize}.time-cursor{stroke:#62d6a8;stroke-width:1.5;pointer-events:none}.time-cursor.preview{stroke:#fff;stroke-width:2}.time-dot{fill:#62d6a8;pointer-events:none}.time-dot.preview{fill:#fff}.legend{display:flex;align-items:center;gap:8px 18px;padding:0 20px 14px;color:#8d93a1;font-size:10.5px}.hint{margin-left:auto}.monitor-status{border-top:1px solid #30333e;color:#898f9d;font-size:11px}.monitor-status summary{padding:12px 20px;cursor:pointer;list-style-position:inside}.monitor-status summary strong{color:#cdd1d9}.monitor-status>div{display:grid;padding:0 20px 13px}.monitor-status>div span{display:flex;justify-content:space-between;padding:7px 0;border-top:1px solid #2d3039}.monitor-status b{color:#bfc3cc}.monitor-status em{color:#d4937e;font-style:normal}.monitor-status em.supported{color:#72c497}.monitor-status p{margin:0}@media(max-width:760px){.schedule-header{align-items:flex-start;flex-direction:column}.preview-values{justify-content:flex-start}.legend{flex-wrap:wrap}.hint{width:100%;margin-left:0}}
</style>
