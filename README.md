# NOTHING CONNECT

Relie un téléphone Android à un PC Windows sur votre réseau Wi-Fi local, sans compte ni serveur Internet.

- **Partage** de fichiers, dossiers, textes et liens dans les deux sens, avec pause, reprise et vérification d'intégrité (SHA-256).
- **Notifications** du téléphone sur Windows, avec filtre par application, réponse rapide et masquage du contenu sensible.
- **Centre de contrôle** : batterie, stockage, réseau ; faire sonner, vibrer, volume, Ne pas déranger.
- **Multimédia** : contrôle de Spotify, YouTube Music, Deezer… depuis le PC.
- **Arrière-plan** : zone de notification Windows, service Android discret, reconnexion automatique.
- **Vraies mises à jour** : installées par-dessus l'ancienne version, appareils et réglages conservés.

L'état réel de chaque fonction (fait, testé, pas encore commencé) est tenu à jour dans
[docs/matrice-fonctionnalites.md](docs/matrice-fonctionnalites.md).

## Installer

1. Ouvrez la page [Releases](../../releases/latest).
2. **Windows** : téléchargez `NOTHING CONNECT_x.y.z_x64-setup.exe` et lancez-le. Aucun droit administrateur n'est demandé. Au premier lancement, autorisez l'application dans le pare-feu Windows pour les **réseaux privés**.
3. **Android** (10 ou plus récent) : téléchargez le fichier `.apk` sur le téléphone et ouvrez-le. Android demande d'autoriser l'installation depuis cette source.
4. Sur le PC : **Associer un téléphone**. Sur le téléphone : scannez le QR code, ou choisissez le PC détecté et tapez le code à 6 chiffres. Confirmez sur le PC.

Guide détaillé : [docs/installation.md](docs/installation.md). Problèmes : [docs/depannage.md](docs/depannage.md).

## Désinstaller

- **Windows** : Paramètres > Applications > NOTHING CONNECT > Désinstaller (ou le raccourci « Désinstaller » du menu Démarrer). Cochez « Supprimer les données de l'application » pour tout effacer, y compris la clé d'identité stockée dans le Gestionnaire d'identification Windows.
- **Android** : appui long sur l'icône > Désinstaller.

## Pour les développeurs

| Dossier | Contenu |
|---|---|
| `crates/` | Cœur Rust partagé : protocole, sécurité, association, transport, transferts, stockage, orchestration, liaison Android |
| `apps/windows/` | Application Tauri 2 (Svelte + TypeScript), installeur NSIS |
| `apps/android/` | Application Kotlin / Jetpack Compose |
| `docs/` | Architecture, protocole, sécurité, permissions, matrice des fonctions |

```bash
cargo test                                   # cœur Rust (tests unitaires + bout en bout)
cargo test --release -p nc-core --test e2e -- throughput --nocapture
cd apps/windows && npm ci && npx tauri dev   # application Windows
./scripts/build-android-native.sh            # bibliothèques natives + liaisons Kotlin
cargo run -p nc-core --example fake_phone -- 127.0.0.1:47820 123456   # téléphone simulé
```

Publier une version : `python3 scripts/set-version.py 0.2.0`, commit sur `main`, puis `git push origin main:release` (ou bouton « Run workflow » de Publication dans l'onglet Actions).
La CI construit, signe et publie l'installeur Windows, l'APK et le fichier de mise à jour.

Licence : MIT.
