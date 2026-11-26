<script lang="ts" generics="T extends string | number | boolean">
  // Choix exclusif compact (groupe radio accessible au clavier : flèches gauche/droite).
  let { label, options, value, onchange, disabled = false }: {
    label: string;
    options: { value: T; label: string; hint?: string }[];
    value: T;
    onchange: (v: T) => void;
    disabled?: boolean;
  } = $props();

  function key(e: KeyboardEvent, i: number) {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const n = (i + (e.key === "ArrowRight" ? 1 : options.length - 1)) % options.length;
    onchange(options[n].value);
    ((e.currentTarget as HTMLElement).parentElement?.children[n] as HTMLElement | undefined)?.focus();
  }
</script>

<div class="field">
  <span class="label">{label}</span>
  <div class="seg" role="radiogroup" aria-label={label}>
    {#each options as o, i (o.value)}
      <button
        type="button"
        role="radio"
        aria-checked={o.value === value}
        tabindex={o.value === value ? 0 : -1}
        title={o.hint}
        {disabled}
        class:on={o.value === value}
        onclick={() => onchange(o.value)}
        onkeydown={(e) => key(e, i)}>{o.label}</button>
    {/each}
  </div>
</div>

<style>
  .field { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .label { font-size: 13px; color: var(--muted); }
  .seg { display: flex; background: var(--black); border: 1px solid var(--line); border-radius: var(--r-pill); padding: 3px; gap: 2px; width: fit-content; max-width: 100%; overflow-x: auto; }
  .seg button { border: none; background: transparent; color: var(--muted); padding: 8px 14px; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .seg button:hover:not(:disabled):not(.on) { color: var(--white); }
  .seg button.on { background: var(--white); color: var(--black); }
</style>
