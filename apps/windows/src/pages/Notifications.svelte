<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { api, type PhoneNotification } from "../lib/api";
  import { clock, ago } from "../lib/format";
  import Toggle from "../components/Toggle.svelte";

  let tab = $state<"live" | "history" | "apps">("live");
  let query = $state("");
  let replies = $state<Record<string, string>>({});
  let peer = $derived(app.selected);
  let apps = $derived((peer && app.apps[peer]) || []);
  let filteredApps = $derived(apps.filter((a) => a.app_name.toLowerCase().includes(query.toLowerCase()) || a.package.includes(query.toLowerCase())));
  let canAct = $derived(app.online && app.has("notification_actions"));

  function act(n: PhoneNotification, idx: number, withText: boolean) {
    if (!peer) return;
    const reply = withText ? replies[n.key] ?? "" : null;
    if (withText && !reply?.trim()) return app.toast("Écrivez une réponse d'abord.", "error");
    app.run(api.notificationAction(peer, n.key, idx, reply), withText ? "Réponse envoyée" : undefined).then(() => (replies[n.key] = ""));
  }
  async function toggleApp(pkg: string, on: boolean) {
    if (!peer) return;
    await app.run(api.setNotificationApp(peer, pkg, on));
    app.apps[peer] = await api.notificationApps(peer);
  }
</script>

<div class="stack page">
  <div class="row head">
    <h1 class="grow">Notifications</h1>
    <div class="tabs" role="tablist">
      <button role="tab" aria-selected={tab === "live"} class:on={tab === "live"} onclick={() => (tab = "live")}>Actives</button>
      <button role="tab" aria-selected={tab === "history"} class:on={tab === "history"} onclick={() => { tab = "history"; app.refreshHistory(); }}>Historique</button>
      <button role="tab" aria-selected={tab === "apps"} class:on={tab === "apps"} onclick={() => (tab = "apps")}>Applications</button>
    </div>
  </div>

  {#if !app.settings?.notifications_enabled}
    <p class="panel">La réception des notifications est désactivée. Réactivez-la dans Paramètres.</p>
  {/if}

  {#if tab === "live"}
    {#if !app.live.length}
      <p class="muted">Aucune notification active. Sur le téléphone, autorisez « Accès aux notifications » pour NOTHING CONNECT afin de les recevoir ici.</p>
    {/if}
    {#each app.live as n (n.key)}
      <article class="notif panel">
        <div class="row">
          {#if n.icon_png_b64}<img src={`data:image/png;base64,${n.icon_png_b64}`} alt="" width="24" height="24" />{/if}
          <span class="small muted grow">{n.app_name}</span>
          <span class="small muted">{clock(n.posted_at)}</span>
        </div>
        {#if n.title}<h3>{n.title}</h3>{/if}
        {#if n.text}<p class="body">{n.text}</p>{/if}
        {#if canAct}
          <div class="row wrap">
            {#each n.actions as a}
              {#if a.accepts_text}
                <input type="text" class="grow" placeholder="Réponse rapide" bind:value={replies[n.key]} onkeydown={(e) => e.key === "Enter" && act(n, a.index, true)} />
                <button class="primary" onclick={() => act(n, a.index, true)}>{a.title}</button>
              {:else}
                <button onclick={() => act(n, a.index, false)}>{a.title}</button>
              {/if}
            {/each}
            <button class="ghost" onclick={() => peer && app.run(api.notificationOpen(peer, n.key), "Ouverte sur le téléphone")}>Ouvrir sur le téléphone</button>
            <button class="ghost" onclick={() => peer && app.run(api.notificationDismiss(peer, n.key))}>Ignorer</button>
          </div>
        {/if}
      </article>
    {/each}
  {:else if tab === "history"}
    <div class="row">
      <p class="muted grow small">{app.history.length} notifications conservées (limite : {app.settings?.notification_history_limit}).</p>
      {#if app.history.length}<button class="ghost small" onclick={async () => { await app.run(api.clearNotifications()); app.refreshHistory(); }}>Effacer l'historique</button>{/if}
    </div>
    {#each app.history as h (h.id)}
      <div class="hist row">
        <span class="small muted appname">{h.app_name}</span>
        <span class="grow ellipsis">{h.title ?? "Contenu non conservé"}{h.body ? ` — ${h.body}` : ""}</span>
        <span class="small muted">{ago(h.posted_at)}</span>
      </div>
    {/each}
  {:else}
    <p class="muted">Choisissez les applications dont les notifications sont transmises. Le filtre est appliqué sur le téléphone : une application désactivée n'envoie rien au PC.</p>
    <input type="search" placeholder="Rechercher une application" bind:value={query} />
    {#if !apps.length}<p class="muted">La liste apparaît après la première connexion du téléphone avec l'accès aux notifications autorisé.</p>{/if}
    <div class="apps">
      {#each filteredApps as a (a.package)}
        <Toggle checked={a.enabled} label={a.app_name} hint={a.package} disabled={!app.online} onchange={(v) => toggleApp(a.package, v)} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { gap: 20px; }
  .head { flex-wrap: wrap; }
  .tabs { display: flex; gap: 4px; background: var(--surface); padding: 4px; border-radius: var(--r-pill); }
  .tabs button { border: none; background: transparent; color: var(--muted); }
  .tabs button.on { background: var(--white); color: var(--black); }
  .notif { display: flex; flex-direction: column; gap: 8px; padding: 18px 20px; }
  .notif img { border-radius: 6px; }
  .body { color: #d6d6d6; white-space: pre-line; }
  .wrap { flex-wrap: wrap; margin-top: 6px; }
  .hist { padding: 10px 4px; border-bottom: 1px solid var(--line); }
  .appname { width: 130px; flex: none; }
  .apps { display: flex; flex-direction: column; }
  .apps :global(.toggle) { border-bottom: 1px solid var(--line); }
</style>
