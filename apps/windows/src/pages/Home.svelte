<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { api, type FirewallStatus } from "../lib/api";
  import { bytes, ago } from "../lib/format";
  import { pickAndSend } from "../lib/send";
  import BatteryRing from "../components/BatteryRing.svelte";
  import ExtendedDisplay from "../components/ExtendedDisplay.svelte";

  let st = $derived(app.selected ? app.status[app.selected] : undefined);
  let d = $derived(app.device);
  let playing = $derived(app.selected ? (app.media[app.selected] ?? []).find((m) => m.state === "playing") ?? (app.media[app.selected] ?? [])[0] : undefined);
  let recent = $derived(app.past.filter((t) => t.device_id === app.selected).slice(0, 3));

  // Téléphone hors ligne : le pare-feu Windows est une cause fréquente (réseau « Public »).
  let fw = $state<FirewallStatus | null>(null);
  $effect(() => {
    if (d && !app.online && !fw) api.firewallStatus().then((s) => (fw = s)).catch(() => {});
  });
  async function allowFirewall() {
    if ((await app.run(api.firewallAllow(), "NOTHING CONNECT est autorisé dans le pare-feu")) !== undefined) fw = await api.firewallStatus();
  }

  function ring() {
    if (app.selected) app.run(api.command(app.selected, { c: "ring" }), "Le téléphone sonne");
  }
  function toggle() {
    if (app.selected && playing) app.run(api.media(app.selected, playing.id, "toggle_play_pause"));
  }
</script>

{#if !d}
  <section class="empty">
    <h1>Reliez votre téléphone</h1>
    <p class="muted">Installez NOTHING CONNECT sur votre téléphone Android, puis associez-le avec un code ou un QR code. Le PC et le téléphone doivent être sur le même réseau Wi-Fi, ou reliés par un câble USB.</p>
    <div><button class="primary" onclick={() => (app.pairingOpen = true)}>Associer un téléphone</button></div>
  </section>
{:else}
  <div class="home">
    <section class="hero">
      <BatteryRing percent={st?.battery_percent ?? null} charging={st?.charging} />
      <div class="info">
        <h1>{d.custom_name || d.name}</h1>
        <p class="muted">{[d.manufacturer, d.model, d.os_version && `Android ${d.os_version}`].filter(Boolean).join(" · ") || "Android"}</p>
        <dl class="facts">
          <div><dt>Liaison</dt><dd>{app.online ? (app.link?.link === "usb" ? "Câble USB" : `Wi-Fi, ${app.link?.addr.split(":")[0]}`) : "Hors ligne"}</dd></div>
          <div><dt>Stockage libre</dt><dd>{st?.storage_free_bytes != null ? `${bytes(st.storage_free_bytes)} sur ${bytes(st.storage_total_bytes)}` : "—"}</dd></div>
          <div><dt>Vu</dt><dd>{app.online ? "maintenant" : ago(d.last_seen)}</dd></div>
        </dl>
        <div class="row actions">
          <button class="primary" disabled={!app.online} onclick={() => pickAndSend(false)}>Envoyer des fichiers</button>
          <button disabled={!app.online || !app.has("screen_mirror")} onclick={() => (app.page = "mirror")}>Voir l'écran du téléphone</button>
          {#if app.has("ring")}<button disabled={!app.online} onclick={ring}>Faire sonner</button>{/if}
        </div>
        {#if !app.online}
          <p class="muted small">Ouvrez l'application sur le téléphone : il se reconnecte seul dès qu'il est sur le même réseau ou branché en USB.</p>
          {#if fw?.needs_fix}
            <p class="small">Le pare-feu Windows bloque peut-être le téléphone sur ce réseau. <button class="link" onclick={allowFirewall}>Autoriser dans le pare-feu</button></p>
          {/if}
        {/if}
      </div>
    </section>

    <ExtendedDisplay />

    <div class="cols">
      <section class="card">
        <div class="row head"><h2 class="grow">Notifications</h2><button class="ghost small" onclick={() => (app.page = "notifications")}>Tout voir</button></div>
        {#if app.live.length}
          <ul>
            {#each app.live.slice(0, 4) as n (n.key)}
              <li>
                <span class="app small">{n.app_name}</span>
                <span class="ellipsis"><b>{n.title ?? ""}</b>{n.text ? ` ${n.text}` : ""}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted small">Aucune notification sur le téléphone.</p>
        {/if}
      </section>

      <section class="card">
        {#if playing}
          <div class="row head"><h2 class="grow">En lecture</h2><button class="ghost small" onclick={() => (app.page = "media")}>Multimédia</button></div>
          <div class="row">
            <div class="grow stack tight">
              <span class="muted small">{playing.app_name}</span>
              <span class="ellipsis">{playing.title ?? "Sans titre"}</span>
              <span class="muted small ellipsis">{playing.artist ?? ""}</span>
            </div>
            <button class="primary" disabled={!app.online} onclick={toggle}>{playing.state === "playing" ? "Pause" : "Lecture"}</button>
          </div>
        {:else}
          <div class="row head"><h2 class="grow">Fichiers récents</h2><button class="ghost small" onclick={() => (app.page = "share")}>Partage</button></div>
          {#if recent.length}
            <ul>
              {#each recent as t (t.id)}
                <li>
                  <span class="app small">{t.outgoing ? "Envoyé" : "Reçu"}</span>
                  <span class="ellipsis">{t.files.length === 1 ? t.files[0].rel_path : `${t.files.length} fichiers`}</span>
                  <span class="muted small">{bytes(t.total_bytes)}</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="muted small">Glissez des fichiers sur cette fenêtre pour les envoyer au téléphone.</p>
          {/if}
        {/if}
      </section>
    </div>
  </div>
{/if}

<style>
  .empty { display: flex; flex-direction: column; gap: 16px; max-width: 520px; padding-top: 60px; }
  .home { display: flex; flex-direction: column; gap: 22px; }
  .hero { display: flex; gap: 40px; align-items: center; flex-wrap: wrap; }
  .info { flex: 1; min-width: 300px; display: flex; flex-direction: column; gap: 8px; }
  .facts { display: flex; flex-wrap: wrap; gap: 12px 36px; margin: 10px 0 6px; }
  .facts div { display: flex; flex-direction: column; gap: 2px; }
  dt { font-size: 13px; color: var(--muted); }
  dd { margin: 0; }
  .actions { flex-wrap: wrap; margin-top: 6px; }
  .cols { display: grid; grid-template-columns: 1fr 1fr; gap: 22px; }
  .card { background: var(--surface); border-radius: var(--r-big); padding: 20px 22px; display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .head h2 { font-size: 17px; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  li { display: flex; gap: 12px; align-items: baseline; padding: 9px 0; border-top: 1px solid var(--line); min-width: 0; }
  li:first-child { border-top: none; }
  .app { color: var(--muted); width: 84px; flex: none; }
  b { font-weight: 500; }
  .tight { gap: 2px; }
  .link { background: none; border: none; padding: 0; color: var(--white); text-decoration: underline; text-underline-offset: 3px; font-size: inherit; }
  @media (max-width: 980px) { .cols { grid-template-columns: 1fr; } }
</style>
