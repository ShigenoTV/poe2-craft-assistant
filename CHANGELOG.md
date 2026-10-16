# Notes de version

Une section par version, la plus récente en haut. Les notes de la version en préparation s'écrivent sous
« Prochaine version » : `release.mjs` la renomme avec le numéro publié, puis la Release GitHub et la fenêtre
« Nouveautés » de l'appli en reprennent le texte. Format : `### Nouveautés`, `### Corrections`, une puce par changement.

## Prochaine version

### Nouveautés

- Réglages : les raccourcis se choisissent au clavier. On clique dans le champ, on appuie sur la combinaison voulue, et elle s'applique tout de suite (plus besoin de l'écrire ni de cliquer sur « Enregistrer »).
- Réglages : l'historique des versions ne garde que les 5 dernières.

### Corrections

- Raccourcis : l'ancien raccourci (Ctrl+D par défaut) ne peut plus afficher l'overlay après un changement. Une deuxième copie de l'appli lancée par erreur gardait ses propres raccourcis et son overlay ; l'appli ne s'ouvre plus qu'une fois (la relancer ramène la fenêtre déjà ouverte).
- Raccourcis : un raccourci modifié sans cliquer sur « Enregistrer » restait affiché dans le champ mais n'était pas appliqué ; ils s'appliquent maintenant dès la saisie, avec un message qui confirme les raccourcis actifs.

## 0.3.0 (2026-10-16)

### Corrections

- Prix : les Omens of Whittling, Light, Necromancy, Sovereign, Liege et Blackblooded et les 6 os de Désécration sont maintenant relevés sur poe.ninja (ils restaient à 0,5 Exalted alors que Whittling en vaut des milliers, et le planificateur les croyait presque gratuits).
- Un seul mod crafted par objet, comme dans le jeu depuis 0.5.0 : après une Essence, un Alloy ou une Liquid Emotion, le planificateur ne propose plus d'en poser un deuxième.
- Greater Orb of Transmutation et d'Augmentation : niveau de mod minimum 44 (au lieu de 35) ; versions Perfect : 70 (au lieu de 50).
- Omens of Sinistral/Dextral Coronation retirés : ils ne s'obtiennent plus en jeu depuis la 0.3.
- Orb of Alchemy utilisable sur un objet magique (il devient rare avec 4 nouveaux mods), comme dans le jeu.

## 0.2.48 (2026-10-15)

### Nouveautés

- Notes de version : après une mise à jour, une fenêtre « Nouveautés » résume ce qui a changé (une seule fois).
- Réglages : nouvel historique des versions, et les nouveautés d'une mise à jour sont lisibles avant de l'installer.
