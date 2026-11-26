<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { app } from "../lib/state.svelte";
  import { display } from "../lib/display.svelte";
  import { prefs } from "../lib/prefs.svelte";
  import Segmented from "./Segmented.svelte";
  import Icon from "./Icon.svelte";

  let st = $derived(display.st);
  let p = $derived(prefs.display);
  let hz = $derived(display.phoneHz);
  let fpsOptions = $derived.by(() => {
    const o: { value: number; label: string; hint?: string }[] = [{ value: 30, label: "30" }, { value: 60, label: "60" }];
    if (hz && hz > 90) o.push({ value: 90, label: "90" });
    o.push({ value: 0, label: hz && hz > 60 ? `Max (${hz} Hz)` : "Max du téléphone", hint: "Fréquence maximale de l'écran du téléphone" });
    return o;
  });
  let canStart = $derived(app.online && app.has("extended_display") && !!st?.installed);

  async function enableTestSigning() {
    const ok = await confirm(
      "Le mode « signature de test » permet à Windows de charger un pilote qui n'est pas signé par Microsoft. Il affiche « Mode test » en bas de l'écran et demande un redémarrage. Avec le démarrage sécurisé (Secure Boot), il faut d'abord le désactiver dans le BIOS. Continuer ?",
      { title: "Activer la signature de test", kind: "warning", okLabel: "Activer", cancelLabel: "Annuler" },
    );
    if (ok) await display.setup("enable-testsigning");
  }

  onMount(() => {
    const unwatch = display.watch();
    const un = listen<string>("nc-display-error", (e) => {
      display.starting = false;
      app.toast(e.payload, "error");
    });
    return () => {
      unwatch();
      un.then((f) => f());
    };
  });
</script>

<section class="ext" class:on={display.active} aria-labelledby="ext-title">
  <header>
    <span class="glyph"><Icon name="monitor" size={22} /></span>
    <div class="title">
      <h2 id="ext-title">Écran étendu</h2>
      <p class="muted small">
        {#if display.active}Le téléphone est un écran de Windows. Touchez-le pour cliquer et glisser.
        {:else if display.starting}Acceptez l'ouverture sur le téléphone…
        {:else}Le téléphone devient un second écran, à placer dans Paramètres › Système › Écran.{/if}
      </p>
    </div>
    {#if st?.supported && st.installed}
      {#if display.active}
        <button class="danger big" onclick={() => display.stop()}>Débrancher</button>
      {:else}
        <button class="primary big" disabled={!canStart || display.starting} onclick={() => display.start()}>
          {display.starting ? "Ouverture…" : "Brancher l'écran"}
        </button>
      {/if}
    {/if}
  </header>

  {#if !st}
    <p class="muted small">Vérification du pilote…</p>
  {:else if !st.supported}
    <p class="muted small">Disponible uniquement sous Windows.</p>
  {:else if !st.installed}
    <div class="driver">
      <p>Un pilote d'écran virtuel est nécessaire, une seule fois.</p>
      <div class="row wrap">
        {#if st.setup_available}
          <button class="primary" disabled={display.busy} onclick={() => display.setup("install")}>{display.busy ? "Installation…" : "Installer le pilote NOTHING CONNECT"}</button>
        {/if}
        <button disabled={display.busy} onclick={() => display.setup("install-vdd")}>Installer Virtual Display Driver</button>
      </div>
      <p class="muted small">Le pilote intégré s'installe sans mode test avec son propre certificat ; Windows demande une autorisation. Virtual Display Driver (libre, signé) est l'alternative si Windows refuse le premier.</p>
      {#if display.ownFailed && !st.testsigning}
        <p class="muted small">Ce PC refuse le pilote intégré. Dernier recours : <button class="link" onclick={enableTestSigning}>activer la signature de test</button>.</p>
      {/if}
      {#if display.needReboot}<p class="warn small">Redémarrez le PC pour terminer l'installation.</p>{/if}
    </div>
  {:else}
    {#if display.active}
      <div class="stats" aria-live="polite">
        <div><span class="dot n">{st.fps}</span><span class="muted small">images/s</span></div>
        <div><span class="dot n">{st.refresh_hz || "—"}</span><span class="muted small">Hz</span></div>
        <div><span class="dot n">{(st.kbps / 1000).toFixed(st.kbps < 10000 ? 1 : 0)}</span><span class="muted small">Mb/s</span></div>
        <div><span class="dot n res">{st.width}×{st.height}</span><span class="muted small">{st.codec === "h264" ? "vidéo H.264" : "images JPEG"} · {app.name(st.active_peer!)}</span></div>
      </div>
    {:else if !app.has("extended_display") && app.device}
      <p class="muted small">Mettez à jour l'application Android pour utiliser l'écran étendu.</p>
    {/if}

    <div class="settings">
      <Segmented label="Définition" value={p.scale} onchange={(v) => display.change("scale", v)} options={[
        { value: 0.5, label: "50 %", hint: "Le plus fluide" },
        { value: 0.75, label: "75 %", hint: "Équilibré" },
        { value: 1, label: "100 %", hint: "Définition native du téléphone" },
      ]} />
      <Segmented label="Qualité" value={p.quality} onchange={(v) => display.change("quality", v)} options={[
        { value: 45, label: "Économie" },
        { value: 70, label: "Standard" },
        { value: 85, label: "Haute" },
        { value: 95, label: "Max" },
      ]} />
      <Segmented label="Images par seconde" value={p.fps} onchange={(v) => display.change("fps", v)} options={fpsOptions} />
      <Segmented label="Orientation" value={p.portrait} onchange={(v) => display.change("portrait", v)} options={[
        { value: false, label: "Paysage" },
        { value: true, label: "Portrait" },
      ]} />
    </div>
    <p class="muted small foot">
      <span>Réglages enregistrés automatiquement.{#if display.active}{" "}Qualité et images/s s'appliquent tout de suite ; définition et orientation au prochain branchement.{/if}</span>
      {#if st.backend === "nothing-connect"}<button class="link" disabled={display.busy || display.active} onclick={() => display.setup("uninstall")}>Retirer le pilote</button>{/if}
    </p>
  {/if}
</section>

<style>
  .ext { background: var(--surface); border-radius: var(--r-big); padding: 24px; display: flex; flex-direction: column; gap: 20px; border: 1px solid transparent; transition: border-color 0.3s; }
  .ext.on { border-color: var(--white); }
  header { display: flex; align-items: center; gap: 14px; }
  .glyph { width: 44px; height: 44px; border-radius: 50%; background: var(--black); display: grid; place-items: center; flex: none; }
  .ext.on .glyph { color: var(--red); }
  .title { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .big { padding: 13px 22px; font-size: 15px; }
  .stats { display: grid; grid-template-columns: repeat(3, auto) 1fr; gap: 8px 36px; align-items: end; padding: 4px 2px; }
  .stats div { display: flex; flex-direction: column; gap: 4px; }
  .n { font-size: 44px; line-height: 1; font-variant-numeric: tabular-nums; }
  .n.res { font-size: 26px; }
  .settings { display: flex; flex-wrap: wrap; gap: 18px 28px; }
  .foot { display: flex; flex-wrap: wrap; gap: 6px 14px; align-items: baseline; }
  .driver { display: flex; flex-direction: column; gap: 12px; }
  .wrap { flex-wrap: wrap; }
  .warn { color: var(--red); }
  .link { background: none; border: none; padding: 0; color: var(--white); text-decoration: underline; text-underline-offset: 3px; font-size: inherit; }
  .link:disabled { opacity: 0.5; }
  .foot .link { color: var(--muted); }
  @media (max-width: 900px) { .stats { grid-template-columns: repeat(3, auto); } .stats div:last-child { grid-column: 1 / -1; } }
</style>
