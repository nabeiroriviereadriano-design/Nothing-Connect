<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import { bytes, speed, duration, STATE_LABEL, ago } from "../lib/format";
  import { pickAndSend } from "../lib/send";
  import Progress from "../components/Progress.svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";

  let text = $state("");
  let active = $derived(Object.values(app.transfers).filter((t) => !["completed", "cancelled", "rejected", "failed"].includes(t.state)));

  async function sendText() {
    if (!app.selected) return;
    const ok = await app.run(api.sendText(app.selected, text), "Envoyé au téléphone");
    if (ok !== undefined) text = "";
  }
  function reveal(p: string | null) {
    if (p) revealItemInDir(p).catch((e) => app.toast(String(e), "error"));
  }
</script>

<div class="stack page">
  <h1>Partage</h1>

  <section class="send panel">
    <div class="drop">
      <p class="dot big">Glissez ici</p>
      <p class="muted">Déposez des fichiers ou des dossiers n'importe où dans la fenêtre pour les envoyer{app.device ? ` à ${app.device.custom_name || app.device.name}` : ""}.</p>
      <div class="row">
        <button class="primary" disabled={!app.online} onclick={() => pickAndSend(false)}>Choisir des fichiers</button>
        <button disabled={!app.online} onclick={() => pickAndSend(true)}>Choisir un dossier</button>
      </div>
    </div>
    <div class="textbox stack">
      <label for="share-text">Texte ou lien</label>
      <textarea id="share-text" rows="3" bind:value={text} placeholder="Collez un lien ou un texte à ouvrir sur le téléphone"></textarea>
      <div><button disabled={!app.online || !text.trim()} onclick={sendText}>Envoyer le texte</button></div>
    </div>
  </section>

  {#each app.incoming as i (i.id)}
    <section class="incoming panel">
      <div class="row"><span class="live"></span><h2 class="grow">{app.name(i.peer_id)} veut vous envoyer {i.files.length === 1 ? "un fichier" : `${i.files.length} fichiers`}</h2></div>
      <ul class="files small">
        {#each i.files.slice(0, 6) as f}<li><span class="ellipsis">{f.rel_path}</span><span class="muted">{bytes(f.size)}</span></li>{/each}
        {#if i.files.length > 6}<li class="muted">et {i.files.length - 6} autres</li>{/if}
      </ul>
      <p class="muted small">Total : {bytes(i.total_bytes)}. Destination : {app.settings?.download_dir || "Téléchargements\\NOTHING CONNECT"}</p>
      <div class="row">
        <button class="primary" onclick={() => app.run(api.acceptTransfer(i.id, true))}>Accepter</button>
        <button onclick={() => app.run(api.acceptTransfer(i.id, false))}>Refuser</button>
      </div>
    </section>
  {/each}

  {#if active.length}
    <section class="stack">
      <h2>En cours</h2>
      {#each active as t (t.id)}
        <div class="transfer panel">
          <div class="row">
            <span class="dir small">{t.outgoing ? "Envoi" : "Réception"}</span>
            <span class="grow ellipsis">{t.label}</span>
            <span class="muted small">{STATE_LABEL[t.state]}</span>
          </div>
          <Progress value={t.bytes_done} total={t.total_bytes} tone={t.state === "interrupted" ? "red" : "white"} />
          <div class="row small">
            <span class="dot metric">{bytes(t.bytes_done)}</span>
            <span class="muted">sur {bytes(t.total_bytes)}</span>
            <span class="grow"></span>
            {#if t.state === "running"}
              <span class="dot metric">{speed(t.speed_bps)}</span>
              <span class="muted">reste {duration(t.eta_s)}</span>
            {/if}
          </div>
          <div class="row">
            {#if t.state === "running"}<button onclick={() => app.run(api.pauseTransfer(t.id))}>Pause</button>{/if}
            {#if t.state === "paused" || (t.state === "interrupted" && t.outgoing)}<button onclick={() => app.run(api.resumeTransfer(t.id))}>Reprendre</button>{/if}
            <button class="danger" onclick={() => app.run(api.cancelTransfer(t.id))}>Annuler</button>
          </div>
        </div>
      {/each}
    </section>
  {/if}

  <section class="stack">
    <div class="row">
      <h2 class="grow">Historique</h2>
      {#if app.past.length}<button class="ghost small" onclick={async () => { await app.run(api.clearTransferHistory()); app.refreshHistory(); }}>Effacer l'historique</button>{/if}
    </div>
    {#if !app.past.length}
      <p class="muted">Aucun transfert pour l'instant. Les fichiers envoyés et reçus apparaîtront ici.</p>
    {/if}
    {#each app.past as t (t.id)}
      <div class="hist row">
        <span class="dir small">{t.outgoing ? "Envoyé" : "Reçu"}</span>
        <div class="grow stack tight">
          <span class="ellipsis">{t.files.length === 1 ? t.files[0].rel_path : `${t.files.length} fichiers`}</span>
          <span class="muted small">{app.name(t.device_id)}, {bytes(t.total_bytes)}, {ago(t.created_at)}</span>
        </div>
        <span class="small" class:muted={t.state === "completed"} class:err={t.state === "failed"}>{STATE_LABEL[t.state] ?? t.state}</span>
        {#if !t.outgoing && t.files[0]?.final_path}
          <button class="ghost small" onclick={() => reveal(t.files[0].final_path)}>Afficher</button>
        {/if}
      </div>
    {/each}
  </section>
</div>

<style>
  .page { gap: 28px; }
  .send { display: grid; grid-template-columns: 1.3fr 1fr; gap: 28px; }
  .drop { border: 1px dashed var(--line); border-radius: var(--r-mid); padding: 22px; display: flex; flex-direction: column; gap: 12px; }
  .big { font-size: 40px; line-height: 1; }
  .incoming { display: flex; flex-direction: column; gap: 12px; border: 1px solid var(--red); }
  .files { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
  .files li { display: flex; justify-content: space-between; gap: 12px; }
  .transfer { display: flex; flex-direction: column; gap: 12px; padding: 18px 20px; }
  .dir { color: var(--muted); width: 72px; flex: none; }
  .metric { font-size: 17px; }
  .hist { padding: 12px 4px; border-bottom: 1px solid var(--line); }
  .tight { gap: 0; }
  .err { color: var(--red); }
  @media (max-width: 900px) { .send { grid-template-columns: 1fr; } }
</style>
