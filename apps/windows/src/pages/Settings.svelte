<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { api, type Settings } from "../lib/api";
  import Toggle from "../components/Toggle.svelte";
  import { open, confirm } from "@tauri-apps/plugin-dialog";
  import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";

  let s = $derived(app.settings!);
  let autostart = $state(false);
  let defaultDir = $state("");
  let update = $state<Update | null>(null);
  let updState = $state<"idle" | "checking" | "none" | "downloading" | "error">("idle");
  let downloaded = $state(0);
  let total = $state(0);
  let updError = $state("");

  isEnabled().then((v) => (autostart = v)).catch(() => {});
  api.defaultDownloadDir().then((d) => (defaultDir = d));

  async function patch(p: Partial<Settings>) {
    const next = { ...s, ...p };
    await app.run(api.setSettings(next));
    app.settings = await api.getSettings();
  }
  async function pickDir() {
    const d = await open({ directory: true, title: "Dossier de réception" });
    if (typeof d === "string") patch({ download_dir: d });
  }
  async function setAutostart(on: boolean) {
    await app.run(on ? enable() : disable());
    autostart = await isEnabled();
  }
  async function checkUpdate() {
    updState = "checking";
    try {
      update = await check();
      updState = update ? "idle" : "none";
      app.updateVersion = update?.version ?? null;
    } catch (e) {
      updState = "error";
      updError = String(e);
    }
  }
  async function installUpdate() {
    if (!update) return;
    const ok = await confirm(`Installer la version ${update.version} ? L'application redémarrera ; vos appareils et réglages sont conservés.`, { title: "Mise à jour", okLabel: "Installer", cancelLabel: "Plus tard" });
    if (!ok) return;
    updState = "downloading";
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") downloaded += ev.data.chunkLength;
      });
      await relaunch();
    } catch (e) {
      updState = "error";
      updError = `Échec : ${e}. La version actuelle reste installée.`;
    }
  }
  async function wipe() {
    const ok = await confirm("Tout effacer ? Appareils associés, historique, réglages et identité de ce PC seront supprimés. Les fichiers déjà reçus sont conservés.", { title: "Effacer les données", kind: "warning", okLabel: "Tout effacer", cancelLabel: "Annuler" });
    if (ok) await app.run(api.wipeAll());
  }
</script>

<div class="stack page">
  <h1>Paramètres</h1>

  <section class="panel">
    <h2>Connexion</h2>
    <Toggle checked={s.suspended} label="Suspendre toutes les connexions" hint="Aucun téléphone ne peut se connecter tant que c'est activé." onchange={(v) => patch({ suspended: v })} />
    <Toggle checked={autostart} label="Démarrer avec Windows" hint="Démarre réduit dans la zone de notification." onchange={setAutostart} />
    <Toggle checked={s.clipboard_sync} label="Synchroniser le presse-papiers" hint="Le texte copié sur le PC est envoyé au téléphone, et inversement depuis l'appli Android. Désactivé par défaut." onchange={(v) => patch({ clipboard_sync: v })} />
    <Toggle checked={s.usb_adb} label="Liaison par câble USB" hint="Utilise adb (débogage USB) quand le téléphone est branché : liaison prioritaire sur le Wi-Fi." onchange={(v) => patch({ usb_adb: v })} />
    <p class="muted small">
      {#if !app.usb?.adb}adb introuvable : installez « SDK Platform-Tools » de Google pour la liaison USB.
      {:else if !app.usb.devices.length}adb trouvé. Aucun téléphone branché en USB.
      {:else}{#each app.usb.devices as [serial, state]}{serial} : {state === "device" ? (app.usb.tunneled.includes(serial) ? "liaison USB active" : "prêt") : state === "unauthorized" ? "acceptez « Autoriser le débogage USB » sur le téléphone" : state}. {/each}{/if}
    </p>
    <Toggle checked={s.ble_advertise} label="Annonce Bluetooth" hint="Aide le téléphone à trouver le PC si le réseau bloque la découverte. Pris en compte au prochain démarrage." onchange={(v) => patch({ ble_advertise: v })} />
    <p class="muted small">Réseau local uniquement, sans Internet ni compte. Port d'écoute : {app.info?.port ?? "—"}. Au premier lancement, autorisez NOTHING CONNECT dans le pare-feu Windows pour les réseaux privés.</p>
  </section>

  <section class="panel">
    <h2>Fichiers reçus</h2>
    <div class="row line">
      <div class="grow stack tight"><span>Dossier de réception</span><span class="muted small ellipsis">{s.download_dir || defaultDir}</span></div>
      <button onclick={pickDir}>Changer</button>
      {#if s.download_dir}<button class="ghost" onclick={() => patch({ download_dir: null })}>Par défaut</button>{/if}
    </div>
    <div class="row line">
      <label class="grow" for="conflict">Si un fichier du même nom existe déjà</label>
      <select id="conflict" class="sel" value={s.conflict_policy} onchange={(e) => patch({ conflict_policy: (e.currentTarget as HTMLSelectElement).value as Settings["conflict_policy"] })}>
        <option value="keep_both">Conserver les deux</option>
        <option value="replace">Remplacer</option>
        <option value="skip">Ne pas recevoir</option>
      </select>
    </div>
    <p class="muted small">Un fichier identique (même empreinte SHA-256) n'est jamais dupliqué.</p>
  </section>

  <section class="panel">
    <h2>Notifications</h2>
    <Toggle checked={s.notifications_enabled} label="Recevoir les notifications du téléphone" onchange={(v) => patch({ notifications_enabled: v })} />
    <Toggle checked={s.notification_popups} label="Afficher une fenêtre Windows" disabled={!s.notifications_enabled} onchange={(v) => patch({ notification_popups: v })} />
    <Toggle checked={s.hide_sensitive_content} label="Masquer le contenu sensible" hint="Pour les notifications marquées privées par Android (codes, messages bancaires)." onchange={(v) => patch({ hide_sensitive_content: v })} />
    <Toggle checked={s.store_notification_content} label="Conserver le texte dans l'historique" hint="Désactivé : seuls l'application et l'heure sont conservés." onchange={(v) => patch({ store_notification_content: v })} />
    <div class="row line">
      <label class="grow" for="limit">Taille de l'historique</label>
      <select id="limit" class="sel" value={String(s.notification_history_limit)} onchange={(e) => patch({ notification_history_limit: Number((e.currentTarget as HTMLSelectElement).value) })}>
        {#each [100, 500, 2000] as n}<option value={String(n)}>{n} notifications</option>{/each}
      </select>
    </div>
  </section>

  <section class="panel">
    <h2>Mises à jour</h2>
    <div class="row line">
      <div class="grow stack tight">
        <span>Version installée : <span class="dot">{app.info?.version}</span></span>
        <span class="muted small">
          {#if updState === "checking"}Recherche…
          {:else if updState === "none"}Vous avez la dernière version.
          {:else if updState === "downloading"}Téléchargement {total ? Math.round((downloaded / total) * 100) : 0} %
          {:else if updState === "error"}{updError}
          {:else if update}Version {update.version} disponible.
          {:else}Les mises à jour sont signées et vérifiées avant installation.{/if}
        </span>
      </div>
      {#if update && updState !== "downloading"}
        <button class="primary" onclick={installUpdate}>Installer {update.version}</button>
      {:else}
        <button disabled={updState === "checking" || updState === "downloading"} onclick={checkUpdate}>Rechercher</button>
      {/if}
    </div>
    {#if update?.body}<pre class="notes small">{update.body}</pre>{/if}
  </section>

  <section class="panel">
    <h2>Diagnostic et confidentialité</h2>
    <p class="small muted">Empreinte de ce PC : {app.info?.fingerprint}. Protocole v{app.info?.protocol}. Aucune donnée n'est envoyée sur Internet ; les journaux ne contiennent ni clés ni contenus.</p>
    <div class="row wrap">
      <button onclick={() => app.info && revealItemInDir(app.info.logs_dir)}>Ouvrir les journaux</button>
      <button class="danger" onclick={wipe}>Effacer toutes les données</button>
    </div>
  </section>
</div>

<style>
  .page { gap: 20px; }
  .panel { display: flex; flex-direction: column; gap: 4px; }
  .panel h2 { margin-bottom: 6px; }
  .line { padding: 10px 0; }
  .tight { gap: 0; }
  .sel { width: auto; }
  .wrap { flex-wrap: wrap; margin-top: 10px; }
  .notes { white-space: pre-wrap; background: var(--black); padding: 12px; border-radius: 12px; margin: 8px 0 0; font-family: var(--ui); }
</style>
