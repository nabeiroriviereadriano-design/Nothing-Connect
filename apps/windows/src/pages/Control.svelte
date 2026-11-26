<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { api, type RemoteCommand } from "../lib/api";
  import { bytes, ago } from "../lib/format";
  import Toggle from "../components/Toggle.svelte";

  let st = $derived(app.selected ? app.status[app.selected] : undefined);
  let volume = $state(50);
  $effect(() => { if (st?.media_volume_percent != null) volume = st.media_volume_percent; });

  function cmd(c: RemoteCommand, ok?: string) {
    if (app.selected) app.run(api.command(app.selected, c), ok);
  }
  const yesno = (v: boolean | null | undefined, y: string, n: string) => (v == null ? "—" : v ? y : n);
</script>

<div class="stack page">
  <div class="row">
    <h1 class="grow">Centre de contrôle</h1>
    <button disabled={!app.online} onclick={() => cmd({ c: "refresh_status" })}>Actualiser</button>
  </div>
  {#if !app.device}
    <p class="muted">Associez un téléphone pour voir son état.</p>
  {:else}
    {#if !app.online}<p class="muted">Téléphone hors ligne. Dernières valeurs reçues {ago(app.device.last_seen)}.</p>{/if}
    <section class="grid">
      <div class="tile"><span class="muted small">Batterie</span><span class="dot v">{st?.battery_percent ?? "--"}%</span><span class="small muted">{yesno(st?.charging, "En charge", "Sur batterie")}</span></div>
      <div class="tile"><span class="muted small">Température batterie</span><span class="dot v">{st?.battery_temp_c != null ? st.battery_temp_c.toFixed(1) : "--"}°</span><span class="small muted">degrés Celsius</span></div>
      <div class="tile"><span class="muted small">Stockage libre</span><span class="dot v">{st?.storage_free_bytes != null ? bytes(st.storage_free_bytes).split(" ")[0] : "--"}</span><span class="small muted">{st?.storage_free_bytes != null ? `${bytes(st.storage_free_bytes).split(" ")[1]} sur ${bytes(st.storage_total_bytes)}` : "—"}</span></div>
      <div class="tile"><span class="muted small">Wi-Fi</span><span class="t">{yesno(st?.wifi_connected, "Connecté", "Déconnecté")}</span><span class="small muted ellipsis">{st?.wifi_ssid ?? ""}</span></div>
      <div class="tile"><span class="muted small">Bluetooth</span><span class="t">{yesno(st?.bluetooth_enabled, "Activé", "Désactivé")}</span></div>
      <div class="tile"><span class="muted small">Réseau mobile</span><span class="t">{st?.mobile_network ?? "—"}</span></div>
    </section>

    <section class="panel stack">
      <h2>Commandes</h2>
      {#if !(app.device.capabilities ?? []).some((c) => ["ring", "vibrate", "media_volume", "do_not_disturb"].includes(c))}
        <p class="muted">Ce téléphone n'annonce aucune commande à distance.</p>
      {/if}
      <div class="row wrap">
        {#if app.has("ring")}
          <button class="primary" disabled={!app.online} onclick={() => cmd({ c: "ring" }, "Le téléphone sonne")}>Faire sonner</button>
          <button disabled={!app.online} onclick={() => cmd({ c: "stop_ring" })}>Arrêter la sonnerie</button>
        {/if}
        {#if app.has("vibrate")}<button disabled={!app.online} onclick={() => cmd({ c: "vibrate", millis: 600 })}>Vibrer</button>{/if}
      </div>
      {#if app.has("media_volume")}
        <label class="vol">
          <span>Volume multimédia</span>
          <input type="range" min="0" max="100" step="5" bind:value={volume} disabled={!app.online} onchange={() => cmd({ c: "set_media_volume", percent: volume })} />
          <span class="dot">{volume}</span>
        </label>
      {/if}
      {#if app.has("do_not_disturb")}
        <Toggle checked={!!st?.dnd_enabled} label="Ne pas déranger" hint="Nécessite l'autorisation « Accès Ne pas déranger » sur le téléphone." disabled={!app.online} onchange={(v) => cmd({ c: "set_do_not_disturb", enabled: v })} />
      {/if}
    </section>
  {/if}
</div>

<style>
  .page { gap: 24px; }
  .grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .tile { background: var(--surface); border-radius: var(--r-mid); padding: 18px; display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .v { font-size: 40px; line-height: 1.1; }
  .t { font-size: 19px; }
  .wrap { flex-wrap: wrap; }
  .vol { display: flex; align-items: center; gap: 16px; }
  .vol input { flex: 1; accent-color: var(--white); }
  .vol .dot { width: 44px; text-align: right; font-size: 20px; }
  @media (max-width: 900px) { .grid { grid-template-columns: 1fr 1fr; } }
</style>
