# Notes de version

## 0.2.0

- **Miroir de l'écran du téléphone** sur le PC (H.264 matériel, jusqu'à 60 i/s), avec le son, et **contrôle** depuis le PC (clic, appui long, balayage, molette, retour/accueil/applis récentes, saisie de texte).
- **Écran étendu** : le téléphone devient un écran supplémentaire de Windows (portrait ou paysage, toucher = souris). Pilote intégré installé sans mode test, ou Virtual Display Driver (libre et signé).
- **Son dans les deux sens** : son du téléphone sur le PC, son du PC sur le téléphone.
- **Presse-papiers partagé** : automatique du PC vers le téléphone, d'un toucher du téléphone vers le PC (tuile des réglages rapides).
- **USB** : connexion par câble (ADB) prioritaire et automatique, installation de l'appli Android par câble depuis le PC (contourne le blocage Play Protect).
- **Bluetooth LE** : le téléphone trouve le PC même quand le Wi-Fi bloque la découverte.
- Les capacités du téléphone se mettent à jour sans reconnexion.

## 0.1.0 — première version

- Association sécurisée par code à 6 chiffres ou QR code (SPAKE2 + Noise), confirmation sur le PC, révocation immédiate.
- Connexion Wi-Fi locale chiffrée, découverte automatique (mDNS), reconnexion automatique.
- Transferts de fichiers et dossiers dans les deux sens : pause, reprise, annulation, reprise après coupure, vérification SHA-256, gestion des conflits, historique.
- Partage rapide : glisser-déposer, « Envoyer vers » de l'Explorateur, menu Partager d'Android, textes et liens.
- Notifications Android sur Windows : filtre par application, réponse rapide, ouvrir/ignorer, masquage du contenu sensible, historique.
- Centre de contrôle et commandes (sonner, vibrer, volume, Ne pas déranger), contrôle multimédia.
- Fonctionnement en arrière-plan, démarrage avec Windows, mises à jour signées.
