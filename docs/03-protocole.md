# 03 — Protocole (version 1)

## Couches

1. **Préambule en clair** (8 octets, initiateur → répondeur) : `NCN1`, version (u16 BE), mode
   (0 = appareil associé, 1 = association), réservé.
2. **Handshake** : messages préfixés par leur longueur (u16 BE).
   - mode 0 : Noise `IK_25519_ChaChaPoly_BLAKE2s`, clé du PC épinglée. Un appareil inconnu
     reçoit un message vide (refus) et la connexion est fermée.
   - mode 1 : SPAKE2 (Ed25519) sur le code à 6 chiffres → HKDF-SHA256 → psk ;
     Noise `XXpsk3_25519_ChaChaPoly_BLAKE2s`, puis 1 octet de confirmation chiffré.
3. **Trames chiffrées** : `u16 longueur` + message Noise (≤ 65 535 octets). Texte clair :
   - `0x01` + JSON : message de contrôle (`{"t": "...", ...}`) ;
   - `0x02` + transfert (16) + index fichier (u32) + offset (u64) + données : bloc de fichier ;
   - `0x03` + flux (u8) + drapeaux (u8) + horodatage µs (u64 BE) + données : fragment temps réel.
     Flux : 1 miroir (H.264 Annex B), 2 son du téléphone, 3 son du PC (PCM s16le 48 kHz stéréo,
     blocs de 10 ms), 4 écran étendu (JPEG). Drapeaux : 1 configuration (SPS/PPS), 2 image clé,
     4 début d'unité, 8 fin d'unité. Une unité (image) peut tenir sur plusieurs trames ; elle est
     réassemblée (16 Mo max) puis livrée.

L'écrivain de session sert d'abord le contrôle, puis les flux, puis les fichiers. Une file de flux
pleine fait **abandonner l'unité entière** (jamais de retard accumulé) ; le récepteur demande alors
une image clé.

Prologue Noise : `nothing-connect/1`.

## Messages de contrôle

| `t` | Sens | Rôle |
|---|---|---|
| `hello` | ↔ | Premier message : version de protocole, `DeviceInfo` (nom, modèle, version, capacités) |
| `pairing_accepted` / `pairing_rejected` | PC → tél. | Confirmation de l'utilisateur |
| `ping` / `pong` | ↔ | Maintien (15 s) ; session fermée après 45 s sans trame |
| `bye` | ↔ | Fermeture propre |
| `unpaired` | ↔ | Révocation : le destinataire supprime aussi l'appareil |
| `status` | tél. → PC | Batterie, température, stockage, réseau, volume, NPD |
| `command` / `command_result` | PC → tél. / retour | `ring`, `stop_ring`, `vibrate`, `set_media_volume`, `set_do_not_disturb`, `refresh_status` |
| `notification_posted` / `notification_removed` | tél. → PC | Notification (titre, texte, icône, actions, sensible) |
| `notification_action` / `_dismiss` / `_open` | PC → tél. | Action (réponse rapide), ignorer, ouvrir |
| `notification_apps` / `notification_filter` | ↔ | Liste des applis, filtre appliqué sur le téléphone |
| `media_sessions` / `media_command` | ↔ | Lectures en cours ; lecture, pause, suivant, précédent |
| `share` | ↔ | Texte ou lien |
| `transfer_offer` / `_accept` / `_reject` | ↔ | Offre ; acceptation avec offsets de reprise |
| `transfer_file_end` / `_file_verified` | ↔ | SHA-256 de l'émetteur ; résultat de la vérification |
| `transfer_pause` / `_resume` / `_cancel` | ↔ | Contrôle ; `resume` avec offsets vides = demande d'offsets |
| `capabilities` | ↔ | Capacités mises à jour en cours de session (ex. contrôle activé dans Accessibilité) |
| `clipboard` | ↔ | Texte du presse-papiers (≤ 1 Mo) |
| `stream_request` | ↔ | Demande d'un flux (`kind` : `mirror`, `phone_audio`, `pc_audio`, `display`) avec `params` (codec, taille max, débit, i/s, son, orientation) |
| `stream_start` | ↔ | L'émetteur annonce le format réel (définition, fréquence d'échantillonnage…) ; pour `display`, c'est le téléphone qui donne la taille de son écran |
| `stream_stop` | ↔ | Fin d'un flux, avec raison éventuelle (refus, arrêt sur le téléphone) |
| `stream_keyframe` | ↔ | Demande d'image clé (décodeur qui démarre, image perdue) |
| `input` | PC → tél. | Contrôle du miroir : `tap`, `long_press`, `swipe`, `key` (`back`, `home`, `recents`, `notifications`), `text` ; coordonnées 0..1 |
| `display_pointer` | tél. → PC | Toucher sur l'écran étendu : `down`/`move`/`up`/`cancel`, identifiant, coordonnées 0..1 |
| `display_pause` | tél. → PC | `{"paused":true}` quand l'écran étendu passe en arrière-plan sur le téléphone, `false` au retour (le PC suspend l'encodage) ; envoyé seulement si le PC annonce `display_pause` |
| `display_input` | tél. → PC | Écran étendu : `{"a":"key","key":…}` (`back`, `task_view`, `show_desktop`, `next_app`, `prev_app`, `next_desktop`, `prev_desktop`, `touch_keyboard`, `backspace`, `enter`…) ou `{"a":"text","text":…}` ; envoyé seulement si le PC annonce `display_touch` |

## Compatibilité

- Ajouter un champ optionnel, une valeur `Capability` ou un **nouveau type de message** : même version. Depuis 0.4, un type de message inconnu est ignoré à la réception (avant, la connexion était coupée : un nouveau message n'est donc envoyé qu'à un pair qui annonce la capacité correspondante).
- Supprimer, renommer, changer un type : incrémenter `PROTOCOL_VERSION` et documenter ici.
- `MIN_PROTOCOL_VERSION` fixe la plus ancienne version encore acceptée ; en dehors, l'interface
  indique quel appareil mettre à jour.

## Limites

Trame ≤ 65 519 octets en clair ; JSON de contrôle ≤ 65 518 octets ; partage texte ≤ 1 Mo ;
100 000 fichiers par transfert ; chemins relatifs assainis (pas de `..`, pas de lecteur,
noms réservés Windows préfixés).
