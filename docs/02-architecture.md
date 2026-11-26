# 02 — Architecture

## Vue d'ensemble

```text
┌──────────── Windows ────────────┐            ┌──────────── Android ────────────┐
│ Interface Svelte (TypeScript)   │            │ Interface Jetpack Compose       │
│        ▲ invoke / événements    │            │ Services Kotlin : connexion,    │
│ Tauri 2 : fenêtre, zone de      │            │ notifications, multimédia,      │
│ notification, toasts, mises à   │            │ état, commandes, partage        │
│ jour, démarrage automatique     │            │        ▲ UniFFI (JNA)           │
│        ▲ API Rust               │            │        │                        │
│   nc-core (Rust) ◄──── TCP + Noise (Wi-Fi local) ────► nc-core (Rust)        │
└─────────────────────────────────┘            └─────────────────────────────────┘
```

Le **cœur Rust** (`crates/`) contient toute la logique métier ; Tauri et Kotlin ne font que
l'intégration système. Le PC **écoute** (port 47820 par défaut, annoncé en mDNS), le
téléphone **se connecte** et se reconnecte seul.

## Crates

| Crate | Rôle | Dépend de |
|---|---|---|
| `nc-protocol` | Messages JSON versionnés, trames binaires, limites anti-abus | — |
| `nc-security` | Identité X25519, empreintes, coffre de secrets (Windows Credential Manager) | snow |
| `nc-pairing` | Code à 6 chiffres, fenêtre d'association, SPAKE2, QR | spake2 |
| `nc-transport` | Handshakes Noise IK / XXpsk3, canal chiffré, découverte mDNS | tokio, snow, mdns-sd |
| `nc-transfer` | Plan d'envoi, réception, reprise, SHA-256, conflits, chemins sûrs | sha2 |
| `nc-storage` | SQLite avec migrations versionnées | rusqlite |
| `nc-core` | `Node` : sessions, association, transferts, événements, réglages | tous |
| `nc-ffi` | Interface UniFFI pour Kotlin | nc-core |

Écart assumé avec l'arborescence indicative du cahier des charges : `device`, `sync`, `media`
et `streaming` ne sont pas des crates séparées. L'état des appareils et le multimédia sont
des messages du protocole relayés par `nc-core` ; le streaming n'est pas commencé. Une crate
sera créée quand une logique propre le justifiera.

## Flux d'un transfert

1. L'émetteur parcourt fichiers/dossiers (`plan`) et envoie `TransferOffer` (chemins relatifs, tailles).
2. Le récepteur valide l'offre (chemins assainis, limites), puis accepte (manuel ou automatique
   pour un appareil de confiance) en envoyant les **offsets déjà reçus** (reprise).
3. Un thread dédié lit le disque et pousse des blocs de 64 Ko dans une file bornée ; la tâche
   d'écriture réseau sert toujours les messages de contrôle en priorité.
4. Côté réception, un thread dédié écrit dans `.nc-partial/` et calcule le SHA-256 au fil de l'eau.
5. `TransferFileEnd` transporte l'empreinte de l'émetteur ; le fichier n'est déplacé à sa
   place finale qu'après concordance.
6. Coupure : l'état passe à « interrompu » ; à la reconnexion, l'émetteur renvoie la même offre
   (même identifiant) et le transfert reprend à l'octet près.

Débit mesuré en boucle locale (chiffrement + SHA-256 des deux côtés + disque) : ≈ 300 Mo/s
sur 2 cœurs. Le Wi-Fi est donc toujours le facteur limitant.

## Arrière-plan

- **Windows** : processus utilisateur (pas de service, pas de droits administrateur), icône
  dans la zone de notification ; fermer la fenêtre la masque ; « Quitter complètement » ferme
  proprement les sessions (`Bye`).
- **Android** : service de premier plan `connectedDevice` avec notification discrète,
  `MulticastLock` pour mDNS, reprise au démarrage du téléphone. Android peut malgré tout
  limiter l'application (économie d'énergie, surcouches) : l'utilisateur est guidé pour la
  retirer de l'optimisation batterie.

## Mises à jour

| | Windows | Android |
|---|---|---|
| Source | GitHub Releases, `latest.json` | GitHub Releases, API `releases/latest` |
| Vérification | Signature minisign (clé publique dans `tauri.conf.json`) | SHA-256 publié + signature APK (même clé) |
| Installation | NSIS par-dessus, mode passif | Installateur Android par-dessus |
| Données | `%APPDATA%`, conservées | conservées (même signature) |
| Schéma de base | migrations numérotées ; base plus récente refusée | idem |
| Protocole | `Hello` vérifie la version ; message clair si incompatible | idem |
