# Notes de version

## 0.4.0

- **Écran étendu tactile** : le téléphone devient un vrai écran tactile pour Windows (toucher multipoint) : glisser pour défiler, pincer pour zoomer, appui long pour le clic droit, déplacer une fenêtre par sa barre de titre. Le mode souris reste disponible (réglage « Toucher »).
- **Gestes façon smartphone** : 3 doigts vers le haut = vue des tâches, vers le bas = bureau, à gauche/droite = application suivante/précédente ; 4 doigts à gauche/droite = bureau virtuel.
- **Retour d'Android** = page précédente dans Windows (l'écran ne se ferme plus par erreur).
- **Bouton flottant** sur le téléphone (déplaçable) : clavier du téléphone (ce qui est tapé s'écrit sur le PC), clavier tactile de Windows, débrancher l'écran.
- Protocole plus tolérant : un message d'une version plus récente est ignoré au lieu de couper la connexion.

## 0.3.1

- Écran étendu : Windows plaçait souvent l'écran virtuel à 60 Hz même pour un téléphone à 90 ou 120 Hz, ce qui plafonnait l'affichage à 60 images/s. La fréquence du téléphone est maintenant imposée et vérifiée ; le compteur « Hz » de l'accueil affiche la fréquence réellement appliquée par Windows.
- « Images par seconde » est réglé sur « Max » par défaut.

## 0.3.0

- **Écran étendu beaucoup plus rapide** : vidéo H.264 au lieu d'images JPEG. Seules les différences entre images sont envoyées (débit divisé par 10 à 100 sur un bureau) et le téléphone décode avec son circuit vidéo matériel. Mesuré sur un bureau en 2400×1080 : 274 Ko par image en JPEG contre quelques Ko en H.264.
- **Fréquence maximale du téléphone** : l'écran virtuel prend la fréquence de l'écran du téléphone (90, 120 Hz…) et le téléphone passe en mode rapide ; choix « Max » dans les images par seconde.
- **Réglages enregistrés automatiquement** (écran étendu et miroir), conservés après les mises à jour ; qualité et images/s modifiables pendant une session.
- **Mesures en direct** : images/s, fréquence, débit, définition.
- **Nouvelle interface** : l'écran étendu se pilote depuis l'accueil (l'onglet séparé disparaît), barre latérale avec pictogrammes en points, état du téléphone (USB/Wi-Fi, batterie) toujours visible, sélecteurs rapides à la place des listes déroulantes.
- **Nouvelle icône** : monogramme NC en matrice de points.
- Compatibilité : un PC ou un téléphone resté en 0.2 continue de fonctionner (images JPEG).

## 0.2.1

- Écran étendu : le moniteur virtuel prend exactement le format de l'écran du téléphone (l'image remplit tout l'écran) et le toucher tombe au bon endroit, même s'il reste des bandes noires.

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
