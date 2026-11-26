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
   - `0x02` + transfert (16) + index fichier (u32) + offset (u64) + données : bloc de fichier.

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

## Compatibilité

- Ajouter un champ optionnel ou une valeur `Capability` : même version (les inconnus sont ignorés).
- Supprimer, renommer, changer un type : incrémenter `PROTOCOL_VERSION` et documenter ici.
- `MIN_PROTOCOL_VERSION` fixe la plus ancienne version encore acceptée ; en dehors, l'interface
  indique quel appareil mettre à jour.

## Limites

Trame ≤ 65 519 octets en clair ; JSON de contrôle ≤ 65 518 octets ; partage texte ≤ 1 Mo ;
100 000 fichiers par transfert ; chemins relatifs assainis (pas de `..`, pas de lecteur,
noms réservés Windows préfixés).
