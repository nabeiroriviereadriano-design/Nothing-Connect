<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { app } from "../lib/state.svelte";
  import { api, type DisplayStatus } from "../lib/api";

  let st = $state<DisplayStatus | null>(null);
  let busy = $state(false);
  let scale = $state(0.75);
  let quality = $state(70);
  let fps = $state(30);
  let portrait = $state(false);
  let needReboot = $state(false);

  async function refresh() {
    st = await api.displayStatus();
  }
  async function setup(action: "install" | "uninstall" | "enable-testsigning") {
    busy = true;
    const r = await app.run(api.displaySetup(action));
    busy = false;
    if (r === "redemarrage") needReboot = true;
    else if (r !== undefined) app.toast(action === "uninstall" ? "Pilote retiré" : "Pilote installé");
    await refresh();
  }
  async function enableTestSigning() {
    const ok = await confirm(
      "Le mode « signature de test » permet à Windows de charger un pilote qui n'est pas encore signé par Microsoft. Il affiche la mention « Mode test » en bas de l'écran et nécessite un redémarrage. Sur les PC avec démarrage sécurisé (Secure Boot), il faut d'abord le désactiver dans le BIOS. Continuer ?",
      { title: "Activer la signature de test", kind: "warning", okLabel: "Activer", cancelLabel: "Annuler" },
    );
    if (ok) await setup("enable-testsigning");
  }
  async function start() {
    if (!app.selected) return;
    await app.run(api.displayStart(app.selected, scale, quality, fps, portrait), "Ouverture de l'écran sur le téléphone…");
    setTimeout(refresh, 2500);
  }
  async function stop() {
    await app.run(api.displayStop());
    await refresh();
  }

  onMount(() => {
    refresh();
    const t = setInterval(refresh, 4000);
    const un = listen<string>("nc-display-error", (e) => app.toast(e.payload, "error"));
    return () => {
      clearInterval(t);
      un.then((f) => f());
    };
  });
</script>

<div class="stack page">
  <h1>Écran étendu</h1>
  <p class="muted">Le téléphone devient un écran supplémentaire de Windows, indépendant de l'écran principal. Vous pouvez y déplacer des fenêtres et le placer où vous voulez dans Paramètres › Système › Écran.</p>

  {#if !st}
    <p class="muted">Vérification…</p>
  {:else if !st.supported}
    <p class="panel">Fonction disponible uniquement sous Windows.</p>
  {:else}
    <section class="panel stack">
      <h2>Pilote d'écran</h2>
      <div class="row"><span class={st.installed ? "live" : "idle"}></span><span>{st.installed ? "Installé" : "Non installé"}</span></div>
      {#if !st.installed}
        {#if !st.setup_available}
          <p class="muted">Cette version ne contient pas le pilote.</p>
        {:else if !st.testsigning}
          <p class="muted">Le pilote n'est pas encore signé par Microsoft (cette signature exige un certificat professionnel payant). Pour l'utiliser dès maintenant, Windows doit accepter les pilotes signés « en test ».</p>
          <div class="row wrap">
            <button disabled={busy} onclick={enableTestSigning}>Activer la signature de test</button>
          </div>
        {:else}
          <p class="muted">Windows demandera une autorisation administrateur.</p>
          <div><button class="primary" disabled={busy} onclick={() => setup("install")}>Installer le pilote</button></div>
        {/if}
        {#if needReboot}<p class="warn">Redémarrez le PC, puis revenez ici pour installer le pilote.</p>{/if}
      {:else}
        <div><button class="ghost small" disabled={busy || !!st.active_peer} onclick={() => setup("uninstall")}>Retirer le pilote</button></div>
      {/if}
    </section>

    {#if st.installed}
      <section class="panel stack">
        <h2>Écran du téléphone</h2>
        {#if st.active_peer}
          <div class="row"><span class="live"></span><span class="grow">Actif sur {app.name(st.active_peer)}, <span class="dot">{st.width}×{st.height}</span></span>
            <button class="danger" onclick={stop}>Débrancher l'écran</button></div>
          <p class="muted small">Touchez l'écran du téléphone pour cliquer et glisser. Fermer l'application sur le téléphone débranche l'écran.</p>
        {:else if !app.has("extended_display")}
          <p class="muted">Mettez à jour l'application Android pour utiliser l'écran étendu.</p>
        {:else}
          <div class="row wrap">
            <label class="row small">Définition
              <select bind:value={scale}>
                <option value={1}>Native (100 %)</option>
                <option value={0.75}>Équilibrée (75 %)</option>
                <option value={0.5}>Fluide (50 %)</option>
              </select>
            </label>
            <label class="row small">Qualité
              <select bind:value={quality}>
                <option value={85}>Haute</option>
                <option value={70}>Standard</option>
                <option value={50}>Économique</option>
              </select>
            </label>
            <label class="row small">Images/s
              <select bind:value={fps}>
                <option value={60}>60</option>
                <option value={30}>30</option>
                <option value={15}>15</option>
              </select>
            </label>
            <label class="row small"><input type="checkbox" bind:checked={portrait} /> Portrait</label>
          </div>
          <div><button class="primary" disabled={!app.online} onclick={start}>Brancher l'écran sur le téléphone</button></div>
        {/if}
      </section>
    {/if}
  {/if}
</div>

<style>
  .page { gap: 20px; }
  .wrap { flex-wrap: wrap; }
  select { width: auto; }
  .warn { color: var(--red); }
</style>
