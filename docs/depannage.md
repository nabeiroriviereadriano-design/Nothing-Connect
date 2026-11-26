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

## L'application Android ne s'installe pas (Play Protect)

Voir [play-protect.md](play-protect.md) : le plus simple est l'installation par câble depuis le PC.

## L'écran étendu ne démarre pas

- Page Écran étendu : vérifiez qu'un pilote est installé. Si le pilote intégré est refusé,
  installez Virtual Display Driver. Détails : [pilote-ecran.md](pilote-ecran.md).
- Le téléphone doit afficher la notification « … veut utiliser ce téléphone comme écran » :
  touchez-la si l'écran ne s'ouvre pas tout seul (Android limite l'ouverture en arrière-plan).

## Le miroir reste noir ou le contrôle ne répond pas

- Acceptez la fenêtre Android « Commencer à diffuser ? » sur le téléphone (demandée à chaque fois).
- Pour le contrôle : activez « NOTHING CONNECT : contrôle depuis le PC » dans Accessibilité.
- Le son d'une application peut manquer : certaines interdisent sa capture (choix du développeur).
