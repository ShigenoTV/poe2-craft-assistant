# 2026-10-05 : plafond d'affixes des joyaux (2 préfixes / 2 suffixes)

- Source : Path of Building PoE2, `src/Classes/Item.lua` : `affixLimit` 4 pour un Rare de type `Jewel`
  (hors Abyss corrompu, absent du dataset), 2 pour un Magique. Les Time-Lost sont de type `Jewel`
  (`src/Data/Bases/jewel.lua`, subType Radius). poe2db n'énonce pas la règle en clair.
- Moteur : `AffixPool::rare_cap` (3/3 par défaut) borne le Rare et le Magique avant le décalage d'implicite.
- Dataset : `BaseItem::rare_cap` = [2, 2] sur les 8 joyaux ; posé aussi par `tools/import_repoe.mjs`
  pour survivre à update-dataset.bat.
- Tests : `jewel_rare_cap_is_two_prefixes_two_suffixes` (mc.rs) et
  `solver_respects_the_two_two_cap_of_a_jewel` (craft-api, bout en bout : interface 2/2, 3 préfixes
  refusés, 1re action Annulation sur un joyau plein ; échoue si on remet 3/3).
