<script lang="ts">
  // Anneau de 48 points qui se remplit avec la charge ; chiffre en matrice de points.
  let { percent, charging = false }: { percent: number | null; charging?: boolean | null } = $props();
  const N = 48;
  const dots = Array.from({ length: N }, (_, i) => {
    const a = (i / N) * Math.PI * 2 - Math.PI / 2;
    return { x: 120 + Math.cos(a) * 104, y: 120 + Math.sin(a) * 104 };
  });
  let lit = $derived(percent == null ? 0 : Math.round((percent / 100) * N));
  let low = $derived(percent != null && percent <= 15 && !charging);
</script>

<div class="ring" role="img" aria-label={percent == null ? "Batterie inconnue" : `Batterie ${percent} %${charging ? ", en charge" : ""}`}>
  <svg viewBox="0 0 240 240" aria-hidden="true">
    {#each dots as d, i}
      <circle cx={d.x} cy={d.y} r={i < lit ? 4.2 : 2.4} class:on={i < lit} class:low />
    {/each}
  </svg>
  <div class="value">
    <span class="dot num">{percent ?? "--"}</span>
    <span class="unit">{charging ? "% en charge" : "%"}</span>
  </div>
</div>

<style>
  .ring { position: relative; width: 240px; height: 240px; }
  svg { width: 100%; height: 100%; }
  circle { fill: var(--line); transition: r 0.4s ease, fill 0.4s ease; }
  circle.on { fill: var(--white); }
  circle.on.low { fill: var(--red); }
  .value { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; }
  .num { font-size: 76px; line-height: 1; }
  .unit { color: var(--muted); font-size: 14px; margin-top: 6px; }
</style>
