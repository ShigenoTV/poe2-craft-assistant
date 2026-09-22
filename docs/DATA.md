# Données de jeu

Tout le moteur lit un fichier JSON unique (schéma `1`). Le fichier embarqué, `data/sample/dataset.json`, est
**illustratif** : poids, niveaux de mods et prix sont inventés (généré par `tools/build_sample_dataset.py`).
Pour de vrais calculs, importe un jeu de données réel depuis l'écran « Données » (bouton « Importer un fichier »).

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


## Import réel (repoe-fork.github.io/poe2) — session du 22/09/2026

`tools/import_repoe.py` convertit un vrai export du jeu en dataset compatible :

```
python3 tools/import_repoe.py mods.min.json base_items.min.json -o data/poe2/dataset.json
```

Les deux fichiers source se téléchargent à la main depuis un navigateur (le site est généré par une
action GitHub, pas stocké en clair dans un dépôt, donc aucun outil automatique ne peut les récupérer) :
- https://repoe-fork.github.io/poe2/mods.min.json
- https://repoe-fork.github.io/poe2/base_items.min.json

Résultat sur l'export testé (version 4.5.5.2) : 53 bases, 1655 affixes réels, chargés sans erreur.
Vérifié avec la vraie base `gloves_str` : coût espéré et contrôle par visites concordent à 0,0006 %
près une fois le correctif de convergence (voir plus bas) appliqué.

**Limite connue** : le groupe d'exclusion mécanique du jeu (`groups[0]` dans `mods.json`) sert aussi de
famille de tiers dans l'interface. Sur ~155 groupes sur 383 (avant filtrage), plusieurs stats différentes
s'excluent mutuellement sous un même groupe (ex. « BaseLocalDefences » = Armure locale OU Évasion locale
OU Énergie Spirituelle locale, jamais deux à la fois). Le texte de chaque tier reste toujours exact ; seul
le nom de famille au-dessus de la barre de tiers peut être générique dans ces cas. Éviter ces groupes
mélangés comme objectif de craft (ex. préférer « FireResistance » à « BaseLocalDefences »).

**Non importé pour l'instant** : Essences (`is_essence_only`), mods de corruption, mods d'objets uniques.
`data/poe2/dataset.json` n'est pas encore le dataset embarqué par défaut : à importer manuellement via
l'écran « Données » → « Importer un fichier ».

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
