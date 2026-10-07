# 2026-10-07 : coûts affichés en Exalted ou en Divine

- Sélecteur global « Exalted | Divine » dans la barre de gauche (`src/lib/display.tsx` : `useDisplay`, `useCost`,
  `<Cost ex=…/>`, `CostUnitSwitch`). Affichage seulement : solveur, prix et budget stockés restent en Exalted.
- Choix mémorisé dans les réglages (`Settings::cost_unit`, « ex » par défaut, ancien settings.json compatible) ;
  `set_settings` émet `settings-changed` pour que l'overlay suive tout de suite.
- Taux : nouveau prix `divine` du dataset (source poe.ninja Currency/divine, défaut 470,6 ex tiré de la
  réponse réelle `ninja_currency.json`), modifiable à la main dans Réglages > Prix.
- Règles (`src/lib/money.ts`) : en Divine, montant < 1 Divine reste en ex ; sans prix de la Divine, tout reste
  en ex ; survol d'un montant en div = montant en ex ; le champ « J'ai » du budget se saisit dans l'unité choisie.
- Tests : `npm test` (tools/money.test.mjs, 6 tests), craft-data `divine_price_in_exalted_comes_from_the_currency_response`,
  src-tauri `real_http_round_trip…` (Divine relevée par fetch) et `cost_unit_defaults_to_exalted_and_is_kept`.
