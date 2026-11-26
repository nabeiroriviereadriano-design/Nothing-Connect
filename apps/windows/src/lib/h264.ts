/** Utilitaires H.264 (flux Annex B envoyé par MediaCodec). */

/** Positions des unités NAL (après les codes de début 00 00 01 / 00 00 00 01). */
export function nalUnits(b: Uint8Array): { type: number; start: number }[] {
  const out: { type: number; start: number }[] = [];
  for (let i = 0; i + 3 < b.length; i++) {
    if (b[i] === 0 && b[i + 1] === 0 && b[i + 2] === 1) {
      out.push({ type: b[i + 3] & 0x1f, start: i + 3 });
      i += 3;
    }
  }
  return out;
}

/** Chaîne de codec WebCodecs (« avc1.PPCCLL ») déduite du SPS. */
export function codecString(config: Uint8Array): string | null {
  const sps = nalUnits(config).find((n) => n.type === 7);
  if (!sps || sps.start + 3 >= config.length) return null;
  const hex = (v: number) => v.toString(16).padStart(2, "0");
  return `avc1.${hex(config[sps.start + 1])}${hex(config[sps.start + 2])}${hex(config[sps.start + 3])}`;
}

export function isKeyframe(b: Uint8Array): boolean {
  return nalUnits(b).some((n) => n.type === 5);
}

export function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const r = new Uint8Array(a.length + b.length);
  r.set(a, 0);
  r.set(b, a.length);
  return r;
}
