<script lang="ts">
  let { value, total, tone = "white" }: { value: number; total: number; tone?: "white" | "red" } = $props();
  let pct = $derived(total > 0 ? Math.min(100, (value / total) * 100) : value > 0 ? 100 : 0);
</script>

<div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(pct)}>
  <div class="fill" class:red={tone === "red"} style:width={`${pct}%`}></div>
</div>

<style>
  .bar { height: 6px; border-radius: 3px; background: var(--line); overflow: hidden; }
  .fill { height: 100%; background: var(--white); transition: width 0.25s linear; }
  .fill.red { background: var(--red); }
</style>
