# 2026-10-09 : vitesse du solveur (policy iteration + cache par structure)

- Cause mesurée : la value iteration seule progresse d'environ une « tentative » par passe ; quand
  l'objectif est rare (gants Vie/Feu/Froid/Précision T3 : 6 511 états, 1,4 M transitions, ~1 400 ex),
  il faudrait des centaines de milliers de passes. À 45 s elle s'arrêtait non convergée avec un résultat
  faux (849 ex annoncés, moteur exact incohérent). Les visites espérées (plan, liste de courses)
  souffraient du même mal (itération plafonnée à 8 s).
- `craft-solver/src/linsolve.rs` : systèmes creux `I − P`, GMRES redémarré + préconditionneur ILU(0).
- `solve.rs` : policy iteration avant la value iteration (politique initiale propre par distance au but,
  évaluation exacte par GMRES, amélioration seulement pour un gain net > 1e-9 relatif). La value
  iteration d'origine tourne ensuite et confirme la convergence avec le même critère, puis la même
  extraction de politique. Échec (temps, GMRES) → la value iteration seule fait le travail comme avant.
  `SolveConfig::policy_iteration = false` redonne l'ancienne méthode (référence des tests).
- `plan.rs::expected_visits` : (I − Pᵀ) x = e_départ résolu par GMRES, itération gardée en secours.
- Cache `SolveCache::global()` (8 entrées) : graphe énuméré + dernière politique par
  `Model::structure_key` (pool, objectif, ilvl, actions sans prix, remplisseurs écartés, départ). Un
  changement de prix reprend le graphe (coût d'abandon recalculé) et repart de l'ancienne politique.
  Utilisé par `build_context`, `advise_item`, `compare_paths`.
- Temps (release, 4 cœurs, 20 000 essais Monte-Carlo) : 3 mods T3 : 45,3 s → 2,7 s au total (résolution
  5,3 s → 0,3 s, comparateur 39,4 s → 1,8 s). 4 mods T3 : 211 s et résultat faux → 16,8 s (résolution
  0,99 s ; le reste est la vérification Monte-Carlo, inchangée).
- Preuve : `solver_speed_tests` (craft-api) : 6 objectifs réels, ancienne vs nouvelle méthode → mêmes
  états, valeurs à < 1e-6 (écart max observé 3e-9 = imprécision de l'ancienne), 0 action différente,
  mêmes nœuds de plan et liste de courses ; gants 4 mods T3 convergés et confirmés par le moteur exact ;
  changement de prix via cache = calcul complet (5 évaluations au lieu de 11).
- CLI : `craft-cli solve ... --compare` chronomètre le comparateur.
