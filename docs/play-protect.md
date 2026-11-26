# Installer l'application Android malgré Play Protect

## Pourquoi Play Protect bloque

L'application est distribuée hors du Play Store et demande des accès sensibles : accès aux
notifications et service d'accessibilité (contrôle depuis le PC). Quand une telle application
est installée depuis un navigateur, une messagerie ou un gestionnaire de fichiers, Play Protect
peut bloquer l'installation (« Application bloquée pour protéger votre appareil »). Ce blocage
dépend des permissions déclarées, pas d'un virus détecté.

## Solutions, de la plus simple à la plus durable

### 1. Installer par câble depuis le PC (recommandé)

Une installation par câble USB (ADB) n'est pas concernée par ce blocage.

1. Sur le téléphone : Paramètres › À propos du téléphone › touchez 7 fois « Numéro de build »
   (Nothing : « Version de Nothing OS »), puis Paramètres › Système › Options pour les
   développeurs › **Débogage USB**.
2. Branchez le téléphone au PC et acceptez « Autoriser le débogage USB » sur le téléphone.
3. Dans NOTHING CONNECT sur le PC : page **Appareils** › « Installer sur le téléphone ».
   L'application télécharge l'APK de la dernière version, vérifie son empreinte SHA-256 et
   l'installe. Les mises à jour suivantes se font de la même façon ou depuis le téléphone.

Avantage supplémentaire : Android ne grise pas l'accès aux notifications ni l'accessibilité pour
une application installée ainsi (pas de « paramètres restreints »).

### 2. Sur le téléphone, sans câble

- Si la fenêtre de Play Protect propose **Plus d'infos › Installer quand même**, utilisez-le.
- Sinon, désactivez Play Protect le temps de l'installation : Play Store › photo de profil ›
  Play Protect › roue dentée › décochez « Analyser les applications avec Play Protect ».
  Installez l'APK, puis **réactivez** l'analyse.
- Ensuite, pour l'accès aux notifications et l'accessibilité : Paramètres › Applications ›
  NOTHING CONNECT › menu ⋮ › « Autoriser les paramètres restreints ».

### 3. Demander à Google de réexaminer l'application

Formulaire officiel : <https://support.google.com/googleplay/android-developer/contact/protectappeals>.
À remplir par le titulaire du dépôt GitHub, en anglais. Texte prêt à coller :

> **App name:** NOTHING CONNECT
> **Package name:** io.github.nabeiroriviereadriano.nothingconnect
> **Signing certificate SHA-256:** 6B:36:39:78:78:90:DA:88:6F:6B:12:16:D0:A8:C5:BC:1E:65:46:5E:39:6D:8E:D1:4E:30:CE:23:1F:F7:A2:55
> **Download:** https://github.com/nabeiroriviereadriano-design/Nothing-Connect/releases/latest
> **Source code:** https://github.com/nabeiroriviereadriano-design/Nothing-Connect (MIT licence, built and signed by public GitHub Actions workflows)
>
> NOTHING CONNECT is an open-source companion app that links an Android phone to the user's own
> Windows PC over the local network or a USB cable (file transfer, notification mirroring, screen
> mirroring, shared clipboard). There is no server and no account: the phone only talks to PCs the
> user has explicitly paired with a 6-digit code or QR code, over an end-to-end encrypted channel
> (Noise protocol, SPAKE2 pairing).
>
> Sensitive permissions and why they are needed:
> - NotificationListenerService: shows the phone's notifications on the paired PC and lets the
>   user reply or dismiss them from the PC. Apps can be excluded; nothing leaves the local network.
> - AccessibilityService (isAccessibilityTool=false, optional, off by default): lets the user tap,
>   swipe and type on the phone from the PC while the phone screen is mirrored. It listens to no
>   events, reads no screen content except the focused input field when the user types from the
>   PC, and only acts on commands from paired PCs.
> - MediaProjection + RECORD_AUDIO: screen and app-audio mirroring to the PC, with Android's
>   consent dialog shown for every session. The microphone is never used.
>
> The app collects no data, contains no ads or trackers, and does not access SMS, contacts,
> location, accounts or financial data. We believe the installation block is a false positive
> caused by the combination of permissions, and we ask that installs of this signed package be
> allowed.

Un recours ne garantit pas un résultat : ce blocage découle d'une règle sur les permissions,
pas d'une détection d'erreur.

### 4. Vérification des développeurs Android (2026-2027)

Google déploie la vérification obligatoire de l'identité des développeurs pour les applications
installées hors Play Store. Pour une application comme celle-ci, deux voies existent :

- **Compte à diffusion limitée** (gratuit, étudiants et amateurs) : partage avec un petit groupe
  d'appareils (une vingtaine) sans vérification complète.
- **Parcours avancé** côté utilisateur : mode développeur activé, redémarrage, attente unique
  d'une journée, puis confirmation par empreinte ou code ; ensuite « Installer quand même »
  reste possible pour les développeurs non vérifiés.

L'installation par câble (solution 1) reste la plus simple et n'est pas concernée.
