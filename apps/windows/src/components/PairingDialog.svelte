<script lang="ts">
  import { onDestroy } from "svelte";
  import Modal from "./Modal.svelte";
  import { app } from "../lib/state.svelte";
  import { api, type PairingView, type FirewallStatus } from "../lib/api";

  let view = $state<PairingView | null>(null);
  let fw = $state<FirewallStatus | null>(null);
  let fixing = $state(false);
  api.firewallStatus().then((s) => (fw = s)).catch(() => {});

  async function allowFirewall() {
    fixing = true;
    const ok = await app.run(api.firewallAllow(), "NOTHING CONNECT est autorisé dans le pare-feu");
    fixing = false;
    if (ok !== undefined) fw = await api.firewallStatus().catch(() => fw);
  }
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
      {#if fw?.needs_fix}
        <div class="fw">
          <p><b>Le pare-feu Windows peut bloquer le téléphone.</b></p>
          <p class="muted small">
            {fw.blocked ? "Une règle du pare-feu bloque NOTHING CONNECT (souvent créée en refusant la fenêtre de Windows)." : "Ce réseau est classé « Public » (partage de connexion, Wi-Fi public) : Windows y bloque les connexions entrantes."}
            Autorisez NOTHING CONNECT pour que le téléphone puisse se connecter ; Windows demande une confirmation.
          </p>
          <div><button class="primary" disabled={fixing} onclick={allowFirewall}>{fixing ? "Autorisation…" : "Autoriser dans le pare-feu"}</button></div>
        </div>
      {:else}
        <details class="help">
          <summary class="small">Le téléphone ne trouve pas le PC ?</summary>
          <ul class="small muted">
            <li>Les deux appareils doivent être sur le même réseau (Wi-Fi, ou partage de connexion du téléphone).</li>
            <li>Ou branchez le câble USB avec le débogage USB activé : l'association passe alors par le câble.</li>
            <li><button class="link" disabled={fixing} onclick={allowFirewall}>Autoriser NOTHING CONNECT dans le pare-feu Windows</button></li>
          </ul>
        </details>
      {/if}
    {:else}
      <p>Le code a expiré.</p>
      <div><button class="primary" onclick={openWindow}>Nouveau code</button></div>
    {/if}
  {/if}
  <div class="row end"><button onclick={close}>Fermer</button></div>
</Modal>

<style>
  .pair { display: flex; gap: 24px; align-items: center; flex-wrap: wrap; }
  .fw { border: 1px solid var(--red); border-radius: var(--r-mid); padding: 14px 16px; display: flex; flex-direction: column; gap: 8px; }
  .fw p { margin: 0; }
  .help summary { cursor: pointer; color: var(--muted); width: fit-content; }
  .help summary:hover { color: var(--white); }
  .help ul { margin: 8px 0 0; padding-left: 18px; display: flex; flex-direction: column; gap: 4px; }
  .link { background: none; border: none; padding: 0; color: var(--white); text-decoration: underline; text-underline-offset: 3px; font-size: inherit; }
  .qr { width: 200px; height: 200px; border-radius: 14px; overflow: hidden; background: #fff; flex: none; }
  .qr :global(svg) { width: 100%; height: 100%; display: block; }
  .code { font-size: 52px; line-height: 1; }
  .end { justify-content: flex-end; }
</style>
