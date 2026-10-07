# 2026-10-07 : comparateur des meilleurs chemins

- `craft-api/src/compare.rs` : `compare_paths(ctx, max_alternatives, trials, cancel, on_progress)`. Part du
  plan optimal de `ctx`, classe ses familles de monnaies par coût espéré (Transmutation, Exaltation, Chaos,
  Essences et Alloys, Désécration, Omens…), résout en parallèle le meilleur plan SANS chacune des 4 plus
  coûteuses (même MDP, mêmes prix, objet de départ reprojeté), écarte les chemins inatteignables ou
  identiques, garde les 2 moins chers, puis vérifie chaque chemin sur le moteur exact (même graine).
  Le moteur exact n'est pas touché.
- `VerifyResult::std_dev` : écart-type du coût des essais réussis.
- src-tauri : `AppState::last_plan` (dernier plan calculé, actif ou non), commande `compare_paths`.
- UI : onglet « Comparer les chemins » (`ComparePaths.tsx`) : coût moyen, médiane, pire cas (99 sur 100),
  écart-type, monnaies principales, repères « le moins cher / le plus régulier / pire cas le plus bas »,
  bouton « Suivre ce chemin » (retire la famille des monnaies autorisées et recalcule).
- Mesure : gants Vie/Feu/Froid T3, monnaies par défaut : plan 3,5 s, comparaison ~26 s de plus
  (optimal 204 ex, sans Désécration 208 ex, sans Augmentation 225 ex).
- Test : compares_the_optimal_plan_with_a_path_without_its_essence.
