#!/usr/bin/env node
// Poids d'apparition réels (estimés) depuis Craft of Exile.
//
// Les données du jeu PoE2 (RePoE, poe2db) donnent un poids de 1 à tous les tiers : GGG ne publie pas les
// vrais poids. Craft of Exile publie des estimations obtenues au recombinateur par la communauté
// Prohibited Library (voir https://www.craftofexile.com/weightings?game=poe2). Ce script les reporte sur
// notre dataset.
//
// Utilisation directe (modifie le dataset sur place) :
//   node tools/coe_weights.mjs <poec_data.json> [data/sample/dataset.json]
// Le fichier se télécharge depuis https://www.craftofexile.com/json/poe2/main/poec_data.json
// (préfixé par « poecd= », géré ici). `import_repoe.mjs --coe <fichier>` appelle la même fonction.
//
// Correspondance :
// - Base : chaque base du dataset reçoit `weight_key` = nom de la base Craft of Exile qui lui correspond
//   (classe d'objet, archétype d'attribut pour les armures). Pas d'équivalent (armures str/dex/int, joyau
//   prismatique, piège) ou aucun poids connu sur cette base chez Craft of Exile (ils mettent 1 partout,
//   ex. Griffe, Dague, Épée) : pas de `weight_key`, la base reste aux poids du jeu (tous équiprobables).
// - Mod : même groupe d'exclusion, même slot, même texte (nombres → #) et même niveau requis que le tier
//   Craft of Exile. `weights` = { "*": poids le plus courant, "<weight_key>": exception, ... }.
// Chaque mod normal tirable sur une base à `weight_key` doit trouver son poids : sinon le script échoue,
// pour ne jamais mélanger un poids 1 du jeu avec des poids Craft of Exile (~1000) sur une même base.
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const ARCH = { str_armour: "STR", dex_armour: "DEX", int_armour: "INT", str_dex_armour: "STR/DEX", str_int_armour: "STR/INT", dex_int_armour: "DEX/INT" };
const SAME_NAME = new Set([
  "Amulet", "Ring", "Belt", "Quiver", "Focus", "Wand", "Staff", "Sceptre", "Talisman", "Spear", "Flail", "Claw", "Dagger",
  "Bow", "Crossbow", "Warstaff", "One Hand Axe", "One Hand Mace", "One Hand Sword", "Two Hand Axe", "Two Hand Mace", "Two Hand Sword",
]);
const JEWEL = { dexjewel: "Emerald", intjewel: "Sapphire", strjewel: "Ruby" };

export function parseCoe(text) {
  return JSON.parse(text.replace(/^\s*poecd\s*=\s*/, "").replace(/;\s*$/, ""));
}

function coeBaseName(b) {
  if (SAME_NAME.has(b.item_class)) return b.item_class;
  if (b.item_class === "Buckler") return "Shield (DEX)";
  if (b.item_class === "Jewel") {
    const j = b.tags.filter((t) => JEWEL[t]);
    return j.length === 1 ? JEWEL[j[0]] : null;
  }
  const a = b.tags.find((t) => ARCH[t]);
  return a ? `${b.item_class} (${ARCH[a]})` : null;
}

// texte comparable : lignes d'un hybride jointes par « , » (convention Craft of Exile), nombres → #
const norm = (t) => t.replace(/\n/g, ", ").replace(/\(-?\d+(?:\.\d+)?--?\d+(?:\.\d+)?\)|-?\d+(?:\.\d+)?/g, "#").replace(/\s+/g, " ").trim().toLowerCase();

const spawnsOn = (m, b) => {
  const r = m.spawn.find((s) => s.tag === "default" || b.tags.includes(s.tag));
  return !!r && r.weight > 0;
};

/** Ajoute `weight_key` aux bases et `weights` aux mods de `ds` (modifié sur place). */
export function applyCoeWeights(ds, coe) {
  const coeBaseId = new Map(coe.bases.seq.map((b) => [b.name_base, b.id_base]));
  const byText = new Map();
  for (const m of coe.modifiers.seq) {
    if (m.affix !== "prefix" && m.affix !== "suffix") continue;
    const g = JSON.parse(m.modgroups || "[]")[0];
    const k = `${g}|${m.affix}|${norm(m.name_modifier)}`;
    byText.set(k, [...(byText.get(k) ?? []), m.id_modifier]);
  }
  const lookup = (m, coeId) => {
    for (const id of byText.get(`${m.group}|${m.slot}|${norm(m.text)}`) ?? []) {
      const t = coe.tiers[id]?.[coeId]?.find((t) => +t.ilvl === m.level);
      if (t) return +t.weighting;
    }
    return null;
  };

  const normal = ds.mods.filter((m) => !m.desecrated);
  const perMod = new Map(); // mod id -> Map(weight_key -> poids)
  const keyed = [];
  const skipped = [];
  for (const b of ds.bases) {
    delete b.weight_key;
    const name = coeBaseName(b);
    const coeId = name && coeBaseId.get(name);
    if (!coeId) { skipped.push(`${b.id} (pas d'équivalent)`); continue; }
    const found = new Map();
    const missing = [];
    for (const m of normal) {
      if (!spawnsOn(m, b)) continue;
      const w = lookup(m, coeId);
      if (w == null) missing.push(m.id);
      else found.set(m.id, w);
    }
    // Craft of Exile met 1 sur tous les tiers des bases dont il ne connaît pas les poids
    if (found.size === 0 || [...found.values()].every((w) => w <= 1)) { skipped.push(`${b.id} (aucun poids connu chez Craft of Exile)`); continue; }
    if (missing.length) throw new Error(`${b.id} : ${missing.length} mods sans poids Craft of Exile (ex. ${missing.slice(0, 5).join(", ")})`);
    const low = [...found].filter(([, w]) => w <= 0);
    if (low.length) throw new Error(`${b.id} : poids nul chez Craft of Exile pour un mod tirable selon le jeu (ex. ${low.slice(0, 5).map(([id]) => id).join(", ")})`);
    b.weight_key = name;
    keyed.push(b.id);
    for (const [id, w] of found) {
      const per = perMod.get(id) ?? new Map();
      if (per.has(name) && per.get(name) !== w) throw new Error(`${id} : deux poids pour ${name}`);
      per.set(name, w);
      perMod.set(id, per);
    }
  }
  for (const m of ds.mods) {
    delete m.weights;
    const per = perMod.get(m.id);
    if (!per) continue;
    const counts = new Map();
    for (const w of per.values()) counts.set(w, (counts.get(w) ?? 0) + 1);
    const def = [...counts].sort((a, b) => b[1] - a[1] || b[0] - a[0])[0][0];
    m.weights = { "*": def };
    for (const [k, w] of [...per].sort()) if (w !== def) m.weights[k] = w;
  }
  return { keyed, skipped, mods: perMod.size };
}

function main() {
  const [coeFile, dsFile = "data/sample/dataset.json"] = process.argv.slice(2);
  if (!coeFile) {
    console.error("usage: node tools/coe_weights.mjs <poec_data.json> [data/sample/dataset.json]");
    process.exit(2);
  }
  const ds = JSON.parse(readFileSync(dsFile, "utf8"));
  const r = applyCoeWeights(ds, parseCoe(readFileSync(coeFile, "utf8")));
  ds.meta.weights_source = `craftofexile.com (poec_data.json, ${new Date().toISOString().slice(0, 10)})`;
  writeFileSync(dsFile, JSON.stringify(ds));
  report(r);
}

export function report(r) {
  console.log(`  poids Craft of Exile : ${r.mods} mods, ${r.keyed.length} bases`);
  if (r.skipped.length) console.log(`  restent aux poids du jeu (tous égaux) : ${r.skipped.join(", ")}`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main();
