<script lang="ts">
  let { checked, label, hint = "", onchange, disabled = false }: { checked: boolean; label: string; hint?: string; onchange: (v: boolean) => void; disabled?: boolean } = $props();
</script>

<label class="toggle" class:disabled>
  <span class="text">
    <span>{label}</span>
    {#if hint}<span class="muted small">{hint}</span>{/if}
  </span>
  <input type="checkbox" role="switch" {checked} {disabled} onchange={(e) => onchange((e.currentTarget as HTMLInputElement).checked)} />
  <span class="track" aria-hidden="true"><span class="knob"></span></span>
</label>

<style>
  .toggle { display: flex; align-items: center; gap: 16px; padding: 12px 0; cursor: pointer; }
  .toggle.disabled { opacity: 0.45; cursor: default; }
  .text { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  input { position: absolute; opacity: 0; width: 1px; height: 1px; }
  .track { width: 44px; height: 26px; border-radius: 13px; border: 1px solid var(--line); background: var(--black); position: relative; flex: none; transition: background 0.2s; }
  .knob { position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: var(--muted); transition: transform 0.2s, background 0.2s; }
  input:checked + .track { background: var(--white); border-color: var(--white); }
  input:checked + .track .knob { transform: translateX(18px); background: var(--black); }
  input:focus-visible + .track { outline: 2px solid var(--white); outline-offset: 2px; }
</style>
