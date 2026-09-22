# PoE2 Craft Assistant

Assistant de craft pour Path of Exile 2 : simulateur, **reverse-crafting** (coût moyen minimal + arbre de décision),
lecture des objets copiés en jeu et **overlay** transparent par-dessus le jeu.

**Tu veux juste utiliser l'application ?** → [GUIDE.md](GUIDE.md)
Ce fichier-ci est pour la compilation, l'architecture et la publication de versions.

Stack : Rust (moteur, solveur, Tauri 2) + React/TypeScript (React Flow + ELK pour le graphe).

## Architecture

```
crates/craft-core    moteur exact (tirage pondéré, monnaies, Omens) + Monte-Carlo parallèle
crates/craft-solver  reverse-crafting : MDP « plus court chemin stochastique » sur un état abstrait,
                     graphe de plan, conseil sur objet réel, vérification Monte-Carlo sur le moteur exact
crates/craft-data    dataset JSON, poids par base, lecture du texte d'objet (Ctrl+C / Ctrl+Alt+C)
crates/craft-api     couche de service partagée + CLI `craft-cli`
src-tauri            application : commandes, overlay Win32, raccourcis globaux, surveillance du presse-papiers
src/                 interface (fenêtre principale) ; src/overlay = fenêtre overlay ; src/mock = moteur factice
```

Le solveur travaille sur un état abstrait (affixes voulus présents / bloqués par un tier trop bas / fracturés,
nombre de mauvais préfixes et suffixes). L'exclusion de groupe des mauvais affixes est approchée par un champ moyen ;
chaque plan est **revérifié sur le moteur exact** et l'écart est affiché dans l'application.

## Lancer

Prérequis : Rust stable (≥ 1.77), Node ≥ 20 ; sous Windows : outils de build MSVC et WebView2.

```
npm install
npm run tauri dev        # application complète
npm run tauri build      # installeur NSIS
npm run dev:web          # interface seule dans un navigateur (moteur factice, données réelles exportées)
cargo test               # tests des crates de calcul (sans dépendances système)
cargo run -p craft-api --bin craft-cli -- solve gloves_dex 81 life_flat:3 fire_res:3 cold_res:3 evasion_pct:3
npm run fixtures         # régénère src/mock/fixtures depuis le moteur Rust
```

## Installeur Windows

Un installeur `.exe` (NSIS, installation par utilisateur, sans droits administrateur, français/anglais) se construit
uniquement sous Windows. Deux façons :

- **Sur ta machine** : double-clic sur `build-installer.bat` (installe Node, Rust et les outils C++ via winget si besoin,
  puis compile ; premier build : 10 à 20 minutes). Si des outils viennent d'être installés, il faut relancer le fichier
  une fois. Résultat : `target\release\bundle\nsis\*.exe`.
- **Sans rien installer** : pousse le projet sur GitHub, onglet *Actions* → « Installeur Windows » → *Run workflow* ;
  l'installeur est téléchargeable en fin de job. Un tag `v0.1.0` le publie aussi dans une Release.

L'installeur n'est pas signé : Windows SmartScreen affichera « Windows a protégé votre PC » (Informations complémentaires →
Exécuter quand même). Une signature de code demande un certificat payant.

## Publier sur GitHub et mises à jour automatiques

Les mises à jour utilisent le système officiel de Tauri : chaque version est **signée** avec une clé que toi seul possèdes,
et l'application refuse tout ce qui n'est pas signé par elle. Le dépôt GitHub doit être **public** (l'application lit
`latest.json` dans les Releases sans identifiant).

1. Envoie le projet sur GitHub (une fois) :
   ```
   git init -b main
   git add -A
   git commit -m "Première version"
   git remote add origin https://github.com/ShigenoTV/poe2-craft-assistant.git
   git push -u origin main
   ```
2. Double-clique `setup-updates.bat` : crée la paire de clés dans `.tauri/` (ignoré par git), écrit la clé publique et
   l'adresse du dépôt dans `src-tauri/tauri.conf.json`, et t'indique les 2 secrets à ajouter sur GitHub
   (*Settings → Secrets and variables → Actions*). **Sauvegarde `.tauri/` ailleurs** : sans elle, plus aucune mise à jour possible.
3. `git commit -am "Mises à jour" && git push`.
4. Publie une version avec `release.bat` (première fois : `0.1.1`, le numéro doit changer). Après ~15 minutes, la Release
   contient l'installeur signé et `latest.json`. Installe celui-là : c'est lui qui saura se mettre à jour.
5. Pour chaque nouvelle version : `release.bat` avec le nouveau numéro. Les applications installées la proposent au démarrage
   (bandeau à gauche) et dans Réglages → Mises à jour ; rien n'est installé sans clic.

### Vérifier que la chaîne de mise à jour marche (test en trois temps)

1. **Compilation Windows, sans clé** : sur GitHub, onglet *Actions* → « Installeur Windows » → *Run workflow*. Vert = le projet
   compile sous Windows (première vérification du code de l'overlay) ; télécharge l'artefact, installe, lance l'app.
2. **Signature** : après `setup-updates.bat` et `release.bat 0.1.1`, double-clique `check-release.bat`. Il contrôle
   `latest.json`, l'installeur et que la signature vient bien de la clé embarquée dans l'app.
3. **Mise à jour réelle** : installe la 0.1.1 depuis la Release, publie une 0.1.2 avec `release.bat`, puis `check-release.bat`
   de nouveau. Ouvre la 0.1.1 : le bandeau « Version 0.1.2 disponible » apparaît ; « Installer et redémarrer » ; Réglages
   doit afficher 0.1.2.

## En jeu

1. Dans « Reverse-crafting », choisis la base et les affixes voulus, garde « Utiliser ce plan dans l'overlay », calcule.
2. En jeu, survole un objet et copie-le : `Ctrl+Alt+C` (détails complets, format le plus fiable) ou `Ctrl+C`.
3. `Ctrl+D` affiche/masque l'overlay ; `Ctrl+Shift+D` le rend interactif (il capte alors la souris). Modifiable dans « Réglages ».
4. Le jeu doit être en fenêtré ou fenêtré sans bordure (pas de plein écran exclusif).

## Limites connues

- **Données** : le dataset embarqué est un vrai export du jeu (`update-dataset.bat` pour le rafraîchir, voir `docs/DATA.md`).
- **Presse-papiers** : testé sur 3 vrais objets rares (bâton, gants, bottes corrompues avec mod désécré ; voir
  `crates/craft-data/tests/fixtures`). Sur ce client, `Ctrl+C` et `Ctrl+Alt+C` donnent le même texte, avec en-têtes
  `{ Prefix Modifier "…" (Tier: N) — … }`. Non vus : objets magiques, normaux, fracturés (en-tête supposé
  « Fractured … Modifier ») et client en français. Le panneau « Objet en jeu » affiche ce qui n'est pas reconnu.
- **Windows** : `src-tauri/src/platform.rs` (suivi de fenêtre, styles NOACTIVATE) n'a pas pu être compilé sur la
  plateforme cible pendant le développement ; le reste de la coque Tauri compile sous Linux.
- **Règles de craft** : Essences, Désécration, Vaal, etc. ne sont pas modélisés. Règles codées listées dans `docs/DATA.md`.
- **Solveur** : approximation de champ moyen validée à ±1 % sur les cas testés ; la vérification affichée détecte les dérives.
