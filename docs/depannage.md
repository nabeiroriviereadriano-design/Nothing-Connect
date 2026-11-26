# Dépannage

| Symptôme | Solution |
|---|---|
| Le téléphone ne trouve pas le PC | Même Wi-Fi des deux côtés ; profil réseau Windows **Privé** ; pare-feu : autoriser NOTHING CONNECT sur les réseaux privés ; certains routeurs (Wi-Fi invité, « isolation des clients ») bloquent la communication entre appareils : utilisez le réseau principal |
| PC trouvé mais association impossible | Le code expire après 120 s et 3 essais : générez-en un nouveau. Vérifiez l'heure du message « association refusée » sur le PC |
| « Association révoquée » | Le PC a supprimé ce téléphone. Réassociez-le |
| La connexion tombe écran éteint | Téléphone : Accueil > « Ne pas restreindre » (batterie). Sur certaines marques, autorisez aussi le démarrage automatique de l'appli |
| Aucune notification sur le PC | Téléphone : accès aux notifications autorisé ? PC : Paramètres > Notifications activées ? L'application n'est-elle pas désactivée dans Notifications > Applications ? |
| « Version incompatible » | Mettez à jour l'appareil indiqué (PC ou téléphone) |
| Transfert interrompu | Il reprend seul à la reconnexion, à l'endroit exact. Les fichiers partiels abandonnés sont effacés après 7 jours |
| Fichier reçu introuvable (Android) | Dossier Téléchargements > NOTHING CONNECT |
| Fichier reçu introuvable (Windows) | Téléchargements\NOTHING CONNECT, ou le dossier choisi dans Paramètres. Bouton « Afficher » dans l'historique |

## Journaux

- Windows : Paramètres > Diagnostic > Ouvrir les journaux (`%APPDATA%\io.github.nabeiroriviereadriano.nothingconnect\logs`), 7 jours conservés.
- Android : `adb logcat -s nc-core`.

Les journaux ne contiennent ni clés ni contenus ; vous pouvez les joindre à un signalement.
