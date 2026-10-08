# 2026-10-08 : historique des crafts (coût prévu contre coût réel)

- `craft-api/src/live.rs` : chaque étape du suivi porte une `Spend` (monnaie, libellé, coût au prix du plan).
  `advised_spend` = monnaie conseillée juste avant la saisie (comptée par défaut), `new_base_spend` = rachat
  d'une base (`abandon_extra` = prix de la base moins revente, comme le solveur), `spend_of(id)`. Annuler retire
  la dépense ; au-delà de 200 étapes, les dépenses sorties de l'historique restent comptées (`dropped`).
  `planned_cost` figé au démarrage du suivi (`start_from` : coût espéré depuis l'objet de départ).
  `LiveView` expose plannedCost, spent, lastSpend, spendChoices.
- `craft-api/src/history.rs` : `CraftRecord`, `History` (500 fiches au plus, plus récente en tête),
  `finish_record` (réussi = objectif atteint sur l'objet courant).
- src-tauri : `live_edit` et la copie d'objet comptent la monnaie conseillée, `live_reset` compte une base neuve,
  `live_set_spend` corrige la dernière saisie, `live_finish` enregistre puis repart d'une base neuve,
  `history_list`/`history_delete` ; stockage `craft-history.json` dans le dossier de données ; événement `history-updated`.
- UI : overlay (prévu/dépensé, sélecteur « Dernière saisie comptée », bouton Enregistrer), page « Historique »
  (écart calculé sur les crafts réussis seulement). Coûts via `Cost`/`useCost` (Exalted ou Divine).
- Tests : history::a_tracked_craft_records_planned_against_real_cost (bout en bout),
  history_keeps_the_most_recent_records, live::spends_survive_the_history_limit, gapOf dans tools/money.test.mjs.
