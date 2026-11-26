<script lang="ts">
  import { onDestroy } from "svelte";
  import Modal from "./Modal.svelte";
  import { app } from "../lib/state.svelte";
  import { api, type PairingView } from "../lib/api";

  let view = $state<PairingView | null>(null);
  let left = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  async function openWindow() {
    view = (await app.run(api.openPairing())) ?? null;
    if (!view) return;
    left = view.expires_in_s;
    clearInterval(timer);
    timer = setInterval(() => (left = Math.max(0, left - 1)), 1000);
  }
  function close() {
    api.closePairing();
    app.pairingOpen = false;
  }
  openWindow();
  onDestroy(() => clearInterval(timer));
</script>

<Modal title="Associer un téléphone" onclose={close}>
  {#if view}
    {#if left > 0}
      <p class="muted">Dans l'application Android, touchez « Associer un PC », puis scannez ce QR code ou saisissez le code.</p>
      <div class="pair">
        {#if view.qr_svg}<div class="qr" aria-label="QR code d'association">{@html view.qr_svg}</div>{/if}
        <div class="stack">
          <span class="dot code" aria-label={`Code ${view.code.split("").join(" ")}`}>{view.code.slice(0, 3)} {view.code.slice(3)}</span>
          <span class="muted small">Expire dans {left} s</span>
          {#if view.address}<span class="muted small">Adresse du PC : {view.address}</span>{/if}
          <span class="muted small">Empreinte : {view.fingerprint}</span>
        </div>
      </div>
    {:else}
      <p>Le code a expiré.</p>
      <div><button class="primary" onclick={openWindow}>Nouveau code</button></div>
    {/if}
  {/if}
  <div class="row end"><button onclick={close}>Fermer</button></div>
</Modal>

<style>
  .pair { display: flex; gap: 24px; align-items: center; flex-wrap: wrap; }
  .qr { width: 200px; height: 200px; border-radius: 14px; overflow: hidden; background: #fff; flex: none; }
  .qr :global(svg) { width: 100%; height: 100%; display: block; }
  .code { font-size: 52px; line-height: 1; }
  .end { justify-content: flex-end; }
</style>
