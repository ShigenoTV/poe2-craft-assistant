// Conversion d'affichage Exalted → Divine (src/lib/money.ts). Lancer : npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { toDisplay, inputToExalted, exaltedToInput, isCostUnit } from "../src/lib/money.ts";

const DIV = 470.6; // 1 Divine en Exalted (réponse poe.ninja réelle de crates/craft-data/tests/fixtures/ninja_currency.json)

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

test("seules « ex » et « div » sont des unités valides", () => {
  assert.ok(isCostUnit("ex") && isCostUnit("div"));
  assert.ok(!isCostUnit("chaos") && !isCostUnit(undefined));
});
