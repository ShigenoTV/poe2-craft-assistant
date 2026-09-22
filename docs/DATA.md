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
