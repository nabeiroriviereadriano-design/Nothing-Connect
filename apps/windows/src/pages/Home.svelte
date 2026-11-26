<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import { bytes, ago } from "../lib/format";
  import { pickAndSend } from "../lib/send";
  import BatteryRing from "../components/BatteryRing.svelte";

  let st = $derived(app.selected ? app.status[app.selected] : undefined);
  let d = $derived(app.device);

  function ring() {
    if (app.selected) app.run(api.command(app.selected, { c: "ring" }), "Le téléphone sonne");
  }
</script>

{#if !d}
  <section class="empty">
    <h1>Reliez votre téléphone</h1>
    <p class="muted">Installez NOTHING CONNECT sur votre téléphone Android, puis associez-le avec un code ou un QR code. Le PC et le téléphone doivent être sur le même réseau Wi-Fi.</p>
    <div><button class="primary" onclick={() => (app.pairingOpen = true)}>Associer un téléphone</button></div>
  </section>
{:else}
  <section class="hero">
    <BatteryRing percent={st?.battery_percent ?? null} charging={st?.charging} />
    <div class="stack info">
      <h1>{d.custom_name || d.name}</h1>
      <p class="muted">{[d.manufacturer, d.model, d.os_version && `Android ${d.os_version}`].filter(Boolean).join(", ") || "Android"}</p>
      <div class="facts">
        <div><span class="muted small">Liaison</span><span>{app.online ? (app.link?.link === "usb" ? "Câble USB" : `Réseau local, ${app.link?.addr.split(":")[0]}`) : "Hors ligne"}</span></div>
        <div><span class="muted small">Stockage libre</span><span>{st?.storage_free_bytes != null ? `${bytes(st.storage_free_bytes)} sur ${bytes(st.storage_total_bytes)}` : "—"}</span></div>
        <div><span class="muted small">Dernière connexion</span><span>{app.online ? "maintenant" : ago(d.last_seen)}</span></div>
      </div>
      <div class="row actions">
        <button class="primary" disabled={!app.online} onclick={() => pickAndSend(false)}>Envoyer des fichiers</button>
        {#if app.has("ring")}<button disabled={!app.online} onclick={ring}>Faire sonner</button>{/if}
        <button class="ghost" onclick={() => (app.page = "notifications")}>Notifications</button>
      </div>
      {#if !app.online}
        <p class="muted small">Ouvrez l'application sur le téléphone : il se reconnecte seul dès qu'il est sur le même réseau.</p>
      {/if}
    </div>
  </section>

  {#if app.live.length}
    <section class="recent">
      <h2>Dernières notifications</h2>
      {#each app.live.slice(0, 4) as n (n.key)}
        <div class="notif row">
          <span class="app small">{n.app_name}</span>
          <span class="grow ellipsis">{n.title ?? ""}{n.text ? ` — ${n.text}` : ""}</span>
        </div>
      {/each}
    </section>
  {/if}
{/if}

<style>
  .empty { display: flex; flex-direction: column; gap: 16px; max-width: 520px; padding-top: 60px; }
  .hero { display: flex; gap: 48px; align-items: center; flex-wrap: wrap; }
  .info { flex: 1; min-width: 300px; gap: 10px; }
  .facts { display: grid; grid-template-columns: repeat(3, auto); gap: 18px 32px; margin: 14px 0 8px; justify-content: start; }
  .facts div { display: flex; flex-direction: column; gap: 2px; }
  .actions { flex-wrap: wrap; margin-top: 6px; }
  .recent { margin-top: 48px; display: flex; flex-direction: column; gap: 10px; }
  .notif { padding: 12px 16px; background: var(--surface); border-radius: var(--r-mid); }
  .app { color: var(--muted); width: 120px; flex: none; }
  @media (max-width: 980px) { .facts { grid-template-columns: 1fr 1fr; } }
</style>
