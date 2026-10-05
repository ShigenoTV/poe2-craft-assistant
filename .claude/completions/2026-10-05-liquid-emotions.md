# 2026-10-05 — Liquid Emotions complètes et instillation d'amulette

- 26 émotions dans le jeu (Path of Building LiquidEmotions.lua, recoupé poe2db) : 23 importées comme Essences
  sur joyau Rare (72 cibles), 3 omises (Potent Ferocity, Potent Contempt, Ancient Potent Contempt : préfixe OU
  suffixe, choix non documenté).
- Joyaux Time-Lost importés comme bases (cibles des « Ancient »). Diamond exclu sauf Isolation/Potent
  (`item_tags` « a&b&c », `mod_id` vide).
- 875 recettes d'instillation (`instills`), étape finale facultative du plan sur une amulette.
- Prix poe.ninja « Delirium » pour les 26.
- Correctif : l'interface n'envoyait jamais les Essences/Liquid Emotions/Alloys au solveur (absentes de
  `list_actions`) ; elles sont maintenant listées et cochées par défaut.
- Outil : `tools/import_liquid_emotions.mjs` ; `import_repoe.mjs` reporte `instills`.
- Correctif annexe : The Runefather's Alloy visait les tags inexistants « mace_1h »/« mace_2h » (jamais proposé sur une masse) → « mace ».
