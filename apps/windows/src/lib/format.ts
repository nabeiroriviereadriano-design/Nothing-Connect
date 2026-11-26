const UNITS = ["o", "Ko", "Mo", "Go", "To"];

export function bytes(n: number | null | undefined): string {
  if (n == null) return "—";
  let i = 0;
  let v = n;
  while (v >= 1000 && i < UNITS.length - 1) {
    v /= 1000;
    i++;
  }
  return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${UNITS[i]}`.replace(".", ",");
}

export function speed(bps: number): string {
  return bps > 0 ? `${bytes(bps)}/s` : "—";
}

export function duration(s: number | null | undefined): string {
  if (s == null) return "—";
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min ${s % 60} s`;
  return `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min`;
}

const rtf = new Intl.RelativeTimeFormat("fr", { numeric: "auto" });

export function ago(ms: number | null | undefined): string {
  if (!ms) return "jamais";
  const diff = (ms - Date.now()) / 1000;
  const abs = Math.abs(diff);
  if (abs < 45) return "à l'instant";
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
  return rtf.format(Math.round(diff / 86400), "day");
}

export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
}

export const STATE_LABEL: Record<string, string> = {
  offered: "En attente du destinataire",
  waiting: "À accepter",
  running: "En cours",
  paused: "En pause",
  interrupted: "Interrompu — reprise à la reconnexion",
  completed: "Terminé",
  failed: "Échec",
  cancelled: "Annulé",
  rejected: "Refusé",
};
