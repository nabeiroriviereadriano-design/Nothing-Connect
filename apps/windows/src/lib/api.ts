import { invoke, type Channel } from "@tauri-apps/api/core";

// ---- Types partagés avec le cœur Rust (sérialisation serde) ----

export type Capability =
  | "file_transfer" | "share_text" | "device_status" | "notifications" | "notification_actions"
  | "media_control" | "ring" | "vibrate" | "media_volume" | "do_not_disturb"
  | "clipboard" | "screen_mirror" | "mirror_control" | "phone_audio" | "audio_playback" | "extended_display" | "unknown";

export type StreamKind = "mirror" | "phone_audio" | "pc_audio" | "display";
export interface StreamParams {
  codec?: string | null; width?: number | null; height?: number | null; max_size?: number | null;
  fps?: number | null; bitrate?: number | null; sample_rate?: number | null; channels?: number | null;
  with_audio?: boolean | null; dpi?: number | null; refresh_hz?: number | null;
}
export type InputEvent =
  | { i: "tap"; x: number; y: number }
  | { i: "long_press"; x: number; y: number }
  | { i: "swipe"; x1: number; y1: number; x2: number; y2: number; duration_ms: number }
  | { i: "key"; key: "back" | "home" | "recents" | "notifications" }
  | { i: "text"; text: string };
export type StreamMessage =
  | { t: "stream_start"; kind: StreamKind; params: StreamParams }
  | { t: "stream_stop"; kind: StreamKind; reason: string | null }
  | { t: "stream_request"; kind: StreamKind; params: StreamParams };
export interface UsbStatus { adb: string | null; devices: [string, string][]; tunneled: string[] }

export interface DeviceRecord {
  id: string;
  name: string;
  custom_name: string | null;
  platform: "windows" | "android";
  manufacturer: string | null;
  model: string | null;
  os_version: string | null;
  app_version: string | null;
  paired_at: number;
  last_seen: number | null;
  last_addr: string | null;
  auto_accept: boolean;
  capabilities: Capability[];
}

export interface PeerLink { peer_id: string; addr: string; link: string }

export interface DeviceStatus {
  battery_percent: number | null;
  charging: boolean | null;
  battery_temp_c: number | null;
  storage_free_bytes: number | null;
  storage_total_bytes: number | null;
  wifi_connected: boolean | null;
  wifi_ssid: string | null;
  bluetooth_enabled: boolean | null;
  mobile_network: string | null;
  media_volume_percent: number | null;
  dnd_enabled: boolean | null;
  display_max_hz?: number | null;
}

export interface NotificationAction { index: number; title: string; accepts_text: boolean }
export interface PhoneNotification {
  key: string;
  package: string;
  app_name: string;
  title: string | null;
  text: string | null;
  posted_at: number;
  icon_png_b64: string | null;
  actions: NotificationAction[];
  sensitive: boolean;
}
export interface NotificationRecord {
  id: number; device_id: string; key: string; package: string; app_name: string;
  title: string | null; body: string | null; posted_at: number; removed: boolean;
}
export interface AppEntry { package: string; app_name: string; enabled: boolean }

export type PlaybackState = "playing" | "paused" | "stopped" | "buffering" | "other";
export interface MediaSession {
  id: string; package: string; app_name: string;
  title: string | null; artist: string | null; album: string | null;
  duration_ms: number | null; position_ms: number | null; state: PlaybackState;
  artwork_jpeg_b64: string | null; can_skip_next: boolean; can_skip_prev: boolean;
}
export type MediaAction = "play" | "pause" | "toggle_play_pause" | "next" | "previous";

export type TransferState =
  | "offered" | "waiting" | "running" | "paused" | "interrupted" | "completed" | "failed" | "cancelled" | "rejected";

export interface TransferFileRecord {
  idx: number; rel_path: string; size: number; source_path: string | null;
  final_path: string | null; sha256: string | null; state: string;
}
export interface TransferRecord {
  id: string; device_id: string; outgoing: boolean; created_at: number; finished_at: number | null;
  state: string; total_bytes: number; dest_dir: string | null; files: TransferFileRecord[];
}

export type RemoteCommand =
  | { c: "ring" } | { c: "stop_ring" } | { c: "vibrate"; millis: number }
  | { c: "set_media_volume"; percent: number } | { c: "set_do_not_disturb"; enabled: boolean }
  | { c: "refresh_status" };

export interface Settings {
  download_dir: string | null;
  conflict_policy: "keep_both" | "replace" | "skip";
  notifications_enabled: boolean;
  notification_popups: boolean;
  popup_duration_s: number;
  hide_sensitive_content: boolean;
  store_notification_content: boolean;
  notification_history_limit: number;
  suspended: boolean;
  clipboard_sync: boolean;
  usb_adb: boolean;
  ble_advertise: boolean;
}

export interface AppInfo {
  version: string; protocol: number; fingerprint: string; port: number | null;
  lan_ip: string | null; data_dir: string; logs_dir: string;
}
export interface PairingView {
  code: string; expires_in_s: number; qr_svg: string | null; fingerprint: string; address: string | null;
}

export type CoreEvent =
  | { type: "started"; fingerprint: string; port: number | null }
  | { type: "pairing_window"; code: string; expires_in_s: number; qr_uri: string | null }
  | { type: "pairing_window_closed" }
  | { type: "pairing_request"; peer_id: string; device: { name: string; manufacturer: string | null; model: string | null; os_version: string | null }; fingerprint: string }
  | { type: "paired"; device: DeviceRecord }
  | { type: "pairing_failed"; reason: string }
  | { type: "peer_connected"; peer_id: string; addr: string; link: string }
  | { type: "peer_disconnected"; peer_id: string; reason: string }
  | { type: "peer_refused"; peer_id: string }
  | { type: "device_removed"; peer_id: string }
  | { type: "incompatible_version"; peer_id: string | null; remote: number; local: number }
  | { type: "status"; peer_id: string; status: DeviceStatus }
  | { type: "notification"; peer_id: string; notification: PhoneNotification }
  | { type: "notification_removed"; peer_id: string; key: string }
  | { type: "notification_apps"; peer_id: string; apps: AppEntry[] }
  | { type: "media_sessions"; peer_id: string; sessions: MediaSession[] }
  | { type: "share"; peer_id: string; kind: "text" | "link"; content: string }
  | { type: "clipboard"; peer_id: string; content: string }
  | { type: "device_updated"; device: DeviceRecord }
  | { type: "command_result"; peer_id: string; id: number; ok: boolean; error: string | null }
  | { type: "request"; peer_id: string; message: StreamMessage | { t: string } }
  | { type: "transfer_incoming"; id: string; peer_id: string; files: { rel_path: string; size: number }[]; total_bytes: number }
  | { type: "transfer_progress"; id: string; outgoing: boolean; state: TransferState; bytes_done: number; total_bytes: number; speed_bps: number; eta_s: number | null; current_file: string | null }
  | { type: "transfer_file_done"; id: string; index: number; path: string | null; outcome: string }
  | { type: "transfer_finished"; id: string; outgoing: boolean; state: TransferState; error: string | null }
  | { type: "error"; message: string };

// ---- Appels ----

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  devices: () => invoke<{ devices: DeviceRecord[]; connected: PeerLink[] }>("devices"),
  openPairing: () => invoke<PairingView>("open_pairing"),
  closePairing: () => invoke<void>("close_pairing"),
  confirmPairing: (peerId: string, accept: boolean) => invoke<void>("confirm_pairing", { peerId, accept }),
  renameDevice: (peerId: string, name: string | null) => invoke<void>("rename_device", { peerId, name }),
  setAutoAccept: (peerId: string, on: boolean) => invoke<void>("set_auto_accept", { peerId, on }),
  removeDevice: (peerId: string) => invoke<void>("remove_device", { peerId }),
  sendPaths: (peerId: string, paths: string[]) => invoke<string>("send_paths", { peerId, paths }),
  sendText: (peerId: string, content: string) => invoke<void>("send_text", { peerId, content }),
  acceptTransfer: (id: string, accept: boolean) => invoke<void>("accept_transfer", { id, accept }),
  pauseTransfer: (id: string) => invoke<void>("pause_transfer", { id }),
  resumeTransfer: (id: string) => invoke<void>("resume_transfer", { id }),
  cancelTransfer: (id: string) => invoke<void>("cancel_transfer", { id }),
  transfers: (limit = 200) => invoke<TransferRecord[]>("transfers", { limit }),
  clearTransferHistory: () => invoke<number>("clear_transfer_history"),
  notifications: (limit = 300) => invoke<NotificationRecord[]>("notifications", { limit }),
  clearNotifications: () => invoke<number>("clear_notifications"),
  notificationApps: (peerId: string) => invoke<AppEntry[]>("notification_apps", { peerId }),
  setNotificationApp: (peerId: string, pkg: string, enabled: boolean) =>
    invoke<void>("set_notification_app", { peerId, package: pkg, enabled }),
  notificationAction: (peerId: string, key: string, actionIndex: number, reply: string | null) =>
    invoke<void>("notification_action", { peerId, key, actionIndex, reply }),
  notificationDismiss: (peerId: string, key: string) => invoke<void>("notification_dismiss", { peerId, key }),
  notificationOpen: (peerId: string, key: string) => invoke<void>("notification_open", { peerId, key }),
  command: (peerId: string, command: RemoteCommand) => invoke<number>("remote_command", { peerId, command }),
  media: (peerId: string, sessionId: string, action: MediaAction) => invoke<void>("media_command", { peerId, sessionId, action }),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  snapshot: () => invoke<{ status: Record<string, DeviceStatus>; media: Record<string, MediaSession[]>; live: PhoneNotification[] }>("snapshot"),
  defaultDownloadDir: () => invoke<string>("default_download_dir"),
  wipeAll: () => invoke<void>("wipe_all"),
  mirrorSubscribe: (channel: Channel<ArrayBuffer>) => invoke<void>("mirror_subscribe", { channel }),
  mirrorStart: (peerId: string, maxSize: number, bitrate: number, withAudio: boolean) =>
    invoke<void>("mirror_start", { peerId, maxSize, bitrate, withAudio }),
  mirrorStop: (peerId: string) => invoke<void>("mirror_stop", { peerId }),
  mirrorKeyframe: (peerId: string) => invoke<void>("mirror_keyframe", { peerId }),
  mirrorInput: (peerId: string, event: InputEvent) => invoke<void>("mirror_input", { peerId, event }),
  phoneAudioStart: (peerId: string) => invoke<void>("phone_audio_start", { peerId }),
  phoneAudioStop: (peerId: string) => invoke<void>("phone_audio_stop", { peerId }),
  pcAudioStart: (peerId: string) => invoke<void>("pc_audio_start", { peerId }),
  pcAudioStop: () => invoke<void>("pc_audio_stop"),
  pcAudioActive: () => invoke<boolean>("pc_audio_active"),
  usbStatus: () => invoke<UsbStatus | null>("usb_status"),
  androidInstall: (serial: string | null) => invoke<string>("android_install", { serial }),
  displayStatus: () => invoke<DisplayStatus>("display_status"),
  displaySetup: (action: "install" | "uninstall" | "enable-testsigning" | "install-vdd") => invoke<string>("display_setup", { action }),
  displayStart: (peerId: string, scale: number, quality: number, fps: number, portrait: boolean, touch: boolean, gpu: boolean) =>
    invoke<void>("display_start", { peerId, scale, quality, fps, portrait, touch, gpu }),
  displayStop: () => invoke<void>("display_stop"),
  displayTune: (scale: number, quality: number, fps: number, touch: boolean, gpu: boolean) =>
    invoke<void>("display_tune", { scale, quality, fps, touch, gpu }),
  uiPrefsGet: () => invoke<Record<string, unknown>>("ui_prefs_get"),
  firewallStatus: () => invoke<FirewallStatus>("firewall_status"),
  firewallAllow: () => invoke<void>("firewall_allow"),
  usbToolsInstall: () => invoke<string>("usb_tools_install"),
  uiPrefsSet: (prefs: object) => invoke<void>("ui_prefs_set", { prefs }),
};

export interface FirewallStatus {
  supported: boolean; public_network: boolean; allowed_public: boolean; blocked: boolean; needs_fix: boolean;
}

export interface DisplayStatus {
  supported: boolean; installed: boolean; testsigning: boolean; active_peer: string | null;
  width: number; height: number; setup_available: boolean;
  vdd_installed: boolean; backend: "nothing-connect" | "vdd" | null;
  codec: "h264" | "jpeg" | null; fps: number; kbps: number; refresh_hz: number; encoder: string | null; paused: boolean;
}
