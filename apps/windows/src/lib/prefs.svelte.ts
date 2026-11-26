import { api } from "./api";

/** Réglages de l'interface, enregistrés automatiquement à chaque changement. */
export interface DisplayPrefs {
  /** Part de la définition du téléphone (0,5 / 0,75 / 1). */
  scale: number;
  /** 30 à 95 : débit vidéo (H.264) ou qualité JPEG. */
  quality: number;
  /** Images par seconde ; 0 = fréquence maximale du téléphone. */
  fps: number;
  portrait: boolean;
  /** Toucher du téléphone : écran tactile Windows (gestes) ou souris. */
  touch: boolean;
}
export interface MirrorPrefs { maxSize: number; bitrate: number; withAudio: boolean }
export interface Prefs { display: DisplayPrefs; mirror: MirrorPrefs }

const DEFAULTS: Prefs = {
  display: { scale: 0.75, quality: 70, fps: 0, portrait: false, touch: true },
  mirror: { maxSize: 1280, bitrate: 8_000_000, withAudio: true },
};

export const prefs = $state<Prefs>(structuredClone(DEFAULTS));
let loaded = false;
let timer: ReturnType<typeof setTimeout> | undefined;

/** Fusion avec les valeurs par défaut : un réglage ajouté dans une mise à jour garde sa valeur par défaut. */
export async function loadPrefs() {
  try {
    const saved = (await api.uiPrefsGet()) as Partial<Prefs>;
    prefs.display = { ...DEFAULTS.display, ...(saved.display ?? {}) };
    prefs.mirror = { ...DEFAULTS.mirror, ...(saved.mirror ?? {}) };
  } catch {
    // Premier lancement ou fichier illisible : valeurs par défaut.
  }
  loaded = true;
}

/** À appeler après une modification : écrit sur le disque 300 ms plus tard (regroupe les changements). */
export function savePrefs() {
  if (!loaded) return;
  clearTimeout(timer);
  timer = setTimeout(() => {
    api.uiPrefsSet($state.snapshot(prefs)).catch(() => {});
  }, 300);
}
