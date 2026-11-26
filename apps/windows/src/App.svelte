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
  import Display from "./pages/Display.svelte";
  import PairingDialog from "./components/PairingDialog.svelte";
  import PairingRequest from "./components/PairingRequest.svelte";
  import { pickAndSend } from "./lib/send";

  const NAV: { id: Page; label: string }[] = [
    { id: "home", label: "Accueil" },
    { id: "share", label: "Partage" },
    { id: "notifications", label: "Notifications" },
    { id: "control", label: "Centre de contrôle" },
    { id: "media", label: "Multimédia" },
    { id: "mirror", label: "Écran du téléphone" },
    { id: "display", label: "Écran étendu" },
    { id: "devices", label: "Appareils" },
    { id: "settings", label: "Paramètres" },
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
    app.init().then(() => (ready = true));
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
  let activeTransfers = $derived(Object.values(app.transfers).filter((t) => ["running", "offered", "paused"].includes(t.state)).length + app.incoming.length);
</script>

<div class="shell">
  <aside>
    <div class="brand dot">NOTHING<br />CONNECT</div>

    <div class="device">
      {#if app.devices.length > 1}
        <label class="muted small" for="dev">Téléphone</label>
        <select id="dev" bind:value={app.selected}>
          {#each app.devices as d}
            <option value={d.id}>{d.custom_name || d.name}</option>
          {/each}
        </select>
      {/if}
      {#if app.device}
        <div class="row small">
          <span class={app.online ? "live" : "idle"}></span>
          <span class="ellipsis">{app.device.custom_name || app.device.name}</span>
        </div>
        <span class="muted small">{app.online ? (app.link?.link === "usb" ? "Connecté par câble USB" : "Connecté sur le réseau local") : app.refused[app.device.id] ? "Association révoquée" : "Hors ligne"}</span>
      {:else}
        <span class="muted small">Aucun téléphone associé</span>
      {/if}
    </div>

    <nav aria-label="Modules">
      {#each NAV as n}
        <button class="nav" class:active={app.page === n.id} aria-current={app.page === n.id ? "page" : undefined} onclick={() => (app.page = n.id)}>
          <span>{n.label}</span>
          {#if n.id === "notifications" && unread > 0}<span class="count">{unread}</span>{/if}
          {#if n.id === "share" && activeTransfers > 0}<span class="count">{activeTransfers}</span>{/if}
        </button>
      {/each}
    </nav>

    {#if app.settings?.suspended}
      <p class="suspended small">Connexions suspendues</p>
    {/if}
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
      {:else if app.page === "display"}<Display />
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
  .shell { display: grid; grid-template-columns: 232px 1fr; height: 100%; }
  aside { border-right: 1px solid var(--line); padding: 28px 18px; display: flex; flex-direction: column; gap: 28px; overflow-y: auto; }
  .brand { font-size: 24px; line-height: 1.05; }
  .device { display: flex; flex-direction: column; gap: 6px; }
  nav { display: flex; flex-direction: column; gap: 2px; }
  .nav { display: flex; justify-content: space-between; align-items: center; text-align: left; background: transparent; border-color: transparent; border-radius: 12px; padding: 11px 12px; color: var(--muted); }
  .nav:hover { color: var(--white); border-color: transparent; background: var(--surface); }
  .nav.active { color: var(--white); background: var(--surface-2); }
  .count { font-size: 12px; min-width: 22px; text-align: center; padding: 2px 6px; border-radius: 10px; background: var(--red); color: var(--white); }
  .suspended { margin-top: auto; color: var(--red); }
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
