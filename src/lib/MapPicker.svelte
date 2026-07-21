<script>
  import { onMount } from "svelte";
  import L from "leaflet";
  import "leaflet/dist/leaflet.css";

  let { lat = 40.7128, lng = -74.006, onPick = () => {} } = $props();

  let mapEl = $state(null);
  let map = null;
  let marker = null;
  let latInput = $state("");
  let lngInput = $state("");

  onMount(() => {
    const controller = new AbortController();

    latInput = lat.toString();
    lngInput = lng.toString();
    map = L.map(mapEl, {
      center: [lat, lng],
      zoom: 2,
      minZoom: 1,
      maxZoom: 8,
      zoomControl: true,
      attributionControl: false,
      worldCopyJump: true,
      maxBounds: [
        [-90, -200],
        [90, 200],
      ],
      maxBoundsViscosity: 0.8,
    });

    // Dark-friendly background
    map.getContainer().style.background = "#1a2332";

    // Natural Earth boundaries are a separate bundled asset, loaded only when
    // the map is mounted. No network connection is required.
    async function loadBoundaries() {
      const response = await fetch("/world-geo.json", {
        signal: controller.signal,
      });
      if (!response.ok) {
        throw new Error(`Failed to load map boundaries: ${response.status}`);
      }

      const worldGeoJSON = await response.json();
      if (!map) return;

      L.geoJSON(worldGeoJSON, {
        style: {
          fillColor: "#344f46",
          fillOpacity: 0.9,
          color: "#719184",
          weight: 0.65,
        },
      }).addTo(map);

      // Circle marker (no external images)
      marker = L.circleMarker([lat, lng], {
        radius: 8,
        fillColor: "#ff6b4a",
        fillOpacity: 1,
        color: "#fff",
        weight: 2,
      }).addTo(map);
    }

    loadBoundaries().catch((error) => {
      if (!controller.signal.aborted) {
        console.error("Could not load map boundaries:", error);
      }
    });

    map.on("click", (e) => {
      const { lat: la, lng: ln } = e.latlng;
      marker.setLatLng([la, ln]);
      latInput = round4(la).toString();
      lngInput = round4(ln).toString();
      onPick(round4(la), round4(ln));
    });

    return () => {
      controller.abort();
      map.remove();
      map = null;
    };
  });

  function round4(v) {
    return Math.round(v * 10000) / 10000;
  }

  function applyManual() {
    const la = parseFloat(latInput);
    const ln = parseFloat(lngInput);
    if (isNaN(la) || isNaN(ln)) return;
    const clampedLat = Math.max(-90, Math.min(90, la));
    const clampedLng = Math.max(-180, Math.min(180, ln));
    latInput = clampedLat.toString();
    lngInput = clampedLng.toString();
    if (marker) marker.setLatLng([clampedLat, clampedLng]);
    if (map) map.panTo([clampedLat, clampedLng]);
    onPick(round4(clampedLat), round4(clampedLng));
  }
</script>

<div class="map-wrap">
  <div bind:this={mapEl} class="map"></div>
  <div class="manual-row">
    <label>
      Lat
      <input
        type="number"
        step="0.0001"
        min="-90"
        max="90"
        bind:value={latInput}
        onchange={applyManual}
      />
    </label>
    <label>
      Lng
      <input
        type="number"
        step="0.0001"
        min="-180"
        max="180"
        bind:value={lngInput}
        onchange={applyManual}
      />
    </label>
    <span class="hint">Click map or type coordinates</span>
  </div>
</div>

<style>
  .map-wrap {
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid #ddd;
  }

  .map {
    height: 300px;
    width: 100%;
    z-index: 0;
    background: #1a2332;
  }

  .manual-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.7rem;
    background: #f5f5f5;
  }

  .manual-row label {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.78rem;
    color: #555;
  }

  .manual-row input {
    width: 5.5rem;
    padding: 0.3rem 0.4rem;
    border: 1px solid #ddd;
    border-radius: 5px;
    font-size: 0.8rem;
  }

  .hint {
    color: #999;
    font-size: 0.7rem;
    margin-left: auto;
  }

  @media (prefers-color-scheme: dark) {
    .map-wrap {
      border-color: #444;
    }

    .manual-row {
      background: #2a2a3e;
    }

    .manual-row label {
      color: #bbb;
    }

    .manual-row input {
      background: #1e1e30;
      border-color: #444;
      color: #eee;
    }

    .hint {
      color: #888;
    }
  }
</style>
