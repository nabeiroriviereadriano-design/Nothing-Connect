# 05 — Permissions Android

| Permission | Pourquoi | Quand |
|---|---|---|
| `INTERNET`, `ACCESS_NETWORK_STATE`, `ACCESS_WIFI_STATE` | Connexion au PC sur le Wi-Fi ; vérifier les mises à jour | Automatique |
| `CHANGE_WIFI_MULTICAST_STATE` | Trouver le PC (mDNS) | Automatique |
| `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_CONNECTED_DEVICE` | Rester connecté en arrière-plan | Automatique |
| `POST_NOTIFICATIONS` | Notification du service, transferts, fichiers à accepter | Demandée au premier lancement (Android 13+) |
| `RECEIVE_BOOT_COMPLETED` | Se reconnecter après redémarrage | Automatique |
| `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` | Proposer d'exclure l'appli de l'optimisation batterie | Bouton dans l'écran Accueil |
| Accès aux notifications (réglage spécial) | Transmettre les notifications, contrôler la musique | Bouton « Autoriser l'accès » |
| Accès Ne pas déranger (réglage spécial) | Activer NPD depuis le PC | Facultatif |
| `VIBRATE` | Commande « Vibrer » | Automatique |
| `CAMERA` | Scanner le QR code d'association | Demandée au moment du scan |
| `REQUEST_INSTALL_PACKAGES` | Installer une mise à jour téléchargée | Android demande l'accord à la première mise à jour |

Non demandées : localisation (le nom du Wi-Fi n'est donc pas affiché), contacts, SMS,
`QUERY_ALL_PACKAGES` (remplacée par une requête limitée aux applications lançables),
stockage (les fichiers reçus sont publiés dans Téléchargements via MediaStore).

## Différences constructeurs

Certaines surcouches (Xiaomi, Huawei, Samsung…) ferment les services en arrière-plan malgré le
service de premier plan. Si la connexion tombe quand l'écran est éteint : désactiver
l'optimisation batterie pour NOTHING CONNECT et, selon la marque, autoriser le « démarrage
automatique ». Les constats par modèle seront ajoutés ici après tests sur appareils réels.
