import { api, type DisplayStatus } from "./api";
import { app } from "./state.svelte";
import { prefs, savePrefs } from "./prefs.svelte";

/** État et commandes de l'écran étendu, partagés par l'accueil. */
class DisplayController {
  st = $state<DisplayStatus | null>(null);
  busy = $state(false);
  starting = $state(false);
  needReboot = $state(false);
  ownFailed = $state(false);
  private timer: ReturnType<typeof setInterval> | undefined;
  private users = 0;

  get active(): boolean {
    return !!this.st?.active_peer;
  }
  /** Fréquence maximale annoncée par le téléphone sélectionné. */
  get phoneHz(): number | null {
    const s = app.selected ? app.status[app.selected] : undefined;
    return s?.display_max_hz ?? null;
  }

  async refresh() {
    try {
      this.st = await api.displayStatus();
      if (this.st.active_peer) this.starting = false;
    } catch {
      /* hors Windows */
    }
  }

  /** Rafraîchissement : rapide pendant une session (mesures en direct), lent sinon. */
  watch(): () => void {
    this.users++;
    if (this.users === 1) {
      this.refresh();
      const tick = () => {
        clearInterval(this.timer);
        this.timer = setInterval(() => this.refresh().then(() => {
          const want = this.active || this.starting ? 1000 : 4000;
          if (want !== current) { current = want; tick(); }
        }), current);
      };
      let current = 4000;
      tick();
    }
    return () => {
      this.users--;
      if (this.users === 0) clearInterval(this.timer);
    };
  }

  async start() {
    if (!app.selected) return;
    const p = prefs.display;
    this.starting = true;
    const ok = await app.run(api.displayStart(app.selected, p.scale, p.quality, p.fps, p.portrait, p.touch, p.gpu));
    if (ok === undefined) this.starting = false;
    else app.toast("Ouverture de l'écran sur le téléphone…");
    setTimeout(() => { this.starting = false; this.refresh(); }, 8000);
    await this.refresh();
  }

  async stop() {
    await app.run(api.displayStop());
    this.starting = false;
    await this.refresh();
  }

  /** Changement de réglage : enregistré, et appliqué tout de suite à la session en cours
   *  (qualité, fréquence, toucher ; la définition et l'orientation valent au prochain branchement). */
  async change<K extends keyof typeof prefs.display>(key: K, value: (typeof prefs.display)[K]) {
    prefs.display[key] = value;
    savePrefs();
    if (this.active && (key === "quality" || key === "fps" || key === "touch" || key === "gpu")) {
      const p = prefs.display;
      await api.displayTune(p.scale, p.quality, p.fps, p.touch, p.gpu).catch(() => {});
    }
  }

  async setup(action: "install" | "uninstall" | "enable-testsigning" | "install-vdd") {
    this.busy = true;
    let r: string | undefined;
    try {
      r = await api.displaySetup(action);
    } catch (e) {
      if (action === "install") this.ownFailed = true;
      app.toast(String(e), "error");
    }
    this.busy = false;
    if (r === "redemarrage") this.needReboot = true;
    else if (r !== undefined) app.toast(action === "uninstall" ? "Pilote retiré" : "Pilote installé");
    await this.refresh();
  }
}

export const display = new DisplayController();
