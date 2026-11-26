# 04 — Sécurité et modèle de menace

## Actifs

Clés d'identité des appareils, contenu des fichiers, notifications (messages, codes), commandes
à distance (faire sonner, Ne pas déranger), intégrité des fichiers reçus.

## Hypothèses

- Le réseau local n'est **pas** de confiance : un attaquant peut écouter, injecter, se faire
  passer pour le PC ou le téléphone (Wi-Fi public, invité malveillant).
- Le système d'exploitation de chaque appareil et son coffre (Credential Manager, Keystore)
  sont de confiance.
- Aucun serveur tiers n'intervient.

## Mesures

| Menace | Mesure |
|---|---|
| Écoute du réseau | Toutes les trames après le préambule sont chiffrées (ChaCha20-Poly1305, Noise) |
| Usurpation d'un appareil associé | Noise IK avec clés statiques épinglées ; authentification mutuelle |
| Appareil inconnu | Refus par défaut (aucune donnée échangée) |
| Vol du code d'association par observation réseau | Le code n'est jamais transmis (SPAKE2) |
| Attaque hors ligne sur le code | Impossible avec SPAKE2 ; essais en ligne limités à 3, validité 120 s |
| Appareil tiers qui saisit le code | Confirmation explicite sur le PC (nom, modèle, empreinte) |
| QR code intercepté | Ne contient que l'adresse, un code temporaire et l'empreinte publique du PC |
| Révocation | Clé supprimée immédiatement ; le pair est prévenu (`unpaired`) ; reconnexion refusée |
| Entrées malformées | Tailles bornées, JSON typé, chemins assainis, blocs hors séquence refusés |
| Fichier corrompu ou altéré | SHA-256 de bout en bout ; fichier rejeté et supprimé en cas d'écart |
| Fuite par les journaux | Ni clés, ni contenus de fichiers ou de notifications dans les journaux |
| Contenu sensible affiché sur le PC | Notifications marquées privées masquées par défaut, non conservées |
| Mise à jour piégée | Windows : signature minisign vérifiée ; Android : SHA-256 + même clé de signature |
| Exposition Internet | Écoute réseau local uniquement ; aucune télémétrie |

## Stockage des secrets

- Windows : clé privée X25519 dans le Gestionnaire d'identification (DPAPI, utilisateur courant).
  Supprimée par le désinstalleur si « Supprimer les données » est coché.
- Android : chiffrée AES-256-GCM par une clé non exportable de l'Android Keystore.
  Sauvegarde cloud désactivée.
- SQLite ne contient que des clés **publiques**.

## Limites connues

- Pas de rotation automatique des clés d'identité : « Effacer toutes les données » régénère
  l'identité (il faut alors réassocier).
- Une notification affichée en clair sur le PC est visible par toute personne devant l'écran.
- Un PC compromis (logiciel malveillant local) peut lire ce que l'application affiche.
