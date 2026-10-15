// Notes de version (tools/changelog.mjs, CHANGELOG.md). Lancer : npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { NEXT, newerThan, notesFor, parseChangelog, stampNext } from "./changelog.mjs";

const SAMPLE = `# Notes de version

Intro ignorée.

## Prochaine version

### Nouveautés

- C

## 0.2.49 (2026-10-20)

- B

## 0.2.48 (2026-10-16)

### Corrections

- A
`;

test("chaque section garde son numéro, sa date et son texte", () => {
  const s = parseChangelog(SAMPLE.replace(/\n/g, "\r\n"));
  assert.deepEqual(s.map((x) => [x.version, x.date]), [[NEXT, null], ["0.2.49", "2026-10-20"], ["0.2.48", "2026-10-16"]]);
  assert.equal(notesFor(SAMPLE, "0.2.48"), "### Corrections\n\n- A");
  assert.equal(notesFor(SAMPLE, "0.2.47"), null);
});

test("la release renomme « Prochaine version » avec le numéro et la date", () => {
  const out = stampNext(SAMPLE, "0.2.50", "2026-10-25");
  assert.equal(notesFor(out, "0.2.50"), "### Nouveautés\n\n- C");
  assert.equal(parseChangelog(out)[1].date, "2026-10-25");
  assert.equal(notesFor(out, NEXT), null);
  assert.match(out, /## Prochaine version\n\n## 0\.2\.50 \(2026-10-25\)\n\n### Nouveautés/); // section vide rouverte, mise en page gardée
  assert.throws(() => stampNext(out, "0.2.51", "2026-10-26"), /aucune note/);
});

test("la release refuse des notes vides ou une version déjà décrite", () => {
  assert.throws(() => stampNext(SAMPLE.replace("- C", ""), "0.2.50", "2026-10-25"), /aucune note/);
  assert.throws(() => stampNext("## 0.2.48\n\n- A\n", "0.2.50", "2026-10-25"), /aucune note/);
  assert.throws(() => stampNext(SAMPLE, "0.2.49", "2026-10-25"), /déjà/);
});

test("Nouveautés : seulement les versions publiées depuis la dernière vue", () => {
  assert.deepEqual(newerThan(SAMPLE, "0.2.47", "0.2.49").map((s) => s.version), ["0.2.49", "0.2.48"]);
  assert.deepEqual(newerThan(SAMPLE, "0.2.48", "0.2.49").map((s) => s.version), ["0.2.49"]);
  assert.deepEqual(newerThan(SAMPLE, "0.2.49", "0.2.49"), []);
  assert.deepEqual(newerThan(SAMPLE, "0.2.10", "0.2.48").map((s) => s.version), ["0.2.48"]); // 10 > 9 : comparaison numérique
});

test("CHANGELOG.md du dépôt : la prochaine release a ses notes", () => {
  const text = readFileSync(new URL("../CHANGELOG.md", import.meta.url), "utf8");
  const version = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version;
  // soit des notes en attente, soit (juste après release.mjs) celles de la version courante
  assert.ok(notesFor(text, NEXT) || notesFor(text, version), "écris les changements sous « ## Prochaine version » dans CHANGELOG.md");
});
