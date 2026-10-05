# 2026-10-05 : Potent Liquid Ferocity, Potent Liquid Contempt, Ancient Potent Liquid Contempt

- Sources : infobulles du jeu (captures de Max), poe2db, Path of Building PoE2 (LiquidEmotions.lua),
  RePoE (mods CraftedJewel*). Règle donnée par Max (non écrite dans le jeu) : slot tiré à 50/50, puis le
  mod remplace un affixe non fracturé de ce slot.
- Données : `EssenceTarget::alt_mod_id`, `ModDef::prefix_cap_delta/suffix_cap_delta` ; 26/26 émotions,
  84 cibles + 12 secondes, 16 mods Crafted. Import : tools/import_liquid_emotions.mjs.
- Moteur : `Affix::cap_shift`, `AffixPool::cap_of(item)`/`has_room`, `Currency::alt_target`. Essence sans
  place après retrait : l'objet reste tel quel (avant : objet modifié en moteur exact, probabilité perdue
  dans le solveur, qui la comptait comme un succès gratuit).
- Solveur : `MacroState::shifter`, plafonds par état, retrait du mod décaleur, `Goal::with_extra`.
- Interface : PoolView `extraPrefixes/extraSuffixes/extraVia`, GoalPicker autorise la place en plus.
- Tests : mc.rs `contempt_adds_one_of_two_cap_shifting_mods`, craft-api
  `potent_liquid_contempt_opens_a_third_suffix_on_a_jewel` (solveur 3390 vs moteur exact 3400),
  `solver_buys_potent_liquid_ferocity_for_its_prefix`, `pool_view_announces_the_place_opened_by_contempt`.
