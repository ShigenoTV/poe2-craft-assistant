# Guide d'utilisation — PoE2 Craft Assistant

Ce guide est pour toi si tu utilises l'application. Pour compiler le projet ou publier une version,
voir [README.md](README.md).

## Installation

Télécharge l'installeur `.exe` depuis [l'onglet Releases](https://github.com/ShigenoTV/poe2-craft-assistant/releases)
du dépôt, lance-le. Il ne demande pas les droits administrateur.

Windows affichera probablement un avertissement SmartScreen (« Windows a protégé votre PC ») — c'est normal
pour un installeur non signé par un éditeur commercial. Clique sur *Informations complémentaires* puis
*Exécuter quand même*.

## Les quatre écrans

L'application s'ouvre sur **Reverse-crafting**. La barre de gauche donne accès aux quatre autres :

### Reverse-crafting

L'écran principal. Tu choisis une base d'objet, un niveau d'objet, et les affixes que tu veux dessus
(jusqu'à 6, au maximum 3 préfixes et 3 suffixes). Pour chaque affixe, une barre de tiers permet de choisir
le tier minimum accepté : « T3 » veut dire T1, T2 ou T3 acceptés.

Une fois l'objectif défini, clique sur **Calculer le plan**. L'application cherche la suite de monnaies
qui coûte le moins cher en moyenne pour l'obtenir, et affiche :
- un résumé en haut (coût moyen, coût médian, budget qui suffit 9 fois sur 10) ;
- l'**arbre de décision** : chaque case est une étape, les flèches montrent ce qui peut arriver et avec
  quelle probabilité (vert = ça progresse, rouge en pointillés = ça recule) ;
- la **liste de courses** : combien de chaque monnaie acheter en moyenne, et le coût total.

### Simulateur

Pour tester à la main : applique des monnaies une par une sur un objet virtuel et regarde ce qui se passe,
ou calcule la probabilité d'obtenir un affixe donné en répétant une monnaie un certain nombre de fois.

### Objet en jeu

Colle ou copie le texte d'un objet du jeu ici pour voir ce que l'application y reconnaît, et éventuellement
le conseil associé si un plan est actif. Utile pour vérifier que la lecture du presse-papiers fonctionne
correctement avant de compter dessus en jeu.

### Données

Affiche les affixes et tiers utilisés pour les calculs, et permet d'en importer un autre fichier.

### Réglages

Raccourcis clavier, position et taille de l'overlay, prix, et vérification des mises à jour.

## Utiliser l'overlay en jeu

1. Dans **Reverse-crafting**, calcule un plan en gardant coché *« Utiliser ce plan dans l'overlay en jeu »*.
2. Lance Path of Exile 2 en **fenêtré** ou **fenêtré sans bordure** (un overlay ne peut pas s'afficher
   par-dessus un jeu en plein écran exclusif).
3. En jeu, survole l'objet que tu es en train de craft et copie-le avec **Ctrl+Alt+C** (donne le plus de
   détails et est le format le plus fiable) ou **Ctrl+C**.
4. L'overlay apparaît avec l'état de chaque affixe voulu et la prochaine étape conseillée, avec les
   probabilités de chaque issue possible.
5. **Ctrl+D** affiche ou masque l'overlay à la main. **Ctrl+Shift+D** le rend interactif (il capte alors
   la souris, pour pouvoir cliquer dedans) ; refais Ctrl+Shift+D pour repasser en clic-traversant.
6. L'overlay se masque tout seul quelques secondes après une copie (réglable dans Réglages, 0 = jamais).
7. Si les raccourcis ne répondent pas (pris par une autre application), une icône apparaît près de
   l'horloge de Windows : clic droit pour afficher, masquer ou quitter.

## Les prix

Par défaut, l'application utilise les prix relevés sur [poe.ninja](https://poe.ninja) au moment de sa
sortie. Dans **Réglages**, le bouton *Actualiser depuis poe.ninja* récupère les prix actuels (l'app le
fait aussi automatiquement au démarrage si les prix ont plus d'une heure). Tu peux aussi saisir un prix
à la main pour n'importe quelle monnaie : il prend le pas sur poe.ninja jusqu'à ce que tu cliques sur
*Réinitialiser*.

## Les données de craft

Le jeu de données utilisé pour les calculs (quels affixes existent, leurs poids, leurs tiers) est un
vrai export du jeu, embarqué directement dans l'application — il n'y a rien à importer ni à configurer
de ton côté. La version affichée dans l'écran **Données** correspond à l'export utilisé au moment de la
publication de cette version de l'application.

## Mises à jour

Un bandeau apparaît en bas de la barre de gauche quand une nouvelle version est disponible. Clique dessus
pour l'installer : l'application se ferme, s'actualise, et se relance automatiquement. Tu peux aussi
forcer une vérification à tout moment avec le bouton *Rechercher une mise à jour*.

## Problèmes courants

- **L'overlay ne s'affiche pas en jeu** : vérifie que PoE2 est en fenêtré ou fenêtré sans bordure, pas
  en plein écran exclusif.
- **Un objet copié n'est pas reconnu** : colle son texte dans l'écran *Objet en jeu* pour voir le
  message d'erreur exact. Le format le plus fiable est Ctrl+Alt+C.
- **Les raccourcis ne répondent pas** : une autre application (souvent un navigateur ou un autre overlay)
  les utilise peut-être déjà. Change-les dans Réglages, ou utilise l'icône près de l'horloge.
- **Les résultats semblent faux ou incohérents** : vérifie dans l'onglet *Objectif* du plan si la
  convergence est indiquée comme atteinte. Si non, relance le calcul — le solveur peut avoir besoin
  d'un peu plus de temps sur des objectifs complexes avec beaucoup d'affixes.
