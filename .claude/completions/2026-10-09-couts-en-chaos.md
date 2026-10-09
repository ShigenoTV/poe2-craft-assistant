# Coûts affichés en Chaos (2026-10-09)

- Sélecteur « Exalted | Chaos | Divine » (`src/lib/display.tsx`), `CostUnit` = "ex" | "chaos" | "div" (`src/lib/money.ts`).
- Taux : prix `chaos` du dataset (price_sources Currency/chaos, rafraîchi par poe.ninja, défaut 56,4 ex), comme la Divine.
- Conversion générique `toDisplay(ex, unit, rate)` + `rateOf(unit, rates)` ; sous 1 unité le montant reste en ex.
- Tests : `tools/money.test.mjs` (dont bout en bout dataset → affichage), `crates/craft-data/src/prices.rs`, `src-tauri/src/state.rs`.
