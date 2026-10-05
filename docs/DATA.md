# Données de jeu

Tout le moteur lit un fichier JSON unique (schéma `1`). Le fichier embarqué, `data/sample/dataset.json`, est
un vrai export du jeu (RePoE) — pas un exemple. Il n'y a rien à importer depuis l'application : pour le
rafraîchir après une mise à jour de Path of Exile 2, voir `update-dataset.bat` plus bas.

## Schéma

```jsonc
{
  "meta":   { "schema": 1, "source": "poe2db 2026-09-01", "game_version": "0.x", "generated_at": "2026-09-01",
              "notice": "", "price_unit": "Exalted Orb" },
  "tags":   ["gloves", "dex_armour", "life", ...],          // ≤ 64 tags
  "bases":  [{ "id": "gloves_dex", "name": "…", "item_class": "Gloves", "tags": ["gloves", "dex_armour"] }],
  "mods":   [{
      "id": "life_flat_3",            // unique
      "group": "life_flat",           // famille : deux mods du même groupe ne coexistent pas
      "family": "Maximum Life",       // libellé affiché
      "name": "Sturdy",               // nom du tier (sert à l'appariement du texte copié en jeu)
      "slot": "prefix",               // "prefix" | "suffix"
      "level": 46,                    // niveau de mod (ilvl requis)
      "text": "+(40-49) to maximum Life",   // plage entre parenthèses : sert à reconnaître le tier
      "tags": ["life"],
      "spawn": [{ "tag": "dex_armour", "weight": 1000 }, { "tag": "default", "weight": 0 }]
  }],
  "currencies": [{ "id": "exalt_perfect", "label": "Perfect Exalted Orb", "kind": "exalt",
                   "min_mod_level": 50, "price_id": "exalt_perfect", "default_enabled": true }],
  "omens":  [{ "id": "omen_dextral_exaltation", "label": "…", "add_slot": "suffix", "remove_slot": null,
               "applies_to": ["exalt"], "price_id": "omen_dextral_exaltation" }],
  "prices": { "exalt": 1.0, "base_white": 2.0, "base_salvage": 0.0, "...": 0 }
}
```

- **Poids par base** : pour chaque mod, le premier élément de `spawn` dont le tag est porté par la base fixe le poids
  (règle du jeu ; `default` en dernier). Un poids de 0 exclut le mod de cette base.
- **Tiers** : calculés à l'import dans chaque groupe, par niveau décroissant (T1 = niveau le plus haut).
- **Actions** : chaque monnaie est combinée automatiquement à chaque Omen compatible (`applies_to`), prix additionnés.
- **Valeurs de `kind`** : `transmute`, `augment`, `regal`, `alchemy`, `exalt`, `chaos`, `annul`, `fracture`.

## Écrire un importeur poe2db

Un importeur doit produire ce JSON à partir des pages de modificateurs de poe2db.tw (une entrée `mods` par tier,
les poids par tag de base dans `spawn`). Il n'est **pas fourni** : je n'ai pas pu joindre poe2db depuis mon
environnement, donc je n'ai pas pu écrire ni tester ses sélecteurs. Le chargeur valide le fichier
(`Dataset::from_json`) et renvoie une erreur lisible en cas de problème.

## Règles de craft codées en dur (à vérifier)

Le noyau (`crates/craft-core/src/model.rs`) code : magique = 1 préfixe + 1 suffixe, rare = 3 + 3 ; Alchimie = rare
avec 4 affixes ; Chaos = retrait puis ajout ; Fracture = rare avec ≥ 4 affixes, un seul mod fracturé ; Greater/Perfect =
niveau de mod minimal (`min_mod_level`, piloté par les données). Vérifie ces règles contre la version du jeu visée.


## Mettre à jour les données de jeu

Le jeu de données embarqué (`data/sample/dataset.json`, chargé par `Dataset::embedded()`) est désormais
un **vrai** export du jeu (RePoE, via repoe-fork.github.io/poe2), pas un exemple illustratif. Pour le
rafraîchir après une mise à jour de Path of Exile 2 :

```
update-dataset.bat
```

Il télécharge `mods.min.json` et `base_items.min.json` depuis
[repoe-fork.github.io/poe2](https://repoe-fork.github.io/poe2/) (le site est généré par une action GitHub,
pas stocké en clair dans un dépôt : seul un navigateur ou une machine avec accès internet normal peut
l'atteindre — impossible depuis l'environnement de développement sandboxé), puis lance
`tools/import_repoe.mjs` pour produire `data/sample/dataset.json`. Vérifie ensuite que l'application
fonctionne toujours (`npm run tauri dev`) avant de commiter et publier une nouvelle version.

État au 25/09/2026 : 59 bases (dont 4 joyaux Basiques et 2 classes d'arme ajoutées le 25/09, 41 Essences/Alloys vérifiées ajoutées le 26-27/09 — les 13 Alloys Verisium sont TOUS couverts (vérifié par comptage explicite) — Talisman et
Trap, trouvées via l'arborescence poe2db plutôt que via l'export RePoE), 1978 affixes, 267 groupes
d'exclusion. Deux numéros de version coexistent et
ne se correspondent pas terme à terme, à ne pas confondre :
- **version RePoE** (celle qui compte pour savoir si les données sont à jour) : affichée dans le titre de
  https://repoe-fork.github.io/poe2/ (« RePoE - PoE2 version X.Y.Z.W ») — au moment de la génération de ce
  dataset : `4.5.5.2`. Probablement un numéro de build interne du jeu, pas le nom de patch public.
- **nom de patch public du jeu** (communication marketing GGG) : `0.5.5`, « The Forbidden Rites ».

`update-dataset.bat` ne capture aucun des deux automatiquement dans le fichier généré — seulement la date.
Pour savoir si tes données sont à jour, compare le numéro RePoE affiché sur le site au moment de l'import.

Ce que fait l'import, et pourquoi (voir aussi les commentaires en tête de `tools/import_repoe.mjs`) :
- Mods retenus : `domain == "item"`, `generation_type` préfixe ou suffixe, hors mods réservés aux Essences.
- Groupe d'exclusion = le champ brut `groups[0]` du jeu (seule source de vérité mécanique). Un groupe
  peut contenir plusieurs affixes distincts (ex. `IncreaseSocketedGemLevel` = niveau de tous les sorts,
  des sorts de feu, des compétences de mêlée, des sbires...) : chacun forme une **famille** (champ `type`
  du jeu) avec ses propres tiers, choisie directement comme affixe voulu, comme sur craftofexile. Son
  libellé est le texte majoritaire de la famille, nombres remplacés par `#` (« +# to Level of all Fire
  Spell Skills »). Deux familles d'un même groupe restent mutuellement exclusives sur l'objet. Clé
  d'objectif : `Groupe::Famille` quand le groupe a plusieurs familles sur la base, sinon `Groupe` ; une
  ancienne clé `Groupe` seule retombe sur la famille du groupe qui a le plus de tiers.
- Poids d'apparition : les données du jeu PoE2 (RePoE, poe2db) donnent 1 à tous les tiers, GGG ne publie
  pas les vrais poids. Ils viennent donc des estimations de Craft of Exile (recombinateur, communauté
  Prohibited Library : https://www.craftofexile.com/weightings?game=poe2), fichier
  `https://www.craftofexile.com/json/poe2/main/poec_data.json`, reporté par `tools/coe_weights.mjs`
  (appelé par `import_repoe.mjs --coe <fichier>`). Chaque base reçoit `weight_key` (la base Craft of
  Exile équivalente) et chaque mod `weights` (`"*"` = défaut, plus les exceptions par base). Le jeu
  décide toujours si un mod est possible sur une base ; Craft of Exile ne fait que le pondérer. Restent
  aux poids du jeu (tous égaux) : les bases sans équivalent (armures str/dex/int, joyau prismatique,
  piège) et celles où Craft of Exile n'a aucun poids (griffe, dague, fléau, épées et haches). Sans
  `--coe`, l'import reprend les poids du dataset précédent. `meta.weights_source` dit d'où ils viennent.
- Bases retenues : équipement uniquement, `release_state == "released"`. Les 4 classes d'armure
  principales + Shield sont scindées par archétype d'attribut (str/dex/int et hybrides) ; le reste a un
  seul représentant par classe, au plus haut niveau de drop (l'équivalent « fin de jeu »).
- Bijoux (Anneau, Amulette, Ceinture, Carquois) : chaque vraie base est importée avec son implicite
  (champ `implicits`, affiché à côté du nom et utilisé pour reconnaître la variante d'un objet collé,
  ex. les trois Two-Stone Ring). Sur RePoE 4.5.5.2 : 28 anneaux, 25 amulettes, 19 ceintures, 11 carquois.
  Les 13 bases dont l'implicite change le nombre de préfixes/suffixes autorisés (Dusk/Gloam/Penumbra/
  Tenebrous Ring et Amulet, Lament, Portent, Absent, Twisted, Distorted Amulet) portent ce décalage dans
  `prefix_cap_delta` / `suffix_cap_delta` (ex. Penumbra +2/-2 : 5 préfixes / 1 suffixe en Rare). Le
  moteur et le solveur l'appliquent au plafond de la rareté, Magique compris (ex. Dusk Ring Magique :
  2 préfixes / 0 suffixe) — ce dernier point est déduit de la stat, pas confirmé en jeu.
- Mods Désécrés (04/10/2026) : domaine `desecrated` du jeu, uniquement ceux d'un des trois seigneurs
  (tag `ulaman_mod`, `amanamu_mod` ou `kurgal_mod`) : 197 sur RePoE 4.5.5.2 (69 Amanamu, 64 Kurgal,
  64 Ulaman), marqués `desecrated`. Chacun forme sa propre famille « texte (Seigneur) », et le tag du
  seigneur est placé en premier (filtré par les Omens Sovereign/Liege/Blackblooded). Pas importés : les
  32 mods Désécrés de joyau sans seigneur (aucune source ne dit quel os les donne), et ceux de l'arbre
  Genesis, des cartes et des uniques. Os (poe2db) : Rib = armure, Collarbone = amulette/anneau/ceinture,
  Jawbone = arme ou carquois (champ `item_tags` de la monnaie) ; Ancient = mod de niveau 40 minimum.
  Gnawed (objet de niveau 64 maximum) n'est pas modélisé. Prix des os : saisis à la main.
- Non importé : Essences, mods de corruption, mods d'objets uniques.

Essences Greater et Perfect (04/10/2026), ajoutées à la main dans `essences` : 18 Greater et 18
Perfect (toutes sauf « the Infinite », qui donne au hasard Force, Dextérité ou Intelligence — un
choix aléatoire entre plusieurs mods que le modèle « un mod garanti » ne sait pas représenter).
Source : onglet Essences de Craft of Exile (mod exact par classe d'objet), recoupé avec poe2db pour
les valeurs. Seul désaccord : l'arbalète, rangée par Craft of Exile avec les armes à une main pour les
Greater ; poe2db (« Two Handed Melee Weapon or Crossbow ») fait foi. Les Greater ciblent des mods
normaux déjà importés ; les Perfect ciblent 25 mods exclusifs (poids 0, niveau 72, id du jeu), que
l'import reporte comme tous les mods cibles d'Essence. Prix : poe.ninja catégorie « Essences »
(`greater-essence-of-…`, `perfect-essence-of-…`) ; Greater Essence of the Mind et Perfect Essence of
Thawing n'y figurent pas et gardent un prix saisi à la main.
Le même jour, les Lesser et normales ont été réalignées sur ces sources : 9 manquantes ajoutées
(Alacrity, Command, Enhancement, Grounding, Opulence, Thawing ; Lesser Command, Electricity,
Enhancement), cibles corrigées (Haste ne visait aucune épée/hache/masse, Seeking ignorait les armes
martiales, Mind ignorait anneaux et amulettes, Body donnait trop aux bottes/gants), mods sur mesure
remplacés par les vrais mods du jeu aux mêmes valeurs (Battle, Haste, Sorcery, Opulence), the Infinite
retirée. Bilan : 18 familles × 4 tiers = 72 Essences, plus 6 Liquid Emotions et 13 Alloys = 91 (108 depuis le 05/10/2026 : 23 Liquid Emotions, voir plus bas). Seul écart
restant : Craft of Exile ne donne aucune Essence pour la griffe, le dataset l'y laisse (tag d'arme).

Liquid Emotions et instillation d'amulette (05/10/2026), via `tools/import_liquid_emotions.mjs`
(`node tools/import_liquid_emotions.mjs <LiquidEmotions.lua> <tree.lua> <mods.json RePoE>`). Source : les
fichiers générés depuis le client du jeu par Path of Building PoE2 (`src/Data/LiquidEmotions.lua`, et le
champ `recipe` de `src/TreeData/0_5/tree.lua`), recoupés avec poe2db (les 26 pages d'émotions, et les
recettes de Fast Acting Toxins et Splinters). Le jeu compte 26 émotions : 10 de base, leurs 10 versions
« Ancient » (joyaux Time-Lost seulement), 3 « Potent » et leurs 3 « Ancient ». Les 26 sont importées
comme Essences sur joyau Rare (« retire un mod au hasard, ajoute un mod Crafted garanti »), 84 cibles au
total. Les joyaux Time-Lost (Ruby, Emerald, Sapphire, Diamond) sont importés comme bases. Le Diamond ne reçoit
que Concentrated Liquid Isolation et les Potent : sa cible vient en premier (`item_tags` « a&b&c » = tous
ces tags), `mod_id` vide = émotion inapplicable. 16 mods « Crafted » à poids nul (ex. +1% Maximum Chaos
Resistance, Upgrades Radius to Very Large) sont ajoutés et reportés par l'import RePoE comme toute cible
d'Essence. Potent Liquid Ferocity, Potent Liquid Contempt et Ancient Potent Liquid Contempt proposent un
préfixe OU un suffixe (infobulle du jeu « Ruby Prefix: … / Ruby Suffix: … ») : `mod_id` = le préfixe,
`alt_mod_id` = le suffixe, ajoutés à 50/50 parmi ceux qui ont la place après le retrait (le 50/50 vient de
Max, 2026-10-05 ; le jeu ne l'écrit pas). Les mods « +1 Suffix/Prefix Modifier allowed » de Contempt portent
`suffix_cap_delta` / `prefix_cap_delta` (stats `local_maximum_*_allowed_+` de RePoE) : tant qu'ils sont sur
l'objet, le plafond s'élargit ; retirés, les affixes en trop restent mais plus rien n'entre dans ce slot.
Non modélisé :
le `tierLevel` de Path of Building (69 Fear, 73 Suffering, 77 Isolation, 65 Potent), sens non documenté.
`instills` : 875 recettes (passif, ses stats, trois `price_id` dans l'ordre du jeu ; l'ordre compte : sans lui, il
n'y aurait que 233 combinaisons distinctes). Le planificateur l'ajoute en étape finale sur
une amulette : coût fixe payé une fois, inclus dans la liste de courses, pas dans le coût du craft.
Prix : poe.ninja, catégorie « Delirium » (`diluted-liquid-ire`, `ancient-liquid-envy`…), pour les 26.

`tools/import_repoe.mjs` (Node, pas de dépendance en plus) est appelé automatiquement par
`update-dataset.bat` ; utilisable seul si besoin : `node tools/import_repoe.mjs <mods> <base_items> -o data/sample/dataset.json`.

## Correctif de convergence du solveur (22/09/2026)

Sur le dataset réel (27 groupes d'exclusion par base contre ~10-20 dans le jeu de test synthétique), un
objectif à 4 affixes n'atteignait pas la convergence dans les 200 000 balayages autorisés par défaut —
alors que chaque balayage ne coûte que ~20 µs, donc le budget de *temps* (30 s) était très loin d'être
épuisé quand le plafond de *balayages* coupait le calcul. Le nombre affiché était alors faux de près de
moitié (contrôle par visites et coût espéré divergeaient du simple au double).

Correctif dans `crates/craft-solver/src/solve.rs` (`SolveConfig::default`) :
- `max_sweeps` : 200 000 → 50 000 000 (le budget de *temps* redevient la seule limite réelle)
- `max_millis` : 30 000 → 45 000 (marge empirique : ce cas précis converge à 1 509 275 balayages / 35,8 s)

Vérifié : 27 tests toujours au vert, le cas qui échouait converge maintenant et le contrôle d'intégrité
(coût recalculé par les visites vs coût du solveur) concorde à 0,0006 % près.

**Non résolu** : la vérification Monte-Carlo (20 000 essais sur le moteur exact) reste lente sur ce
dataset plus riche — plusieurs dizaines de secondes pour un objectif à 4 affixes, contre quelques
secondes sur le jeu de données d'exemple. Ce n'est plus un problème de justesse (le résultat affiché est
correct), seulement de temps d'attente pour la vérification. À optimiser séparément (ex. réduire le
nombre d'essais par défaut au-delà d'une certaine taille de pool, ou accélérer `AffixPool::draw`).
