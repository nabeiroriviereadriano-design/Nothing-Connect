<script lang="ts">
  import { app } from "../lib/state.svelte";
  import { api, type DeviceRecord } from "../lib/api";
  import { ago } from "../lib/format";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import Toggle from "../components/Toggle.svelte";

  let editing = $state<string | null>(null);
  let newName = $state("");

  async function save(d: DeviceRecord) {
    await app.run(api.renameDevice(d.id, newName.trim() || null));
    editing = null;
    app.refreshDevices();
  }
  async function remove(d: DeviceRecord) {
    const ok = await confirm(`Supprimer ${d.custom_name || d.name} ? Ses clés sont révoquées immédiatement : il faudra l'associer à nouveau.`, { title: "Supprimer l'appareil", kind: "warning", okLabel: "Supprimer", cancelLabel: "Garder" });
    if (ok) await app.run(api.removeDevice(d.id), "Appareil supprimé et révoqué");
  }
  async function auto(d: DeviceRecord, on: boolean) {
    await app.run(api.setAutoAccept(d.id, on));
    app.refreshDevices();
  }
</script>

<div class="stack page">
  <div class="row">
    <h1 class="grow">Appareils</h1>
    <button class="primary" onclick={() => (app.pairingOpen = true)}>Associer un téléphone</button>
  </div>
  <p class="muted">Seuls les appareils associés peuvent se connecter. Tout appareil inconnu est refusé.</p>

  {#each app.devices as d (d.id)}
    <article class="panel dev">
      <div class="row">
        <span class={app.isOnline(d.id) ? "live" : "idle"}></span>
        {#if editing === d.id}
          <input type="text" class="grow" bind:value={newName} placeholder={d.name} onkeydown={(e) => e.key === "Enter" && save(d)} />
          <button class="primary" onclick={() => save(d)}>Enregistrer</button>
          <button class="ghost" onclick={() => (editing = null)}>Annuler</button>
        {:else}
          <h2 class="grow">{d.custom_name || d.name}</h2>
          <button class="ghost" onclick={() => { editing = d.id; newName = d.custom_name ?? ""; }}>Renommer</button>
        {/if}
      </div>
      <dl>
        <dt>Modèle</dt><dd>{[d.manufacturer, d.model].filter(Boolean).join(" ") || "—"}</dd>
        <dt>Android</dt><dd>{d.os_version ?? "—"}</dd>
        <dt>Application</dt><dd>{d.app_version ?? "—"}</dd>
        <dt>État</dt><dd>{app.isOnline(d.id) ? "Connecté en Wi-Fi" : app.refused[d.id] ? "Révoqué" : `Hors ligne, vu ${ago(d.last_seen)}`}</dd>
        <dt>Associé</dt><dd>{new Date(d.paired_at).toLocaleDateString("fr-FR")}</dd>
      </dl>
      <Toggle checked={d.auto_accept} label="Recevoir ses fichiers automatiquement" hint="Sinon, chaque envoi demande votre accord." onchange={(v) => auto(d, v)} />
      <div><button class="danger" onclick={() => remove(d)}>Supprimer et révoquer</button></div>
    </article>
  {:else}
    <p class="panel">Aucun appareil associé.</p>
  {/each}
</div>

<style>
  .page { gap: 20px; }
  .dev { display: flex; flex-direction: column; gap: 14px; }
  dl { display: grid; grid-template-columns: 120px 1fr; gap: 6px 16px; margin: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; }
</style>
