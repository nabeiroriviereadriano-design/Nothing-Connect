<script lang="ts">
  import type { Snippet } from "svelte";
  let { title, onclose, children }: { title: string; onclose?: () => void; children: Snippet } = $props();
  let dlg: HTMLDialogElement;
  $effect(() => { dlg.showModal(); return () => dlg.close(); });
</script>

<dialog bind:this={dlg} oncancel={(e) => { e.preventDefault(); onclose?.(); }} aria-labelledby="modal-title">
  <h2 id="modal-title">{title}</h2>
  <div class="body">{@render children()}</div>
</dialog>

<style>
  dialog { background: var(--surface); color: var(--white); border: 1px solid var(--line); border-radius: var(--r-big); padding: 28px; width: min(520px, 92vw); }
  dialog::backdrop { background: rgb(0 0 0 / 0.75); }
  .body { margin-top: 18px; display: flex; flex-direction: column; gap: 16px; }
</style>
