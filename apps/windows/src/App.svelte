<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { check } from "@tauri-apps/plugin-updater";
  import { app, type Page } from "./lib/state.svelte";
  import { api } from "./lib/api";
  import Home from "./pages/Home.svelte";
  import Share from "./pages/Share.svelte";
  import Notifications from "./pages/Notifications.svelte";
  import Control from "./pages/Control.svelte";
  import Media from "./pages/Media.svelte";
  import Devices from "./pages/Devices.svelte";
  import SettingsPage from "./pages/Settings.svelte";
  import Mirror from "./pages/Mirror.svelte";
  import Icon, { type IconName } from "./components/Icon.svelte";
  import Logo from "./components/Logo.svelte";
  import { loadPrefs } from "./lib/prefs.svelte";
  import PairingDialog from "./components/PairingDialog.svelte";
  import PairingRequest from "./components/PairingRequest.svelte";
  import { pickAndSend } from "./lib/send";

  type NavItem = { id: Page; label: string; icon: IconName };
  // L'écran étendu se pilote depuis l'accueil.
  const NAV: NavItem[] = [
    { id: "home", label: "Accueil", icon: "home" },
    { id: "share", label: "Partage", icon: "share" },
    { id: "notifications", label: "Notifications", icon: "bell" },
    { id: "mirror", label: "Écran du téléphone", icon: "phone" },
    { id: "media", label: "Multimédia", icon: "play" },
    { id: "control", label: "Centre de contrôle", icon: "sliders" },
  ];
  const NAV_BOTTOM: NavItem[] = [
    { id: "devices", label: "Appareils", icon: "link" },
    { id: "settings", label: "Paramètres", icon: "gear" },
  ];

  let dragging = $state(false);
  let ready = $state(false);

  async function sendPaths(paths: string[]) {
    if (!app.selected || !app.online) {
      app.toast("Aucun téléphone connecté pour recevoir ces fichiers.", "error");
      return;
    }
    app.page = "share";
    await app.run(api.sendPaths(app.selected, paths));
  }

  onMount(() => {
    const un: (() => void)[] = [];
    Promise.all([app.init(), loadPrefs()]).then(() => (ready = true));
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "over" || e.payload.type === "enter") dragging = true;
        else if (e.payload.type === "leave") dragging = false;
        else if (e.payload.type === "drop") {
          dragging = false;
          if (e.payload.paths.length) sendPaths(e.payload.paths);
        }
      })
      .then((u) => un.push(u));
    listen<string[]>("nc-send-request", (e) => sendPaths(e.payload)).then((u) => un.push(u));
    listen("nc-pick-files", () => pickAndSend(false)).then((u) => un.push(u));
    // Vérification des mises à jour au démarrage (silencieuse si hors ligne).
    check().then((u) => { if (u) app.updateVersion = u.version; }).catch(() => {});
    return () => un.forEach((f) => f());
  });

  let unread = $derived(app.live.length);
  let battery = $derived(app.selected ? app.status[app.selected]?.battery_percent ?? null : null);
  let activeTransfers = $derived(Object.values(app.transfers).filter((t) => ["running", "offered", "paused"].includes(t.state)).length + app.incoming.length);
</script>

<div class="shell">
  <aside>
    <div class="brand">
      <Logo height={20} />
      <span class="dot word">NOTHING<br />CONNECT</span>
    </div>

    <div class="device">
      {#if app.devices.length > 1}
        <select aria-label="Téléphone" bind:value={app.selected}>
          {#each app.devices as d}
            <option value={d.id}>{d.custom_name || d.name}</option>
          {/each}
        </select>
      {/if}
      {#if app.device}
        <span class="ellipsis dname">{app.device.custom_name || app.device.name}</span>
        <div class="chips">
          <span class="chip" class:live-chip={app.online}>
            <span class={app.online ? "live" : "idle"}></span>
            {app.online ? (app.link?.link === "usb" ? "USB" : "Wi-Fi") : app.refused[app.device.id] ? "Révoqué" : "Hors ligne"}
          </span>
          {#if app.online && battery != null}<span class="chip"><span class="dot">{battery}</span>%</span>{/if}
        </div>
      {:else}
        <span class="muted small">Aucun téléphone associé</span>
      {/if}
    </div>

    {#snippet item(n: NavItem)}
      <button class="nav" class:active={app.page === n.id} aria-current={app.page === n.id ? "page" : undefined} onclick={() => (app.page = n.id)}>
        <Icon name={n.icon} />
        <span class="grow">{n.label}</span>
        {#if n.id === "notifications" && unread > 0}<span class="count">{unread}</span>{/if}
        {#if n.id === "share" && activeTransfers > 0}<span class="count">{activeTransfers}</span>{/if}
      </button>
    {/snippet}

    <nav aria-label="Modules">
      {#each NAV as n}{@render item(n)}{/each}
    </nav>

    <nav class="bottom" aria-label="Réglages">
      {#if app.settings?.suspended}
        <p class="suspended small">Connexions suspendues</p>
      {/if}
      {#each NAV_BOTTOM as n}{@render item(n)}{/each}
    </nav>
  </aside>

  <main>
    {#if app.updateVersion}
      <div class="update row">
        <span class="grow">La version {app.updateVersion} est disponible.</span>
        <button class="primary" onclick={() => (app.page = "settings")}>Voir la mise à jour</button>
      </div>
    {/if}
    {#if ready}
      {#if app.page === "home"}<Home />
      {:else if app.page === "share"}<Share />
      {:else if app.page === "notifications"}<Notifications />
      {:else if app.page === "control"}<Control />
      {:else if app.page === "media"}<Media />
      {:else if app.page === "mirror"}<Mirror />
      {:else if app.page === "devices"}<Devices />
      {:else}<SettingsPage />{/if}
    {/if}
  </main>
</div>

{#if dragging}
  <div class="drop" aria-hidden="true">
    <p class="dot">Déposer</p>
    <p class="muted">{app.online ? `Envoyer vers ${app.name(app.selected!)}` : "Aucun téléphone connecté"}</p>
  </div>
{/if}

{#if app.pairingOpen}<PairingDialog />{/if}
{#if app.pairingRequest}<PairingRequest />{/if}

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast" class:error={t.tone === "error"}>{t.text}</div>
  {/each}
</div>

<style>
  .shell { display: grid; grid-template-columns: 240px 1fr; height: 100%; }
  aside { background: #080808; border-right: 1px solid var(--line); padding: 24px 14px 18px; display: flex; flex-direction: column; gap: 22px; overflow-y: auto; }
  .brand { display: flex; align-items: center; gap: 12px; padding: 0 8px; }
  .word { font-size: 13px; line-height: 1.05; letter-spacing: 0.06em; }
  .device { display: flex; flex-direction: column; gap: 8px; padding: 14px; background: var(--surface); border-radius: var(--r-mid); }
  .dname { font-weight: 500; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; padding: 4px 9px; border-radius: var(--r-pill); border: 1px solid var(--line); color: var(--muted); }
  .chip.live-chip { color: var(--white); }
  .chip .dot { font-size: 13px; color: var(--white); }
  nav { display: flex; flex-direction: column; gap: 2px; }
  nav.bottom { margin-top: auto; }
  .nav { display: flex; gap: 12px; align-items: center; text-align: left; background: transparent; border-color: transparent; border-radius: 12px; padding: 10px 12px; color: var(--muted); }
  .nav:hover:not(.active) { color: var(--white); border-color: transparent; background: var(--surface); }
  .nav.active, .nav.active:hover { color: var(--white); background: var(--surface-2); border-color: transparent; }
  .nav.active :global(svg) { color: var(--red); }
  .count { font-size: 12px; min-width: 22px; text-align: center; padding: 2px 6px; border-radius: 10px; background: var(--red); color: var(--white); }
  .suspended { color: var(--red); padding: 0 12px 6px; }
  main { overflow-y: auto; padding: 36px 44px 60px; }
  main > :global(*) { max-width: 960px; }
  .update { background: var(--surface); border-radius: var(--r-mid); padding: 12px 12px 12px 18px; margin-bottom: 24px; }
  .drop { position: fixed; inset: 14px; border: 2px dashed var(--white); border-radius: var(--r-big); background: rgb(0 0 0 / 0.86); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; pointer-events: none; }
  .drop .dot { font-size: 64px; }
  .toasts { position: fixed; right: 20px; bottom: 20px; display: flex; flex-direction: column; gap: 8px; z-index: 10; }
  .toast { background: var(--white); color: var(--black); padding: 12px 16px; border-radius: var(--r-mid); max-width: 380px; font-size: 14px; }
  .toast.error { background: var(--red); color: var(--white); }
  @media (max-width: 860px) {
    .shell { grid-template-columns: 1fr; grid-template-rows: auto 1fr; }
    aside { border-right: none; border-bottom: 1px solid var(--line); flex-direction: row; flex-wrap: wrap; gap: 16px; padding: 16px; }
    nav { flex-direction: row; flex-wrap: wrap; }
    main { padding: 24px; }
  }
</style>
