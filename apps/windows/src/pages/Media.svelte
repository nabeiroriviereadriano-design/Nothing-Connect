<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { api, type MediaAction } from "../lib/api";

  let sessions = $derived((app.selected && app.media[app.selected]) || []);
  let pcAudio = $state(false);
  let phoneAudio = $derived(!!app.streams.phone_audio);
  api.pcAudioActive().then((v) => (pcAudio = v));

  async function togglePc() {
    if (!app.selected) return;
    if (pcAudio) {
      await app.run(api.pcAudioStop());
      pcAudio = false;
    } else if ((await app.run(api.pcAudioStart(app.selected))) !== undefined) {
      pcAudio = true;
    }
  }
  async function togglePhone() {
    if (!app.selected) return;
    if (phoneAudio) await app.run(api.phoneAudioStop(app.selected));
    else await app.run(api.phoneAudioStart(app.selected), "Acceptez la capture du son sur le téléphone.");
  }
  function go(id: string, a: MediaAction) {
    if (app.selected) app.run(api.media(app.selected, id, a));
  }
  const fmt = (ms: number | null) => (ms == null ? "" : `${Math.floor(ms / 60000)}:${String(Math.floor((ms % 60000) / 1000)).padStart(2, "0")}`);
</script>

<div class="stack page">
  <h1>Multimédia</h1>
  <section class="panel stack">
    <h2>Son</h2>
    <div class="row">
      <div class="grow stack tight">
        <span>Son du téléphone sur le PC</span>
        <span class="muted small">Android demande l'accord à chaque démarrage. Certaines applications (appels, contenus protégés) interdisent la capture de leur son.</span>
      </div>
      <button disabled={!app.online || !app.has("phone_audio")} onclick={togglePhone}>{phoneAudio ? "Arrêter" : "Écouter sur le PC"}</button>
    </div>
    <div class="row">
      <div class="grow stack tight">
        <span>Son du PC sur le téléphone</span>
        <span class="muted small">Tout le son joué par Windows est envoyé au téléphone (latence d'environ un dixième de seconde).</span>
      </div>
      <button disabled={!app.online || !app.has("audio_playback")} onclick={togglePc}>{pcAudio ? "Arrêter" : "Envoyer au téléphone"}</button>
    </div>
  </section>
  <h2>Lectures en cours</h2>
  {#if !app.has("media_control")}
    <p class="panel">Ce téléphone n'annonce pas le contrôle multimédia. Il nécessite l'accès aux notifications sur le téléphone.</p>
  {:else if !sessions.length}
    <p class="muted">Aucune lecture en cours. Lancez Spotify, YouTube Music, Deezer ou une autre application de lecture sur le téléphone.</p>
  {/if}
  {#each sessions as s (s.id)}
    <article class="player panel">
      {#if s.artwork_jpeg_b64}
        <img src={`data:image/jpeg;base64,${s.artwork_jpeg_b64}`} alt="Pochette" />
      {:else}
        <div class="noart dot" aria-hidden="true">♪</div>
      {/if}
      <div class="stack grow meta">
        <span class="muted small">{s.app_name}</span>
        <h2 class="ellipsis">{s.title ?? "Titre inconnu"}</h2>
        <span class="ellipsis">{s.artist ?? ""}{s.album ? `, ${s.album}` : ""}</span>
        {#if s.duration_ms}<span class="muted small">{fmt(s.position_ms)} / {fmt(s.duration_ms)}</span>{/if}
        <div class="row controls">
          <button class="icon" disabled={!app.online || !s.can_skip_prev} aria-label="Piste précédente" onclick={() => go(s.id, "previous")}>Précédent</button>
          <button class="primary" disabled={!app.online} onclick={() => go(s.id, s.state === "playing" ? "pause" : "play")}>{s.state === "playing" ? "Pause" : "Lecture"}</button>
          <button class="icon" disabled={!app.online || !s.can_skip_next} aria-label="Piste suivante" onclick={() => go(s.id, "next")}>Suivant</button>
        </div>
      </div>
    </article>
  {/each}
</div>

<style>
  .page { gap: 20px; }
  .player { display: flex; gap: 24px; align-items: center; }
  img, .noart { width: 132px; height: 132px; border-radius: var(--r-mid); object-fit: cover; flex: none; }
  .noart { background: var(--surface-2); display: flex; align-items: center; justify-content: center; font-size: 48px; }
  .meta { gap: 4px; min-width: 0; }
  .controls { margin-top: 10px; }
  .tight { gap: 2px; }
</style>
