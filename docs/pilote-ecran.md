# Écran étendu : pilote d'écran virtuel

Pour que le téléphone devienne un écran supplémentaire, Windows doit voir un vrai moniteur.
Il faut donc un pilote d'écran virtuel (Indirect Display, IddCx). NOTHING CONNECT sait utiliser
deux pilotes ; la page **Écran étendu** de l'application installe l'un ou l'autre.

## 1. Pilote NOTHING CONNECT (recommandé)

- Fourni avec l'application, installé d'un clic (Windows demande une autorisation administrateur).
- Pilote **en mode utilisateur** (UMDF) : Windows n'impose la signature Microsoft qu'aux pilotes
  du noyau. Il suffit que son certificat soit approuvé sur le PC. L'outil d'installation l'ajoute
  aux magasins « Autorités de certification racines de confiance » et « Éditeurs approuvés » de
  l'ordinateur, puis installe le pilote. **Pas de mode test, pas de réglage du BIOS.**
- Sécurité : la clé privée de ce certificat est créée pendant la compilation (GitHub Actions),
  sert à signer le pilote, puis est **détruite**. Le certificat ajouté au PC ne peut donc rien
  signer d'autre. Il est retiré avec le pilote (bouton « Retirer le pilote » ou désinstallation
  de NOTHING CONNECT).
- Mises à jour : le pilote est conservé, que la mise à jour vienne de l'application ou d'un
  nouvel installateur lancé par-dessus (même avec « Désinstaller avant d'installer »). Avant la
  0.5.1, ce second cas le retirait et il fallait le réinstaller.
- Fonctionnement : le moniteur n'existe que pendant une session. Le téléphone annonce la taille
  de son écran, le pilote crée un moniteur de cette taille, l'image passe par une mémoire
  partagée locale puis est compressée en JPEG vers le téléphone. Le toucher sur le téléphone
  devient la souris sur ce moniteur.
- Code dérivé de l'exemple Microsoft *IddSampleDriver* (licence MS-PL, fichier
  `LICENSE-MS-PL.txt` livré avec le pilote).

Si un PC refuse quand même ce pilote (stratégie d'entreprise, version de Windows), l'application
le signale et propose la solution 2, ou en dernier recours le mode signature de test
(`bcdedit /set testsigning on`, redémarrage, Secure Boot désactivé).

## 2. Virtual Display Driver (alternative)

- Pilote libre (licence MIT) de VirtualDrivers : <https://github.com/VirtualDrivers/Virtual-Display-Driver>,
  signé par son auteur. Installé par l'application avec `winget` depuis son dépôt officiel.
- NOTHING CONNECT le pilote par son canal de commande local (création et retrait de l'écran,
  résolution du téléphone ajoutée à ses réglages) et capture l'image avec l'API Desktop
  Duplication de Windows.
- Désinstallation : Paramètres Windows › Applications (il n'appartient pas à NOTHING CONNECT).

## Codes de l'outil d'installation (`NcDisplaySetup.exe`)

| Code | Signification |
|---|---|
| 0 | Terminé |
| 2 | Pilote absent du dossier de l'application |
| 3 | Autorisation administrateur refusée |
| 10 | Windows exige une signature Microsoft pour ce pilote sur ce PC |
| 11 | Secure Boot actif : le mode test ne peut pas être activé |
| 12 | Redémarrage nécessaire |
