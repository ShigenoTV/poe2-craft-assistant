# 2026-10-05 : suivi de craft en direct dans l'overlay

- `craft-api/src/live.rs` : `LiveSession` (objet suivi + historique, 200 étapes au plus) et `LiveEdit`
  (add, remove, replace, rarity, fracture). Une liste d'éditions = une étape annulable ; une saisie refusée
  ne change rien. Ajout sur Normal → Magique, ajout qui déborde un Magique → Rare (un clic après un Regal).
  Contrôles : groupe déjà pris, une seule Désécration, 6 affixes, plafond de la rareté (`cap_of`), une
  seule fracture. `live_view` recalcule le conseil via `advise_item` (résolution à la volée, cache `extra`).
- src-tauri : `AppState::live`, commandes `live_state`/`live_edit`/`live_undo`/`live_reset`, événement
  `live-updated`. Une copie d'objet de la base du plan remplace l'objet suivi (étape annulable). Activer
  ou effacer un plan remet le suivi à zéro et émet `plan-refreshed`. Chaque saisie met à jour `last_item`
  (le recalcul après actualisation des prix repart donc de l'objet saisi).
- UI : `src/overlay/LivePanel.tsx` remplace le conseil de l'overlay quand un plan est actif (rareté,
  tiers ▲▼, ◆ fracture, ⇄ remplacer, ✕ retirer, + préfixe/suffixe avec recherche, voulus ★ en tête,
  « non voulu quelconque »). Saisie seulement en mode interactif. Mode navigateur : `overlay.html?live`.
- Tests : live_tracking_recomputes_the_next_step_after_each_input, live_tracking_promotes_a_full_magic_item_to_rare,
  live_edits_reject_impossible_items_without_changing_anything.
