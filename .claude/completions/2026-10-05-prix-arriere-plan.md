# 2026-10-05 : prix poe.ninja en arrière-plan + date par prix

- Avant : une seule actualisation silencieuse au démarrage (si prix > 1 h) + bouton manuel dans Réglages.
- `prices::spawn_background_refresh` (src-tauri) : thread dédié, vérifie chaque minute ; actualise si
  `refresh_due` (intervalle `Settings::price_refresh_minutes`, défaut 60, plancher 15). Réglage relu à
  chaque tour.
- Échec réseau : prix précédents inchangés, erreur gardée dans `AppState::price_error` (affichée), nouvel
  essai 10 min plus tard (`RETRY_AFTER_ERROR_SECS`).
- `MarketPrices::updated_at` (serde default, anciens fichiers -> `fetched_at`) ; `merge` garde, sur la même
  ligue, la dernière valeur et la date d'un prix absent du nouveau relevé.
- `AppState::price_fetch` sérialise bouton et arrière-plan. Les prix saisis à la main restent prioritaires
  (`AppState::prices`), jamais écrits par l'actualisation.
- UI : colonne « Mis à jour » (il y a N min, date complète au survol), intervalle réglable, « Prochaine : dans N min ».
- Tests src-tauri (`cargo test --lib` dans src-tauri, nécessite libgtk-3-dev/libwebkit2gtk-4.1-dev sous Linux) :
  schedule_tests (3) + live_tests (refresh conserve prix manuels et anciens prix après échec). Le serveur de
  test renvoie désormais une réponse sans ligne correspondante pour Delirium/Essences (aucun relevé réel
  enregistré, poe.ninja injoignable depuis le cloud) : le test réseau échouait déjà sur main depuis l'ajout
  de ces catégories.
