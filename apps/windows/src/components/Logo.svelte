<script lang="ts">
  // Monogramme NC en matrice de points (même dessin que l'icône de l'application).
  let { height = 22 }: { height?: number } = $props();
  const N = ["X...X", "XX..X", "X.X.X", "X..XX", "X...X", "X...X", "X...X"];
  const C = [".XXX.", "X...X", "X....", "X....", "X....", "X...X", ".XXX."];
  const dots = [N, C].flatMap((g, gi) => g.flatMap((row, y) => [...row].map((c, x) => (c === "X" ? { x: gi * 6 + x, y } : null)).filter((d) => d !== null)));
</script>

<svg viewBox="0 0 13 7" height={height} width={(height * 13) / 7} role="img" aria-label="NOTHING CONNECT">
  {#each dots as d}<circle cx={d.x + 0.5} cy={d.y + 0.5} r="0.4" fill="var(--white)" />{/each}
  <circle cx="12.3" cy="6.5" r="0.4" fill="var(--red)" />
</svg>
