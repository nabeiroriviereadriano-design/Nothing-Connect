<script module lang="ts">
  export type IconName = "home" | "share" | "bell" | "sliders" | "play" | "phone" | "monitor" | "link" | "gear" | "clipboard";
</script>

<script lang="ts">
  // Pictogrammes en matrice de points 7×7, dans l'esprit de l'icône de l'application.
  let { name, size = 18 }: { name: IconName; size?: number } = $props();
  const GRIDS: Record<IconName, string[]> = {
    home: ["...1...", "..111..", ".11111.", "1111111", ".1...1.", ".1...1.", ".11111."],
    share: ["...1...", "..111..", ".1.1.1.", "...1...", "...1...", "1.....1", "1111111"],
    bell: ["...1...", "..111..", ".11111.", ".11111.", ".11111.", "1111111", "...1..."],
    sliders: [".1.....", "1111111", ".1.....", ".......", ".....1.", "1111111", ".....1."],
    play: [".1.....", ".11....", ".111...", ".1111..", ".111...", ".11....", ".1....."],
    phone: [".11111.", ".1...1.", ".1...1.", ".1...1.", ".1...1.", ".1.1.1.", ".11111."],
    monitor: ["1111111", "1.....1", "1.....1", "1.....1", "1111111", "...1...", ".11111."],
    link: ["11...11", "11...11", "..111..", "..1.1..", "..111..", "11...11", "11...11"],
    gear: ["..1.1..", ".11111.", "11...11", ".1...1.", "11...11", ".11111.", "..1.1.."],
    clipboard: ["..111..", "1111111", "1.....1", "1.111.1", "1.....1", "1.111.1", "1111111"],
  };
  let dots = $derived(
    GRIDS[name].flatMap((row, y) => [...row].map((c, x) => (c === "1" ? { x, y } : null)).filter((d) => d !== null)),
  );
</script>

<svg viewBox="0 0 7 7" width={size} height={size} aria-hidden="true">
  {#each dots as d}<circle cx={d.x + 0.5} cy={d.y + 0.5} r="0.38" />{/each}
</svg>

<style>
  svg { flex: none; fill: currentColor; }
</style>
