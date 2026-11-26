# 01 — Étude de faisabilité (périmètre risque faible/moyen)

Date : 26/11/2026. Ce document fixe les décisions qui conditionnent le reste. Les sujets à risque
élevé (USB, BLE, mirroring, pilote IddCx, audio) sont listés en fin de document avec leur statut.

## 1. Versions ciblées

| Système | Minimum | Recommandé | Justification |
|---|---|---|---|
| Windows | 10 22H2 (x64) | 11 23H2+ | WebView2 disponible, toasts natifs, IddCx 1.4+ sur 10 récents |
| Android | 10 (API 29) | 13+ (API 33+) | API 29 : `AudioPlaybackCapture`, stockage scoped ; API 33 : permission `POST_NOTIFICATIONS` explicite |
| targetSdk Android | 35 | — | Exigence Google Play 2025-2026 |

## 2. API réellement disponibles (sans root, sans contournement)

| Besoin | Android | Windows | Faisable |
|---|---|---|---|
| Notifications | `NotificationListenerService` (activation manuelle par l'utilisateur) | Toasts via plugin Tauri | Oui |
| Actions sur notif | `Notification.Action` + `RemoteInput` (réponse) quand l'appli les expose | — | Partiel, par notification |
| Sessions multimédias | `MediaSessionManager.getActiveSessions()` (exige l'accès notifications) | — | Oui, si l'appli publie une MediaSession |
| Batterie / stockage / réseau | `BatteryManager`, `StatFs`, `ConnectivityManager` | — | Oui |
| Température batterie | `EXTRA_TEMPERATURE` de l'intent batterie | — | Oui |
| Faire sonner / vibrer | `RingtoneManager`, `Vibrator` | — | Oui |
| Volume multimédia | `AudioManager.setStreamVolume(STREAM_MUSIC)` | — | Oui |
| Ne pas déranger | `NotificationManager.setInterruptionFilter` (accès DND manuel) | — | Oui après autorisation |
| Arrière-plan | Foreground Service type `connectedDevice` (API 34 : permission dédiée) | Processus utilisateur + zone de notification | Oui, **sans garantie** côté Android (constructeurs) |
| Presse-papiers | Lecture interdite en arrière-plan depuis Android 10 | Libre | Manuel uniquement côté Android → hors de cette passe |
| Découverte LAN | `NsdManager` (mDNS) | `mdns-sd` (Rust) | Oui ; repli IP manuelle / QR |

## 3. Choix d'intégration

| Sujet | Options | Décision | Raison |
|---|---|---|---|
| UI Windows | React, Svelte, Solid, TS natif | **Svelte 5 + Vite** | Bundle léger, peu de code, réactivité simple |
| Rust ↔ Kotlin | JNI manuel, UniFFI | **UniFFI** | Bindings Kotlin générés, callbacks typés, moins d'erreurs mémoire |
| Rust ↔ Tauri | Commandes Tauri | **Commandes + événements Tauri** | Natif Tauri 2 |
| Sérialisation | JSON, CBOR, Protobuf | **JSON (contrôle) + trames binaires (données)** | Débogable ; les chunks de fichiers ne passent jamais par JSON |
| Chiffrement canal | TLS auto-signé, Noise | **Noise `KK_25519_ChaChaPoly_BLAKE2s` (`snow`)** | Authentification mutuelle par clés statiques épinglées, pas de PKI |
| Association | Code seul, PAKE | **SPAKE2 (6 chiffres) + confirmation** | Code court sûr : résiste aux attaques hors ligne |
| Stockage | SQLite, fichiers JSON | **SQLite (`rusqlite` bundled)** | Transactions, historique, même moteur Windows/Android |
| Secrets | En base | **Coffre OS** : Windows Credential Manager (DPAPI), Android Keystore | Jamais de clé privée en clair dans SQLite |

## 4. Dépendances principales et licences

Toutes compatibles avec une distribution gratuite (MIT / Apache-2.0 / BSD) :
`tokio`, `serde`, `serde_json`, `snow`, `spake2`, `sha2`, `hkdf`, `chacha20poly1305`, `rand`,
`rusqlite` (SQLite = domaine public), `mdns-sd`, `thiserror`, `tracing`, `uniffi`, `keyring`,
`tauri` + plugins officiels, `svelte`, `vite`, AndroidX/Jetpack Compose (Apache-2.0),
`qrcode` (MIT/Apache), ZXing (Apache-2.0) pour le scan côté Android.

## 5. Sujets à risque élevé — non traités dans cette passe

| Sujet | Statut | Blocage principal |
|---|---|---|
| USB | Non commencé | Sans ADB : Android Open Accessory + pilote WinUSB ; dépend du téléphone |
| BLE découverte | Non commencé | API Windows BLE à valider ; le LAN + QR couvre l'association |
| Screen mirroring | Non commencé | Injection d'entrées sans root limitée (Accessibilité ou ADB) |
| Écran étendu IddCx | Non commencé | Signature pilote (test-signing en dev ; signature Microsoft pour distribuer) |
| Audio | Non commencé | `AudioPlaybackCapture` refusable par les applis |
| Presse-papiers auto | Non commencé | Android bloque la lecture en arrière-plan |
