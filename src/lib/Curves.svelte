<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let { settings } = $props();

  const W = 900;
  const H = 330;
  const PAD = { left: 54, right: 22, top: 46, bottom: 42 };
  const PLOT_W = W - PAD.left - PAD.right;
  const PLOT_H = H - PAD.top - PAD.bottom;
  const hours = Array.from({ length: 25 }, (_, i) => i);

  let svgEl = $state(null);
  let dragging = $state(null);
  let sunTimes = $state(null);
  let solarError = $state(false);
  let now = $state(new Date());
  let requestId = 0;

  onMount(() => {
    const timer = setInterval(() => (now = new Date()), 1000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    const latitude = Number(settings.location.latitude);
    const longitude = Number(settings.location.longitude);
    if (!Number.isFinite(latitude) || !Number.isFinite(longitude)) return;
    loadSunTimes(latitude, longitude);
  });

  async function loadSunTimes(latitude, longitude) {
    const id = ++requestId;
    try {
      const result = await invoke("get_sun_times", { latitude, longitude });
      if (id === requestId) {
        sunTimes = result;
        solarError = false;
      }
    } catch {
      if (id === requestId) solarError = true;
    }
  }

  function toMinute(value) {
    if (!/^\d{2}:\d{2}$/.test(value ?? "")) return 720;
    const [hour, minute] = value.split(":").map(Number);
    return hour * 60 + minute;
  }

  function xForMinute(minute) {
    return PAD.left + (minute / 1440) * PLOT_W;
  }

  function yForPercent(percent) {
    return PAD.top + PLOT_H - (percent / 100) * PLOT_H;
  }

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

  function brightnessAt(minute) {
    const night = nightAmount(minute);
    return settings.brightness.day_percent * (1 - night) + settings.brightness.night_percent * night;
  }

  function warmthAt(minute) {
    const night = nightAmount(minute);
    const day = kelvinToWarmth(settings.color.day_temp_k);
    const nightValue = kelvinToWarmth(settings.color.night_temp_k);
    return day * (1 - night) + nightValue * night;
  }

  function kelvinToWarmth(kelvin) {
    return ((10000 - kelvin) / 8200) * 100;
  }

  function warmthToKelvin(warmth) {
    return Math.round((10000 - (warmth / 100) * 8200) / 100) * 100;
  }

  function pathFor(getValue) {
    let path = "";
    for (let minute = 0; minute <= 1440; minute += 5) {
      path += `${minute ? " L" : "M"}${xForMinute(minute).toFixed(1)},${yForPercent(getValue(minute)).toFixed(1)}`;
    }
    return path;
  }

  let brightnessPath = $derived(sunTimes ? pathFor(brightnessAt) : "");
  let warmthPath = $derived(sunTimes ? pathFor(warmthAt) : "");
  let currentMinute = $derived(now.getHours() * 60 + now.getMinutes() + now.getSeconds() / 60);

  function startDrag(kind, event) {
    if (event.button !== 0) return;
    dragging = { kind, pointerId: event.pointerId };
    svgEl?.setPointerCapture(event.pointerId);
    event.preventDefault();
  }

  function pointerMove(event) {
    if (!dragging || !svgEl) return;
    const point = svgEl.createSVGPoint();
    point.x = event.clientX;
    point.y = event.clientY;
    const local = point.matrixTransform(svgEl.getScreenCTM().inverse());
    const percent = Math.round(Math.max(0, Math.min(100, ((PAD.top + PLOT_H - local.y) / PLOT_H) * 100)));

    if (dragging.kind === "brightness-day") settings.brightness.day_percent = Math.max(30, percent);
    if (dragging.kind === "brightness-night") settings.brightness.night_percent = Math.max(10, percent);
    if (dragging.kind === "warmth-day") settings.color.day_temp_k = Math.max(4000, Math.min(10000, warmthToKelvin(percent)));
    if (dragging.kind === "warmth-night") settings.color.night_temp_k = Math.max(1800, Math.min(5000, warmthToKelvin(percent)));
  }

  function stopDrag(event) {
    if (!dragging) return;
    if (svgEl?.hasPointerCapture(dragging.pointerId)) svgEl.releasePointerCapture(dragging.pointerId);
    dragging = null;
  }

  function handleKey(kind, event) {
    const direction = event.key === "ArrowUp" || event.key === "ArrowRight" ? 1 : event.key === "ArrowDown" || event.key === "ArrowLeft" ? -1 : 0;
    if (!direction && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    if (kind === "brightness-day") settings.brightness.day_percent = event.key === "Home" ? 30 : event.key === "End" ? 100 : Math.max(30, Math.min(100, settings.brightness.day_percent + direction));
    if (kind === "brightness-night") settings.brightness.night_percent = event.key === "Home" ? 10 : event.key === "End" ? 100 : Math.max(10, Math.min(100, settings.brightness.night_percent + direction));
    if (kind === "warmth-day") settings.color.day_temp_k = event.key === "Home" ? 4000 : event.key === "End" ? 10000 : Math.max(4000, Math.min(10000, settings.color.day_temp_k + direction * 100));
    if (kind === "warmth-night") settings.color.night_temp_k = event.key === "Home" ? 1800 : event.key === "End" ? 5000 : Math.max(1800, Math.min(5000, settings.color.night_temp_k + direction * 100));
  }

  function formatClock(date) {
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }
</script>

<div class="schedule-card">
  <div class="schedule-header">
    <div>
      <span class="eyebrow">Today</span>
      <strong>{formatClock(now)}</strong>
    </div>
    <div class="solar-times" aria-live="polite">
      <span><i class="sunrise-dot"></i>Sunrise <b>{sunTimes?.sunrise ?? "--:--"}</b></span>
      <span><i class="sunset-dot"></i>Sunset <b>{sunTimes?.sunset ?? "--:--"}</b></span>
    </div>
  </div>

  {#if solarError}<p class="error">Could not calculate solar times for this location.</p>{/if}

  <svg bind:this={svgEl} viewBox="0 0 {W} {H}" class="chart" role="application" aria-label="Draggable 24-hour brightness and color schedule" onpointermove={pointerMove} onpointerup={stopDrag} onpointercancel={stopDrag}>
    {#each [0, 25, 50, 75, 100] as value}
      <line x1={PAD.left} y1={yForPercent(value)} x2={W - PAD.right} y2={yForPercent(value)} class="grid horizontal" />
      <text x={PAD.left - 10} y={yForPercent(value) + 4} class="axis y">{value}</text>
    {/each}
    {#each hours as hour}
      {#if hour % 3 === 0}
        <line x1={xForMinute(hour * 60)} y1={PAD.top} x2={xForMinute(hour * 60)} y2={PAD.top + PLOT_H} class="grid" />
        <text x={xForMinute(hour * 60)} y={H - 14} class="axis">{String(hour).padStart(2, "0")}:00</text>
      {/if}
    {/each}

    {#if sunTimes}
      <rect x={PAD.left} y={PAD.top} width={xForMinute(toMinute(sunTimes.sunrise)) - PAD.left} height={PLOT_H} class="night" />
      <rect x={xForMinute(toMinute(sunTimes.sunset))} y={PAD.top} width={W - PAD.right - xForMinute(toMinute(sunTimes.sunset))} height={PLOT_H} class="night" />
      <line x1={xForMinute(toMinute(sunTimes.sunrise))} y1={PAD.top} x2={xForMinute(toMinute(sunTimes.sunrise))} y2={PAD.top + PLOT_H} class="solar sunrise" />
      <line x1={xForMinute(toMinute(sunTimes.sunset))} y1={PAD.top} x2={xForMinute(toMinute(sunTimes.sunset))} y2={PAD.top + PLOT_H} class="solar sunset" />
      <path d={brightnessPath} class="curve brightness" />
      <path d={warmthPath} class="curve warmth" />

      <g class="handle brightness" role="slider" aria-label="Night brightness" aria-valuemin="10" aria-valuemax="100" aria-valuenow={settings.brightness.night_percent} tabindex="0" onkeydown={(event) => handleKey("brightness-night", event)} onpointerdown={(event) => startDrag("brightness-night", event)}>
        <circle cx={xForMinute(60)} cy={yForPercent(settings.brightness.night_percent)} r="11" /><title>Night brightness: {settings.brightness.night_percent}%</title>
      </g>
      <g class="handle brightness" role="slider" aria-label="Day brightness" aria-valuemin="30" aria-valuemax="100" aria-valuenow={settings.brightness.day_percent} tabindex="0" onkeydown={(event) => handleKey("brightness-day", event)} onpointerdown={(event) => startDrag("brightness-day", event)}>
        <circle cx={xForMinute(720)} cy={yForPercent(settings.brightness.day_percent)} r="11" /><title>Day brightness: {settings.brightness.day_percent}%</title>
      </g>
      <g class="handle warmth" role="slider" aria-label="Night color warmth" aria-valuemin="1800" aria-valuemax="5000" aria-valuenow={settings.color.night_temp_k} tabindex="0" onkeydown={(event) => handleKey("warmth-night", event)} onpointerdown={(event) => startDrag("warmth-night", event)}>
        <circle cx={xForMinute(90)} cy={yForPercent(kelvinToWarmth(settings.color.night_temp_k))} r="11" /><title>Night color: {settings.color.night_temp_k}K</title>
      </g>
      <g class="handle warmth" role="slider" aria-label="Day color warmth" aria-valuemin="4000" aria-valuemax="10000" aria-valuenow={settings.color.day_temp_k} tabindex="0" onkeydown={(event) => handleKey("warmth-day", event)} onpointerdown={(event) => startDrag("warmth-day", event)}>
        <circle cx={xForMinute(750)} cy={yForPercent(kelvinToWarmth(settings.color.day_temp_k))} r="11" /><title>Day color: {settings.color.day_temp_k}K</title>
      </g>
    {/if}

    <line x1={xForMinute(currentMinute)} y1={PAD.top - 8} x2={xForMinute(currentMinute)} y2={PAD.top + PLOT_H} class="current" />
    <circle cx={xForMinute(currentMinute)} cy={PAD.top - 10} r="4" class="current-dot" />
  </svg>

  <div class="legend">
    <span><i class="key brightness"></i>Monitor brightness <b>{settings.brightness.night_percent}% / {settings.brightness.day_percent}%</b></span>
    <span><i class="key warmth"></i>Red hue / gamma <b>{settings.color.night_temp_k}K / {settings.color.day_temp_k}K</b></span>
    <span class="drag-hint">Drag any circle up or down</span>
  </div>
</div>

<style>
  .schedule-card { border: 1px solid #dfe3ea; border-radius: 14px; background: #fff; overflow: hidden; box-shadow: 0 8px 30px rgba(20, 28, 45, .06); }
  .schedule-header { display: flex; justify-content: space-between; align-items: center; gap: 1rem; padding: 1rem 1.2rem .4rem; }
  .schedule-header > div:first-child { display: flex; flex-direction: column; }
  .eyebrow { color: #7a8394; font-size: .72rem; text-transform: uppercase; letter-spacing: .08em; }
  .schedule-header strong { font-size: 1.35rem; font-variant-numeric: tabular-nums; }
  .solar-times { display: flex; gap: 1rem; font-size: .78rem; color: #687184; }
  .solar-times span { display: flex; align-items: center; gap: .35rem; }
  .solar-times b { color: inherit; font-variant-numeric: tabular-nums; }
  .sunrise-dot, .sunset-dot { width: 8px; height: 8px; border-radius: 50%; background: #f7b731; }
  .sunset-dot { background: #8c6ff7; }
  .error { margin: .2rem 1.2rem; color: #b42318; font-size: .78rem; }
  .chart { display: block; width: 100%; height: auto; min-height: 230px; touch-action: none; user-select: none; }
  .grid { stroke: #edf0f5; stroke-width: 1; }
  .horizontal { stroke-dasharray: 3 4; }
  .axis { fill: #8a93a3; font: 11px system-ui; text-anchor: middle; }
  .axis.y { text-anchor: end; }
  .night { fill: #4b5875; opacity: .07; pointer-events: none; }
  .solar { stroke-width: 1.5; stroke-dasharray: 5 4; pointer-events: none; }
  .solar.sunrise { stroke: #e9a31b; } .solar.sunset { stroke: #7658df; }
  .curve { fill: none; stroke-width: 4; stroke-linecap: round; stroke-linejoin: round; pointer-events: none; }
  .curve.brightness { stroke: #3578e5; } .curve.warmth { stroke: #ef7b45; }
  .handle { cursor: ns-resize; }
  .handle:focus { outline: none; }
  .handle:focus circle { filter: drop-shadow(0 0 5px rgba(142,184,255,.9)); }
  .handle circle { fill: white; stroke-width: 4; filter: drop-shadow(0 2px 3px rgba(0,0,0,.18)); }
  .handle.brightness circle { stroke: #3578e5; } .handle.warmth circle { stroke: #ef7b45; }
  .current { stroke: #14a673; stroke-width: 2; pointer-events: none; }
  .current-dot { fill: #14a673; pointer-events: none; }
  .legend { display: flex; flex-wrap: wrap; gap: .6rem 1.2rem; align-items: center; padding: .6rem 1.2rem 1rem; color: #687184; font-size: .78rem; }
  .legend span { display: flex; gap: .4rem; align-items: center; }
  .legend b { color: #30384a; font-variant-numeric: tabular-nums; }
  .key { width: 20px; height: 4px; border-radius: 3px; }
  .key.brightness { background: #3578e5; } .key.warmth { background: #ef7b45; }
  .drag-hint { margin-left: auto; color: #8a93a3; }
  @media (max-width: 640px) { .schedule-header, .solar-times { align-items: flex-start; flex-direction: column; } .solar-times { gap: .35rem; } .drag-hint { margin-left: 0; } }
  @media (prefers-color-scheme: dark) {
    .schedule-card { background: #202231; border-color: #373a4d; box-shadow: none; }
    .grid { stroke: #343748; } .axis { fill: #858a9b; } .night { fill: #02030a; opacity: .18; }
    .handle circle { fill: #202231; } .legend b { color: #e7e9ee; }
  }
</style>
