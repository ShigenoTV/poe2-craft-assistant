// Conversion d'affichage Exalted → Chaos / Divine (src/lib/money.ts). Lancer : npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { toDisplay, inputToExalted, exaltedToInput, isCostUnit, rateOf, gapOf } from "../src/lib/money.ts";
import { readFileSync } from "node:fs";

const DIV = 470.6; // 1 Divine en Exalted (réponse poe.ninja réelle de crates/craft-data/tests/fixtures/ninja_currency.json)
const CHAOS = 56.4; // 1 Chaos en Exalted (même réponse : 470,6 / 8,35)

test("en Exalted, rien ne change", () => {
  assert.deepEqual(toDisplay(53354, "ex", DIV), { value: 53354, unit: "ex" });
});

test("en Divine, les gros montants sont divisés par le prix de la Divine", () => {
  const d = toDisplay(53354, "div", DIV);
  assert.equal(d.unit, "div");
  assert.ok(Math.abs(d.value - 113.38) < 0.01, String(d.value));
  assert.deepEqual(toDisplay(DIV, "div", DIV), { value: 1, unit: "div" });
});

test("en Divine, un montant sous 1 Divine reste en Exalted", () => {
  assert.deepEqual(toDisplay(304, "div", DIV), { value: 304, unit: "ex" });
  assert.deepEqual(toDisplay(1.53, "div", DIV), { value: 1.53, unit: "ex" });
});

test("sans prix de la Divine, tout reste en Exalted", () => {
  assert.deepEqual(toDisplay(53354, "div", null), { value: 53354, unit: "ex" });
  assert.deepEqual(toDisplay(53354, "div", 0), { value: 53354, unit: "ex" });
  assert.deepEqual(toDisplay(53354, "div", NaN), { value: 53354, unit: "ex" });
});

test("le budget saisi en Divine est converti en Exalted, aller-retour exact", () => {
  assert.equal(inputToExalted(10, "div", DIV), 4706);
  assert.equal(inputToExalted(10, "ex", DIV), 10);
  assert.equal(inputToExalted(10, "div", null), 10);
  assert.ok(Math.abs(exaltedToInput(inputToExalted(2.5, "div", DIV), "div", DIV) - 2.5) < 1e-12);
});

test("en Chaos, les montants sont divisés par le prix du Chaos, sous 1 Chaos ils restent en Exalted", () => {
  const d = toDisplay(53354, "chaos", CHAOS);
  assert.equal(d.unit, "chaos");
  assert.ok(Math.abs(d.value - 945.99) < 0.01, String(d.value));
  assert.deepEqual(toDisplay(CHAOS, "chaos", CHAOS), { value: 1, unit: "chaos" });
  assert.deepEqual(toDisplay(30, "chaos", CHAOS), { value: 30, unit: "ex" });
  assert.deepEqual(toDisplay(53354, "chaos", null), { value: 53354, unit: "ex" });
  assert.equal(inputToExalted(10, "chaos", CHAOS), 564);
  assert.ok(Math.abs(exaltedToInput(inputToExalted(2.5, "chaos", CHAOS), "chaos", CHAOS) - 2.5) < 1e-12);
});

test("chaque unité prend son propre prix", () => {
  const rates = { chaos: CHAOS, div: DIV };
  assert.equal(rateOf("ex", rates), 1);
  assert.equal(rateOf("chaos", rates), CHAOS);
  assert.equal(rateOf("div", rates), DIV);
  assert.equal(rateOf("chaos", { chaos: null, div: DIV }), null);
  assert.equal(rateOf("div", { chaos: CHAOS, div: 0 }), null);
});

test("de bout en bout : prix du dataset embarqué → affichage en Chaos et en Divine", () => {
  // les mêmes prix que lit l'interface (api.getPrices → p.chaos, p.divine), rafraîchis par poe.ninja via price_sources
  const ds = JSON.parse(readFileSync(new URL("../data/sample/dataset.json", import.meta.url), "utf8"));
  assert.equal(ds.price_sources.chaos.ninja_id, "chaos");
  assert.equal(ds.price_sources.divine.ninja_id, "divine");
  const rates = { chaos: ds.prices.chaos, div: ds.prices.divine };
  const ex = 10 * ds.prices.divine;
  assert.deepEqual(toDisplay(ex, "div", rateOf("div", rates)), { value: 10, unit: "div" });
  const c = toDisplay(ex, "chaos", rateOf("chaos", rates));
  assert.equal(c.unit, "chaos");
  assert.ok(Math.abs(c.value - ex / ds.prices.chaos) < 1e-9);
  assert.ok(c.value > 10, "un Chaos vaut moins qu'une Divine");
});

test("seules « ex », « chaos » et « div » sont des unités valides", () => {
  assert.ok(isCostUnit("ex") && isCostUnit("chaos") && isCostUnit("div"));
  assert.ok(!isCostUnit("c") && !isCostUnit(undefined));
});

test("historique : écart du réel au prévu", () => {
  assert.equal(gapOf(200, 300), 0.5);
  assert.equal(gapOf(200, 150), -0.25);
  assert.equal(gapOf(0, 40), null, "sans prévision, pas d'écart");
  // l'écart ne dépend pas de l'unité affichée
  assert.equal(gapOf(200 * DIV, 300 * DIV), gapOf(200, 300));
});
