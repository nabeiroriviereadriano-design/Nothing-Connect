<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { api, type MediaAction } from "../lib/api";

  let sessions = $derived((app.selected && app.media[app.selected]) || []);
  function go(id: string, a: MediaAction) {
    if (app.selected) app.run(api.media(app.selected, id, a));
  }
  const fmt = (ms: number | null) => (ms == null ? "" : `${Math.floor(ms / 60000)}:${String(Math.floor((ms % 60000) / 1000)).padStart(2, "0")}`);
</script>

<div class="stack page">
  <h1>Multimédia</h1>
  <p class="muted">Contrôle des lectures en cours sur le téléphone. Le son reste sur le téléphone : la transmission audio vers le PC n'est pas encore disponible.</p>
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
</style>
