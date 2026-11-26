<script lang="ts">
  import Modal from "./Modal.svelte";
  import { app } from "../lib/state.svelte";
  import { api } from "../lib/api";

  let r = $derived(app.pairingRequest!);
  async function answer(accept: boolean) {
    await app.run(api.confirmPairing(r.peer_id, accept));
    app.pairingRequest = null;
    if (!accept) app.pairingOpen = false;
  }
</script>

<Modal title="Autoriser ce téléphone ?" onclose={() => answer(false)}>
  <div class="stack">
    <h2>{r.name}</h2>
    {#if r.detail}<span class="muted">{r.detail}</span>{/if}
    <span class="muted small">Empreinte : {r.fingerprint}</span>
  </div>
  <p class="muted">Ce téléphone a saisi le bon code. Une fois autorisé, il pourra se reconnecter seul et échanger fichiers et notifications avec ce PC.</p>
  <div class="row">
    <button class="primary" onclick={() => answer(true)}>Autoriser</button>
    <button onclick={() => answer(false)}>Refuser</button>
  </div>
</Modal>
