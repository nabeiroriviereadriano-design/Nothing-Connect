# Matrice des fonctionnalités — état réel

Légende : **Testé** = tests automatisés reproductibles qui passent en CI ; **Implémenté** = code
écrit et compilé, validation sur appareils réels à faire ; **Non commencé** = rien n'est
annoncé dans l'interface.

Dernière mise à jour : version 0.5.0. Les fonctions temps réel (miroir, son, écran étendu)
compilent en CI mais n'ont pas encore été validées sur un PC et un téléphone réels : elles sont
marquées « Implémenté » tant que ce n'est pas fait.

| § | Fonction | État | Preuve / limite |
|---|---|---|---|
| 5.1 | Association par code | **Testé** | `e2e::pairing_with_code_and_confirmation`, `wrong_code_is_refused`, `pairing_rejected_by_user` |
| 5.2 | Association par QR | **Testé** (cœur) / Implémenté (scan Android) | QR sans secret permanent ; scan ZXing à valider sur appareil |
| 5.3 | Plusieurs appareils, renommage, révocation, inconnus refusés | **Testé** | `reconnects_after_restart_and_revocation_is_immediate`, `revoked_phone_is_refused`, `unknown_peer_refused_by_default` |
| 4.2 | Wi-Fi local, découverte mDNS, reconnexion | **Testé** (reconnexion) / Implémenté (mDNS) | mDNS à valider sur réseaux réels (routeurs avec isolation) |
| 4.1 | USB-C | Implémenté | Par ADB (débogage USB activé) : tunnel `adb reverse`, le téléphone passe sur le câble tout seul ; branchement détecté par notification Windows (`CM_Register_Notification`) et par le téléphone (`USB_STATE`) ; outils Google téléchargeables en un clic ; installation de l'APK par câble depuis le PC. Pas d'AOA (exigerait un pilote WinUSB) |
| 4.3 | Bluetooth / BLE | Implémenté | Le PC s'annonce en BLE (empreinte + adresse) ; le téléphone (Android 12+) le trouve quand mDNS est bloqué. Les données passent toujours par le Wi-Fi ou l'USB (débit BLE insuffisant) |
| 4.4 | Priorité et basculement USB/Wi-Fi | Implémenté | USB essayé en premier ; bascule vers l'USB dès qu'il répond, y compris pendant l'écran étendu ou le son (transferts de fichiers exceptés) ; délai de 4 s pour qu'une liaison remplace l'autre sans couper les flux ; repli Wi-Fi à la débranche |
| 6.1–6.3 | Transferts bidirectionnels, dossiers, gros fichiers, pause/reprise/annulation, SHA-256, conflits, reprise après coupure | **Testé** | `files_and_folders_both_directions_with_sha256`, `pause_and_resume_keeps_integrity`, `transfer_resumes_after_connection_loss`, `reject_and_cancel`, tests de `nc-transfer` ; débit ≈ 300 Mo/s en boucle locale |
| 6.4 | Historique | **Testé** (cœur) / Implémenté (UI) | « Afficher » ouvre l'Explorateur sur le fichier |
| 7 | Partage rapide : glisser-déposer, zone de notification, « Envoyer vers », menu Partager Android, textes et liens | Implémenté | Raccourci clavier global : non commencé ; menu contextuel Explorateur : via « Envoyer vers » |
| 8 | Miroir de l'écran + contrôle | Implémenté | H.264 matériel (MediaCodec) → WebCodecs ; accord Android à chaque diffusion ; contrôle (toucher, balayer, retour, texte) par service d'accessibilité activé par l'utilisateur ; images perdues → image clé demandée. Test de bout en bout du transport : `realtime_streams_are_reassembled_in_order`, `congested_stream_drops_units_instead_of_queueing` |
| 9 | Écran secondaire étendu | Implémenté | Pilote IddCx intégré (UMDF, certificat local, sans mode test) ou Virtual Display Driver (signé, MIT) ; vidéo H.264 encodée par la carte graphique (Media Foundation) ou par OpenH264 (testé : `h264enc::tests`), capture et encodage en parallèle, décodée par MediaCodec, repli JPEG pour les versions 0.2 ; fréquence max du téléphone ; tactile multipoint et gestes ; pause quand le téléphone quitte l'écran étendu. Voir [pilote-ecran.md](pilote-ecran.md) |
| 10 | Notifications : réception, popups, historique, filtre par appli (appliqué sur le téléphone), masquage du sensible | **Testé** (cœur) / Implémenté (Android, Windows) | `messages_commands_and_notifications` |
| 10.2 | Réponse rapide, ouvrir, ignorer | Implémenté | Dépend de chaque application ; ouverture soumise aux règles Android 14+ |
| 11 | Centre de contrôle : batterie, température, stockage, Wi-Fi, Bluetooth, réseau mobile, sonner, vibrer, volume, NPD | Implémenté | Commandes testées de bout en bout dans le cœur ; nom du Wi-Fi non affiché (exigerait la localisation) |
| 12.1 | Contrôle multimédia | Implémenté | Uniquement les applis publiant une MediaSession |
| 12.2–12.3 | Son dans les deux sens | Implémenté | Téléphone → PC : AudioPlaybackCapture (Android 10+, applis qui l'autorisent) ; PC → téléphone : capture WASAPI loopback. PCM 48 kHz stéréo, tampon anti-gigue, latence plafonnée |
| 13 | Presse-papiers | **Testé** (cœur) / Implémenté | `clipboard_is_delivered`. PC → téléphone automatique (réglage) ; téléphone → PC d'un toucher (tuile des réglages rapides, bouton) car Android interdit la lecture en arrière-plan |
| 14.1 | Arrière-plan Windows : zone de notification, fermer sans quitter, quitter complètement, démarrage automatique | Implémenté | Validé à l'écran sous Linux (Xvfb) ; à valider sous Windows |
| 14.2 | Arrière-plan Android : service connectedDevice, redémarrage, batterie | Implémenté | Comportement variable selon constructeur |
| 14.3 | États (connecté, transfert, déconnecté, erreur…) | Implémenté | Événements du cœur |
| 17 | Suspendre toutes les connexions, effacer les données | **Testé** | `suspend_closes_connections` |
| 18 | Mises à jour signées (Windows : minisign ; Android : SHA-256 + même clé) | Implémenté | Effective à partir de la 2ᵉ version publiée |
| 18 | Migrations de base (mise à jour sans perte) | **Testé** | `migrates_v1_database_without_losing_devices`, `data_survives_reopen_like_an_update` |
| 21 | Désinstalleur Windows | Implémenté | NSIS : désinstalleur, retrait « Envoyer vers », démarrage auto et clé d'identité |

## Limites de distribution

- L'installeur Windows n'est pas signé par un certificat de signature de code (payant) :
  SmartScreen affiche un avertissement au premier lancement. Les **mises à jour**, elles, sont
  signées et vérifiées par l'application.
- L'APK est distribué hors Play Store : Android demande d'autoriser la source d'installation, et
  Play Protect peut bloquer l'installation (voir [play-protect.md](play-protect.md)).
