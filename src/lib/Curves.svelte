<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let { settings, onThemeChange = () => {} } = $props();

  const W = 800;
  const H = 280;
  const PAD_L = 50;
  const PAD_R = 20;
  const PAD_T = 20;
  const PAD_B = 50;
  const PLOT_W = W - PAD_L - PAD_R;
  const PLOT_H = H - PAD_T - PAD_B;

  let sunTimes = $state(null);
  let dragging = $state(null); // { type: 'brightness'|'color', x: number }
  let svgEl = $state(null);
  let currentTime = $state(new Date());

  // Update current time every second
  onMount(async () => {
    try {
      sunTimes = await invoke("get_sun_times");
    } catch {
      sunTimes = { sunrise: "06:30", sunset: "20:00" };
    }

    const timer = setInterval(() => {
      currentTime = new Date();
    }, 1000);

    return () => clearInterval(timer);
  });

  // Convert "HH:MM" to minutes from midnight
  function toMin(t) {
    if (!t) return 720;
    const [h, m] = t.split(":").map(Number);
    return h * 60 + (m || 0);
  }

  // Minutes → x pixel
  function minX(min) {
    return PAD_L + (min / 1440) * PLOT_W;
  }

  // Value 0-100 → y pixel (inverted)
  function valY(v) {
    return PAD_T + PLOT_H - (v / 100) * PLOT_H;
  }

  // Pixel → value
  function yToVal(y) {
    return Math.max(0, Math.min(100, ((PAD_T + PLOT_H - y) / PLOT_H) * 100));
  }

  // Smoothstep interpolation
  function smoothstep(edge0, edge1, x) {
    const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0)));
    return t * t * (3 - 2 * t);
  }

  // Compute intensity at a given minute (0=full day, 1=full night)
  function intensityAt(min) {
    if (!sunTimes || !settings) return 0;
    const sunrise = toMin(sunTimes.sunrise);
    const sunset = toMin(sunTimes.sunset);
    const fadeDur = settings.fade.fade_duration_min;
    const eveOff = settings.fade.evening_offset_min;
    const mornOff = settings.fade.morning_offset_min;

    // Evening fade: starts at sunset - eveOff - fadeDur, ends at sunset - eveOff
    const eveStart = sunset - eveOff - fadeDur;
    const eveEnd = sunset - eveOff;
    // Morning fade: starts at sunrise + mornOff - fadeDur, ends at sunrise + mornOff
    const mornStart = sunrise + mornOff - fadeDur;
    const mornEnd = sunrise + mornOff;

    if (min >= eveEnd || min < mornStart) return 1; // night
    if (min >= mornEnd && min < eveStart) return 0; // day
    if (min >= eveStart && min < eveEnd) return smoothstep(eveStart, eveEnd, min); // evening fade
    if (min >= mornStart && min < mornEnd) return 1 - smoothstep(mornStart, mornEnd, min); // morning fade
    return 0;
  }

  // Generate SVG path for a curve
  function curvePath(getVal) {
    const step = 5; // every 5 minutes for smoother curves
    let d = "";
    for (let min = 0; min <= 1440; min += step) {
      const x = minX(min);
      const y = valY(getVal(min));
      d += min === 0 ? `M${x},${y}` : ` L${x},${y}`;
    }
    return d;
  }

  // Brightness curve value at minute
  function brightnessVal(min) {
    if (!settings) return 100;
    const t = intensityAt(min);
    return settings.brightness.day_percent + (settings.brightness.night_percent - settings.brightness.day_percent) * t;
  }

  // Color/gamma curve value at minute (normalized: day=100%, night=0%)
  function colorVal(min) {
    const t = intensityAt(min);
    return 100 - t * 100; // 100% = full day temp, 0% = full night temp (warm)
  }

  let brightnessPath = $derived(settings ? curvePath(brightnessVal) : "");
  let colorPath = $derived(settings ? curvePath(colorVal) : "");

  // Current time marker
  let currentMinute = $derived(() => {
    const h = currentTime.getHours();
    const m = currentTime.getMinutes();
    return h * 60 + m;
  });

  let currentX = $derived(minX(currentMinute()));

  // Drag handling for curves
  function onPointerDown(type, e) {
    if (e.button !== 0) return; // Left button only
    dragging = { type, x: e.clientX };
    e.target.setPointerCapture?.(e.pointerId);
    e.preventDefault();
  }

  function onPointerMove(e) {
    if (!dragging || !svgEl || !settings) return;
    
    const rect = svgEl.getBoundingClientRect();
    const py = e.clientY - rect.top;
    const val = Math.round(yToVal(py));

    if (dragging.type === 'brightness-day') {
      settings.brightness.day_percent = Math.max(30, Math.min(100, val));
    } else if (dragging.type === 'brightness-night') {
      settings.brightness.night_percent = Math.max(10, Math.min(100, val));
    } else if (dragging.type === 'color-day') {
      // Map back to color temp: 100% = day temp, 0% = night temp
      const dayK = 4000 + val * 60; // 4000-10000K range
      settings.color.day_temp_k = Math.max(4000, Math.min(10000, Math.round(dayK / 100) * 100));
    } else if (dragging.type === 'color-night') {
      const nightK = 1800 + val * 32; // 1800-5000K range  
      settings.color.night_temp_k = Math.max(1800, Math.min(5000, Math.round(nightK / 100) * 100));
    }
  }

  function onPointerUp() {
    dragging = null;
  }

  function fmtTime(min) {
    const h = Math.floor(min / 60);
    const m = min % 60;
    return `${h.toString().padStart(2, "0")}:${m.toString().padStart(2, "0")}`;
  }

  // Hour grid lines
  const hours = Array.from({ length: 25 }, (_, i) => i);
</script>

<div class="curves-wrap">
  <svg
    bind:this={svgEl}
    viewBox="0 0 {W} {H}"
    class="curves-svg"
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointerleave={onPointerUp}
  >
    <!-- Grid: hour lines -->
    {#each hours as h}
      <line
        x1={minX(h * 60)} y1={PAD_T}
        x2={minX(h * 60)} y2={PAD_T + PLOT_H}
        class="grid-line"
        class:major={h % 6 === 0}
      />
      {#if h % 3 === 0 && h < 24}
        <text x={minX(h * 60)} y={H - 8} class="axis-label">{h}h</text>
      {/if}
    {/each}

    <!-- Y axis labels -->
    <text x={8} y={valY(100) + 5} class="axis-label">100%</text>
    <text x={8} y={valY(75) + 5} class="axis-label">75%</text>
    <text x={8} y={valY(50) + 5} class="axis-label">50%</text>
    <text x={8} y={valY(25) + 5} class="axis-label">25%</text>
    <text x={8} y={valY(0) + 5} class="axis-label">0%</text>

    <!-- Night region shading -->
    {#if sunTimes}
      <rect
        x={minX(toMin(sunTimes.sunset))}
        y={PAD_T}
        width={minX(1440) - minX(toMin(sunTimes.sunset))}
        height={PLOT_H}
        class="night-shade"
      />
      <rect
        x={minX(0)}
        y={PAD_T}
        width={minX(toMin(sunTimes.sunrise)) - minX(0)}
        height={PLOT_H}
        class="night-shade"
      />
    {/if}

    <!-- Brightness curve -->
    {#if brightnessPath}
      <path d={brightnessPath} class="curve brightness" />
      <!-- Interactive control points -->
      <circle
        cx={minX(720)}
        cy={valY(settings?.brightness?.day_percent ?? 100)}
        r="6"
        class="control-point brightness"
        onpointerdown={(e) => onPointerDown('brightness-day', e)}
      />
      <circle
        cx={minX(60)}
        cy={valY(settings?.brightness?.night_percent ?? 50)}
        r="6"
        class="control-point brightness"
        onpointerdown={(e) => onPointerDown('brightness-night', e)}
      />
    {/if}

    <!-- Color/gamma curve -->
    {#if colorPath}
      <path d={colorPath} class="curve color" />
      <!-- Interactive control points -->
      <circle
        cx={minX(720)}
        cy={valY(colorVal(720))}
        r="6"
        class="control-point color"
        onpointerdown={(e) => onPointerDown('color-day', e)}
      />
      <circle
        cx={minX(60)}
        cy={valY(colorVal(60))}
        r="6"
        class="control-point color"
        onpointerdown={(e) => onPointerDown('color-night', e)}
      />
    {/if}

    <!-- Sunrise/Sunset markers -->
    {#if sunTimes}
      <g class="time-marker sunrise">
        <line 
          x1={minX(toMin(sunTimes.sunrise))} 
          y1={PAD_T} 
          x2={minX(toMin(sunTimes.sunrise))} 
          y2={PAD_T + PLOT_H} 
          class="time-line sunrise"
        />
        <text 
          x={minX(toMin(sunTimes.sunrise))} 
          y={PAD_T - 5} 
          class="time-label sunrise"
        >
          ☀️ {sunTimes.sunrise}
        </text>
      </g>
      <g class="time-marker sunset">
        <line 
          x1={minX(toMin(sunTimes.sunset))} 
          y1={PAD_T} 
          x2={minX(toMin(sunTimes.sunset))} 
          y2={PAD_T + PLOT_H} 
          class="time-line sunset"
        />
        <text 
          x={minX(toMin(sunTimes.sunset))} 
          y={PAD_T - 5} 
          class="time-label sunset"
        >
          🌙 {sunTimes.sunset}
        </text>
      </g>
    {/if}

    <!-- Current time marker -->
    <g class="time-marker current">
      <line 
        x1={currentX} 
        y1={PAD_T} 
        x2={currentX} 
        y2={PAD_T + PLOT_H} 
        class="time-line current"
      />
      <text 
        x={currentX} 
        y={H - 28} 
        class="time-label current"
      >
        ▼ {fmtTime(currentMinute())}
      </text>
    </g>
  </svg>

  <!-- Legend -->
  <div class="legend">
    <div class="legend-item">
      <span class="swatch brightness"></span> 
      <span>Monitor brightness (drag circles to adjust)</span>
    </div>
    <div class="legend-item">
      <span class="swatch color"></span> 
      <span>Color warmth / gamma (drag circles to adjust)</span>
    </div>
    <div class="legend-item">
      <span class="indicator current"></span> 
      <span>Current time</span>
    </div>
  </div>
</div>

<style>
  .curves-wrap {
    border: 1px solid #ddd;
    border-radius: 8px;
    padding: 0.75rem;
    background: #fafafa;
  }

  .curves-svg {
    width: 100%;
    height: auto;
    touch-action: none;
    user-select: none;
  }

  .grid-line {
    stroke: #e8e8e8;
    stroke-width: 0.5;
  }

  .grid-line.major {
    stroke: #ccc;
    stroke-width: 1;
  }

  .axis-label {
    font-size: 10px;
    fill: #999;
    text-anchor: middle;
    font-family: system-ui, sans-serif;
  }

  .night-shade {
    fill: #1a1a3a;
    opacity: 0.08;
  }

  .curve {
    fill: none;
    stroke-width: 3;
    stroke-linecap: round;
    stroke-linejoin: round;
    pointer-events: none;
  }

  .curve.brightness {
    stroke: #4a9eff;
  }

  .curve.color {
    stroke: #ff8c42;
  }

  .control-point {
    cursor: grab;
    stroke-width: 2;
    transition: r 0.15s;
  }

  .control-point:hover {
    r: 8;
  }

  .control-point:active {
    cursor: grabbing;
  }

  .control-point.brightness {
    fill: #4a9eff;
    stroke: #fff;
  }

  .control-point.color {
    fill: #ff8c42;
    stroke: #fff;
  }

  .time-line {
    stroke-width: 2;
    stroke-dasharray: 5 3;
    pointer-events: none;
  }

  .time-line.sunrise {
    stroke: #ffa500;
  }

  .time-line.sunset {
    stroke: #8b5cf6;
  }

  .time-line.current {
    stroke: #22c55e;
    stroke-width: 2.5;
  }

  .time-label {
    font-size: 11px;
    text-anchor: middle;
    font-weight: 600;
    font-family: system-ui, sans-serif;
    pointer-events: none;
  }

  .time-label.sunrise {
    fill: #ffa500;
  }

  .time-label.sunset {
    fill: #8b5cf6;
  }

  .time-label.current {
    fill: #22c55e;
  }

  .legend {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.6rem 0.5rem 0.2rem;
    font-size: 0.8rem;
    color: #666;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .swatch {
    display: inline-block;
    width: 20px;
    height: 4px;
    border-radius: 2px;
  }

  .swatch.brightness {
    background: #4a9eff;
  }

  .swatch.color {
    background: #ff8c42;
  }

  .indicator {
    display: inline-block;
    width: 20px;
    height: 3px;
    border-radius: 1px;
  }

  .indicator.current {
    background: #22c55e;
  }

  @media (prefers-color-scheme: dark) {
    .curves-wrap {
      border-color: #444;
      background: #1e1e30;
    }

    .grid-line {
      stroke: #333;
    }

    .grid-line.major {
      stroke: #444;
    }

    .axis-label {
      fill: #777;
    }

    .night-shade {
      fill: #000;
      opacity: 0.2;
    }

    .legend {
      color: #aaa;
    }
  }
</style>
