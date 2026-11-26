import { listen } from "@tauri-apps/api/event";
import {
  api,
  type AppEntry, type AppInfo, type CoreEvent, type DeviceRecord, type DeviceStatus, type MediaSession,
  type NotificationRecord, type PeerLink, type PhoneNotification, type Settings, type TransferRecord, type TransferState,
} from "./api";

export type Page = "home" | "share" | "notifications" | "control" | "media" | "devices" | "settings";

export interface LiveTransfer {
  id: string;
  peer_id: string;
  outgoing: boolean;
  state: TransferState;
  bytes_done: number;
  total_bytes: number;
  speed_bps: number;
  eta_s: number | null;
  current_file: string | null;
  error: string | null;
  label: string;
}

export interface Incoming { id: string; peer_id: string; files: { rel_path: string; size: number }[]; total_bytes: number }
export interface PairingRequest { peer_id: string; name: string; detail: string; fingerprint: string }
export interface Toast { id: number; text: string; tone: "info" | "error" }

class AppStore {
  page = $state<Page>("home");
  info = $state<AppInfo | null>(null);
  devices = $state<DeviceRecord[]>([]);
  connected = $state<PeerLink[]>([]);
  selected = $state<string | null>(null);
  status = $state<Record<string, DeviceStatus>>({});
  media = $state<Record<string, MediaSession[]>>({});
  apps = $state<Record<string, AppEntry[]>>({});
  live = $state<PhoneNotification[]>([]);
  history = $state<NotificationRecord[]>([]);
  transfers = $state<Record<string, LiveTransfer>>({});
  past = $state<TransferRecord[]>([]);
  incoming = $state<Incoming[]>([]);
  pairingRequest = $state<PairingRequest | null>(null);
  pairingOpen = $state(false);
  settings = $state<Settings | null>(null);
  refused = $state<Record<string, boolean>>({});
  toasts = $state<Toast[]>([]);
  updateVersion = $state<string | null>(null);
  private toastSeq = 0;

  get device(): DeviceRecord | null {
    return this.devices.find((d) => d.id === this.selected) ?? null;
  }
  get online(): boolean {
    return !!this.selected && this.connected.some((c) => c.peer_id === this.selected);
  }
  get link(): PeerLink | null {
    return this.connected.find((c) => c.peer_id === this.selected) ?? null;
  }
  has(cap: string): boolean {
    return !!this.device?.capabilities?.includes(cap as never);
  }
  name(id: string): string {
    const d = this.devices.find((x) => x.id === id);
    return d ? d.custom_name || d.name : "Appareil";
  }
  isOnline(id: string): boolean {
    return this.connected.some((c) => c.peer_id === id);
  }

  toast(text: string, tone: Toast["tone"] = "info") {
    const id = ++this.toastSeq;
    this.toasts.push({ id, text, tone });
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), 4200);
  }

  async run<T>(p: Promise<T>, ok?: string): Promise<T | undefined> {
    try {
      const v = await p;
      if (ok) this.toast(ok);
      return v;
    } catch (e) {
      this.toast(String(e), "error");
      return undefined;
    }
  }

  async refreshDevices() {
    const v = await api.devices();
    this.devices = v.devices.filter((d) => d.platform === "android");
    this.connected = v.connected;
    if (!this.selected || !this.devices.some((d) => d.id === this.selected)) {
      this.selected = v.connected[0]?.peer_id ?? this.devices[0]?.id ?? null;
    }
    if (this.selected) this.apps[this.selected] = await api.notificationApps(this.selected);
  }

  async refreshHistory() {
    this.past = await api.transfers(200);
    this.history = await api.notifications(300);
  }

  async init() {
    await listen<CoreEvent>("nc-event", (e) => this.onEvent(e.payload));
    await listen("nc-settings-changed", async () => (this.settings = await api.getSettings()));
    this.info = await api.appInfo();
    this.settings = await api.getSettings();
    const snap = await api.snapshot();
    this.status = snap.status;
    this.media = snap.media;
    this.live = snap.live;
    await this.refreshDevices();
    await this.refreshHistory();
  }

  private onEvent(e: CoreEvent) {
    switch (e.type) {
      case "peer_connected":
        if (!this.connected.some((c) => c.peer_id === e.peer_id)) this.connected.push({ peer_id: e.peer_id, addr: e.addr, link: e.link });
        delete this.refused[e.peer_id];
        this.refreshDevices();
        break;
      case "peer_disconnected":
        this.connected = this.connected.filter((c) => c.peer_id !== e.peer_id);
        break;
      case "peer_refused":
        this.refused[e.peer_id] = true;
        break;
      case "paired":
        this.pairingOpen = false;
        this.pairingRequest = null;
        this.selected = e.device.id;
        this.toast(`${e.device.name} est associé`);
        this.refreshDevices();
        break;
      case "pairing_request":
        this.pairingRequest = {
          peer_id: e.peer_id,
          name: e.device.name,
          detail: [e.device.manufacturer, e.device.model, e.device.os_version && `Android ${e.device.os_version}`].filter(Boolean).join(", "),
          fingerprint: e.fingerprint,
        };
        break;
      case "pairing_failed":
        this.pairingRequest = null;
        this.toast(`Association impossible : ${e.reason}`, "error");
        break;
      case "pairing_window_closed":
        break;
      case "device_removed":
        this.devices = this.devices.filter((d) => d.id !== e.peer_id);
        this.connected = this.connected.filter((c) => c.peer_id !== e.peer_id);
        if (this.selected === e.peer_id) this.selected = this.devices[0]?.id ?? null;
        break;
      case "incompatible_version":
        this.toast(
          e.remote > e.local
            ? "Le téléphone utilise une version plus récente : mettez à jour NOTHING CONNECT sur ce PC."
            : "Le téléphone utilise une ancienne version : mettez à jour l'application Android.",
          "error",
        );
        break;
      case "status":
        this.status[e.peer_id] = e.status;
        break;
      case "notification": {
        this.live = [e.notification, ...this.live.filter((n) => n.key !== e.notification.key)].sort((a, b) => b.posted_at - a.posted_at).slice(0, 200);
        break;
      }
      case "notification_removed":
        this.live = this.live.filter((n) => n.key !== e.key);
        break;
      case "notification_apps":
        this.apps[e.peer_id] = e.apps;
        break;
      case "media_sessions":
        this.media[e.peer_id] = e.sessions;
        break;
      case "share":
        this.toast(e.kind === "link" ? "Lien reçu du téléphone (copié)" : "Texte reçu du téléphone (copié)");
        navigator.clipboard?.writeText(e.content).catch(() => {});
        break;
      case "command_result":
        if (!e.ok) this.toast(`Commande refusée par le téléphone : ${e.error ?? "raison inconnue"}`, "error");
        break;
      case "transfer_incoming":
        this.incoming = [...this.incoming.filter((i) => i.id !== e.id), { id: e.id, peer_id: e.peer_id, files: e.files, total_bytes: e.total_bytes }];
        this.page = "share";
        break;
      case "transfer_progress": {
        const prev = this.transfers[e.id];
        this.transfers[e.id] = {
          id: e.id,
          peer_id: prev?.peer_id ?? "",
          outgoing: e.outgoing,
          state: e.state,
          bytes_done: e.bytes_done,
          total_bytes: e.total_bytes,
          speed_bps: e.speed_bps,
          eta_s: e.eta_s,
          current_file: e.current_file,
          error: null,
          label: e.current_file ?? prev?.label ?? "Transfert",
        };
        if (e.state !== "waiting") this.incoming = this.incoming.filter((i) => i.id !== e.id);
        break;
      }
      case "transfer_finished": {
        const t = this.transfers[e.id];
        if (t) {
          t.state = e.state;
          t.error = e.error;
          if (e.state === "completed") t.bytes_done = t.total_bytes;
        }
        this.incoming = this.incoming.filter((i) => i.id !== e.id);
        if (e.state === "failed") this.toast(`Transfert échoué : ${e.error ?? ""}`, "error");
        this.refreshHistory();
        break;
      }
      case "error":
        this.toast(e.message, "error");
        break;
    }
  }
}

export const app = new AppStore();
