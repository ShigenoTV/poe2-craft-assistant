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

État au 22/09/2026 : 53 bases, 1655 affixes, 177 groupes d'exclusion. Deux numéros de version coexistent et
ne se correspondent pas terme à terme, à ne pas confondre :
- **version RePoE** (celle qui compte pour savoir si les données sont à jour) : affichée dans le titre de
  https://repoe-fork.github.io/poe2/ (« RePoE - PoE2 version X.Y.Z.W ») — au moment de la génération de ce
  dataset : `4.5.5.2`. Probablement un numéro de build interne du jeu, pas le nom de patch public.
- **nom de patch public du jeu** (communication marketing GGG) : `0.5.5`, « The Forbidden Rites ».

`update-dataset.bat` ne capture aucun des deux automatiquement dans le fichier généré — seulement la date.
Pour savoir si tes données sont à jour, compare le numéro RePoE affiché sur le site au moment de l'import.

Ce que fait l'import, et pourquoi (voir aussi les commentaires en tête de `tools/import_repoe.mjs`) :
- Mods retenus : `domain == "item"`, `generation_type` préfixe ou suffixe, hors mods réservés aux Essences.
- Groupe d'exclusion = le champ brut `groups[0]` du jeu (seule source de vérité mécanique). La famille
  affichée dans l'interface s'appuie dessus mais peut être plus générique sur ~155 groupes qui mélangent
  plusieurs stats mutuellement exclusives (ex. `BaseLocalDefences` = Armure locale OU Évasion locale OU
  Énergie Spirituelle locale). Le texte de chaque tier reste toujours exact.
- Bases retenues : équipement uniquement, `release_state == "released"`. Les 4 classes d'armure
  principales + Shield sont scindées par archétype d'attribut (str/dex/int et hybrides) ; le reste a un
  seul représentant par classe, au plus haut niveau de drop (l'équivalent « fin de jeu »).
- Non importé : Essences, mods de corruption, mods d'objets uniques.

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
