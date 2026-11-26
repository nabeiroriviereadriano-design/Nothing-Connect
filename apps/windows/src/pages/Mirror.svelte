<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Channel } from "@tauri-apps/api/core";
  import { app } from "../lib/state.svelte";
  import { api, type InputEvent } from "../lib/api";
  import { codecString, concat } from "../lib/h264";

  const CONFIG = 0x01;
  const KEYFRAME = 0x02;

  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D | null = null;
  let decoder: VideoDecoder | null = null;
  let config: Uint8Array | null = null;
  let waitingKey = true;
  let running = $state(false);
  let quality = $state(1280);
  let bitrate = $state(8_000_000);
  let withAudio = $state(true);
  let fps = $state(0);
  let kbps = $state(0);
  let size = $state<{ w: number; h: number } | null>(null);
  let text = $state("");
  let frames = 0;
  let bytes = 0;
  let statTimer: ReturnType<typeof setInterval> | undefined;
  const supported = typeof VideoDecoder !== "undefined";
  let peer = $derived(app.selected);
  let stopped = $derived(app.streamStopped.mirror);

  function resetDecoder() {
    try { decoder?.close(); } catch {}
    decoder = null;
    waitingKey = true;
  }

  function ensureDecoder(): VideoDecoder | null {
    if (decoder) return decoder;
    if (!config) return null;
    const codec = codecString(config);
    if (!codec) return null;
    decoder = new VideoDecoder({
      output: (frame) => {
        if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
          canvas.width = frame.displayWidth;
          canvas.height = frame.displayHeight;
          size = { w: frame.displayWidth, h: frame.displayHeight };
        }
        ctx?.drawImage(frame, 0, 0);
        frame.close();
        frames++;
      },
      error: (e) => {
        console.warn("décodeur", e);
        resetDecoder();
        if (peer) api.mirrorKeyframe(peer).catch(() => {});
      },
    });
    decoder.configure({ codec, optimizeForLatency: true, hardwareAcceleration: "prefer-hardware" });
    return decoder;
  }

  function onPacket(buf: ArrayBuffer) {
    const v = new Uint8Array(buf);
    const flags = v[0];
    const ts = Number(new DataView(buf, 1, 8).getBigUint64(0, true));
    const data = v.subarray(9);
    bytes += data.length;
    if (flags & CONFIG) {
      const changed = !config || config.length !== data.length || config.some((b, i) => b !== data[i]);
      config = data.slice();
      if (changed) resetDecoder(); // nouvelle résolution ou rotation
      return;
    }
    const key = (flags & KEYFRAME) !== 0;
    if (waitingKey && !key) return;
    const d = ensureDecoder();
    if (!d) return;
    // Retard qui s'accumule : on saute jusqu'à la prochaine image clé plutôt que d'afficher en retard.
    if (d.decodeQueueSize > 4 && !key) {
      waitingKey = true;
      if (peer) api.mirrorKeyframe(peer).catch(() => {});
      return;
    }
    waitingKey = false;
    d.decode(new EncodedVideoChunk({ type: key ? "key" : "delta", timestamp: ts, data: key && config ? concat(config, data) : data }));
  }

  async function start() {
    if (!peer) return;
    delete app.streamStopped.mirror;
    resetDecoder();
    config = null;
    const ok = await app.run(api.mirrorStart(peer, quality, bitrate, withAudio));
    if (ok !== undefined) {
      running = true;
      app.toast("Acceptez la diffusion de l'écran sur le téléphone.");
    }
  }

  async function stop() {
    running = false;
    resetDecoder();
    if (peer) await app.run(api.mirrorStop(peer));
  }

  $effect(() => {
    if (stopped !== undefined && running) {
      running = false;
      resetDecoder();
    }
  });

  // ---- contrôle à la souris ----
  let down: { x: number; y: number; t: number } | null = null;
  function norm(e: PointerEvent | WheelEvent) {
    const r = canvas.getBoundingClientRect();
    return { x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)), y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)) };
  }
  function send(ev: InputEvent) {
    if (peer && app.has("mirror_control")) api.mirrorInput(peer, ev).catch((e) => app.toast(String(e), "error"));
  }
  function pointerDown(e: PointerEvent) {
    canvas.setPointerCapture(e.pointerId);
    down = { ...norm(e), t: performance.now() };
  }
  function pointerUp(e: PointerEvent) {
    if (!down) return;
    const p = norm(e);
    const dt = performance.now() - down.t;
    const dist = Math.hypot(p.x - down.x, p.y - down.y);
    if (dist < 0.01) send(dt > 450 ? { i: "long_press", x: down.x, y: down.y } : { i: "tap", x: down.x, y: down.y });
    else send({ i: "swipe", x1: down.x, y1: down.y, x2: p.x, y2: p.y, duration_ms: Math.round(Math.min(2000, Math.max(80, dt))) });
    down = null;
  }
  let wheelAcc = 0;
  let wheelTimer: ReturnType<typeof setTimeout> | undefined;
  function wheel(e: WheelEvent) {
    e.preventDefault();
    wheelAcc += e.deltaY;
    const p = norm(e);
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => {
      const dy = Math.max(-0.6, Math.min(0.6, -wheelAcc / 1500));
      wheelAcc = 0;
      send({ i: "swipe", x1: p.x, y1: Math.min(0.9, Math.max(0.1, p.y)), x2: p.x, y2: Math.min(0.95, Math.max(0.05, p.y + dy)), duration_ms: 150 });
    }, 90);
  }
  function sendText() {
    if (text) {
      send({ i: "text", text });
      text = "";
    }
  }

  onMount(() => {
    ctx = canvas.getContext("2d", { alpha: false, desynchronized: true }) as CanvasRenderingContext2D | null;
    const ch = new Channel<ArrayBuffer>();
    ch.onmessage = onPacket;
    api.mirrorSubscribe(ch);
    statTimer = setInterval(() => {
      fps = frames;
      kbps = Math.round((bytes * 8) / 1000);
      frames = 0;
      bytes = 0;
    }, 1000);
  });
  onDestroy(() => {
    clearInterval(statTimer);
    if (running && peer) api.mirrorStop(peer).catch(() => {});
    resetDecoder();
  });
</script>

<div class="stack page">
  <div class="row head">
    <h1 class="grow">Écran du téléphone</h1>
    {#if running}
      <span class="small muted dot">{fps} i/s</span>
      <span class="small muted">{(kbps / 1000).toFixed(1)} Mb/s</span>
      <button onclick={() => canvas.requestFullscreen()}>Plein écran</button>
      <button class="danger" onclick={stop}>Arrêter</button>
    {/if}
  </div>

  {#if !supported}
    <p class="panel">Le décodage vidéo n'est pas disponible dans ce moteur d'affichage. Sous Windows, il est fourni par WebView2 (Microsoft Edge).</p>
  {:else if !app.has("screen_mirror")}
    <p class="panel">Ce téléphone n'annonce pas la diffusion d'écran. Mettez à jour l'application Android.</p>
  {:else if !running}
    <section class="panel stack">
      <p class="muted">Android demandera votre accord sur le téléphone à chaque démarrage. Les contenus protégés (vidéos avec DRM, applis bancaires) apparaissent en noir : c'est une protection du système.</p>
      <div class="row wrap">
        <label class="row small">Qualité
          <select bind:value={quality}>
            <option value={960}>Économique (960 px)</option>
            <option value={1280}>Standard (1280 px)</option>
            <option value={1920}>Haute (1920 px)</option>
          </select>
        </label>
        <label class="row small">Débit
          <select bind:value={bitrate}>
            <option value={4_000_000}>4 Mb/s</option>
            <option value={8_000_000}>8 Mb/s</option>
            <option value={16_000_000}>16 Mb/s</option>
          </select>
        </label>
        <label class="row small"><input type="checkbox" bind:checked={withAudio} /> Son du téléphone sur le PC</label>
      </div>
      <div><button class="primary" disabled={!app.online} onclick={start}>Afficher l'écran</button></div>
    </section>
  {/if}

  <div class="screen" class:hidden={!running}>
    <canvas bind:this={canvas} width="720" height="1280" onpointerdown={pointerDown} onpointerup={pointerUp} onwheel={wheel} aria-label="Écran du téléphone"></canvas>
  </div>

  {#if running}
    {#if app.has("mirror_control")}
      <div class="row wrap">
        <button onclick={() => send({ i: "key", key: "back" })}>Retour</button>
        <button onclick={() => send({ i: "key", key: "home" })}>Accueil</button>
        <button onclick={() => send({ i: "key", key: "recents" })}>Applis récentes</button>
        <button onclick={() => send({ i: "key", key: "notifications" })}>Notifications</button>
        <input type="text" class="grow" placeholder="Taper dans le champ sélectionné du téléphone" bind:value={text} onkeydown={(e) => e.key === "Enter" && sendText()} />
        <button onclick={sendText}>Envoyer</button>
      </div>
      <p class="muted small">Clic : toucher. Clic long : appui long. Glisser : balayer. Molette : faire défiler.</p>
    {:else}
      <p class="muted small">Pour piloter le téléphone à la souris, activez « NOTHING CONNECT — contrôle à distance » dans Paramètres Android › Accessibilité.</p>
    {/if}
    {#if size}<p class="muted small">{size.w} × {size.h}</p>{/if}
  {/if}
</div>

<style>
  .page { gap: 18px; }
  .head { flex-wrap: wrap; }
  .wrap { flex-wrap: wrap; }
  .screen { display: flex; justify-content: center; background: var(--surface); border-radius: var(--r-big); padding: 16px; }
  .screen.hidden { display: none; }
  canvas { max-height: 70vh; max-width: 100%; border-radius: 18px; background: #000; touch-action: none; cursor: pointer; }
  canvas:fullscreen { max-height: none; border-radius: 0; object-fit: contain; }
  select { width: auto; }
</style>
