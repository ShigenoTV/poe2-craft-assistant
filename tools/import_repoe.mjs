#!/usr/bin/env node
// Importe un vrai jeu de données PoE2 depuis l'export RePoE (repoe-fork.github.io/poe2).
//
// D'habitude tu n'as pas besoin d'appeler ce fichier toi-même : `update-dataset.bat` télécharge les
// sources et l'appelle pour toi. Utilisation directe :
//   node tools/import_repoe.mjs <mods.min.json> <base_items.min.json> [-o data/sample/dataset.json]
//
// Ce que fait l'import, et pourquoi :
// - Mods retenus : domain == "item" (exclut monstres/zones/coffres), generation_type in
//   {prefix, suffix} (exclut les mods d'objets uniques et les mods de corruption, qui ne sont pas du
//   craft « normal »), is_essence_only == false (ces mods n'apparaissent que via une Essence, jamais par
//   tirage classique — les inclure fausserait les poids).
// - Groupe d'exclusion (« deux affixes de ce groupe ne coexistent jamais ») = le CHAMP BRUT `groups[0]`
//   du jeu. C'est la seule source de vérité mécanique : la famille affichée dans l'interface (utilisée
//   pour le sélecteur de tiers) s'appuie dessus mais peut être plus fine si `type` varie au sein d'un
//   même groupe (~155 groupes sur 383 mélangent plusieurs stats qui s'excluent mutuellement mais ne sont
//   pas des tiers d'un même affixe, ex. BaseLocalDefences = Armure locale OU Évasion locale OU Énergie
//   Spirituelle locale, un seul à la fois). Le texte affiché à chaque tier reste toujours exact ; seul
//   le NOM de famille au-dessus de la barre de tiers peut être générique dans ces cas-là.
// - Bases retenues : équipement seulement (armures, armes, bijoux, carquois/bouclier/focus), release_state
//   == "released", domain == "item". Pour les 4 classes d'armure principales + Shield, chaque archétype
//   d'attribut (str/dex/int et hybrides) devient une base séparée ; pour le reste, un seul représentant
//   par classe. Dans chaque groupe, on garde la variante au plus haut drop_level (l'équivalent « fin de
//   jeu ») : c'est elle qui compte pour un craft à haut niveau d'objet.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

const BRACKET = /\[([^\]|]+)(?:\|([^\]]+))?\]/g;
const clean = (t) => t.replace(BRACKET, (_, a, b) => b ?? a);
const prettify = (key) => key.replace(/(?<!^)(?=[A-Z])/g, " ").replace(/\s+/g, " ").trim();

const ARCHETYPE_TAGS = new Set([
  "str_armour", "dex_armour", "int_armour", "str_dex_armour", "str_int_armour", "dex_int_armour", "str_dex_int_armour",
  "strjewel", "dexjewel", "intjewel",
]);
const ARCHETYPE_LABEL = {
  str_armour: "Armour", dex_armour: "Evasion", int_armour: "Energy Shield",
  str_dex_armour: "Armour/Evasion", str_int_armour: "Armour/ES", dex_int_armour: "Evasion/ES",
  str_dex_int_armour: "Armour/Evasion/ES",
  strjewel: "Str", dexjewel: "Dex", intjewel: "Int",
};
const ARCHETYPE_SPLIT_CLASSES = new Set(["Gloves", "Boots", "Body Armour", "Helmet", "Shield", "Jewel"]);
// domaine "item" : affixes normaux d'équipement. "misc" : affixes de joyaux (jamais rangés sous "item"
// dans les données du jeu, même si le mécanisme de craft — tirage pondéré par tag de base — est identique).
const ALLOWED_MOD_DOMAINS = new Set(["item", "misc"]);
const EQUIP_CLASSES = new Set([
  "Gloves", "Boots", "Body Armour", "Helmet", "Shield", "Buckler", "Focus",
  "Amulet", "Ring", "Belt", "Quiver", "Jewel",
  "Claw", "Dagger", "Wand", "One Hand Sword", "One Hand Axe", "One Hand Mace",
  "Bow", "Staff", "Two Hand Sword", "Two Hand Axe", "Two Hand Mace",
  "Sceptre", "Spear", "Flail", "Warstaff", "Crossbow", "Talisman", "TrapTool",
]);
const CLASS_TO_ID = {
  Gloves: "gloves", Boots: "boots", "Body Armour": "body_armour", Helmet: "helmet", Shield: "shield",
  Buckler: "buckler", Focus: "focus", Amulet: "amulet", Ring: "ring", Belt: "belt", Quiver: "quiver", Jewel: "jewel",
  Claw: "claw", Dagger: "dagger", Wand: "wand", "One Hand Sword": "sword_1h", "One Hand Axe": "axe_1h",
  "One Hand Mace": "mace_1h", Bow: "bow", Staff: "staff", "Two Hand Sword": "sword_2h", "Two Hand Axe": "axe_2h",
  "Two Hand Mace": "mace_2h", Sceptre: "sceptre", Spear: "spear", Flail: "flail", Warstaff: "warstaff",
  Crossbow: "crossbow", Talisman: "talisman", TrapTool: "trap",
};

function importMods(mods) {
  const craft = Object.entries(mods).filter(
    ([, v]) => ALLOWED_MOD_DOMAINS.has(v.domain) && (v.generation_type === "prefix" || v.generation_type === "suffix") && !v.is_essence_only,
  );
  const typeByGroup = new Map();
  for (const [, v] of craft) {
    const g = (v.groups ?? [null])[0];
    if (!g) continue;
    const t = v.type || g;
    const counts = typeByGroup.get(g) ?? new Map();
    counts.set(t, (counts.get(t) ?? 0) + 1);
    typeByGroup.set(g, counts);
  }
  const familyLabel = new Map();
  for (const [g, counts] of typeByGroup) {
    const top = [...counts.entries()].sort((a, b) => b[1] - a[1])[0][0];
    familyLabel.set(g, prettify(top));
  }

  const out = [];
  for (const [modId, v] of craft) {
    const g = (v.groups ?? [null])[0];
    if (!g || !v.stats?.length || !v.text) continue;
    const spawn = (v.spawn_weights ?? []).map((s) => ({ tag: s.tag, weight: s.weight }));
    if (!spawn.some((s) => s.tag !== "default" && s.weight > 0)) continue;
    out.push({
      id: modId,
      group: g,
      family: familyLabel.get(g) ?? prettify(g),
      name: v.name || modId,
      slot: v.generation_type,
      level: v.required_level ?? 1,
      text: clean(v.text),
      tags: (v.implicit_tags ?? []).slice(0, 8),
      spawn,
    });
  }
  return out;
}

function importBases(items) {
  const eq = Object.values(items).filter((v) => v.release_state === "released" && ALLOWED_MOD_DOMAINS.has(v.domain) && EQUIP_CLASSES.has(v.item_class));
  const byKey = new Map();
  for (const v of eq) {
    const cls = v.item_class;
    let key;
    if (ARCHETYPE_SPLIT_CLASSES.has(cls)) {
      const arch = [...new Set(v.tags ?? [])].filter((t) => ARCHETYPE_TAGS.has(t)).sort();
      if (arch.length === 0) continue;
      key = `${cls}\u0000${arch.join(",")}`;
    } else {
      key = `${cls}\u0000`;
    }
    if (!byKey.has(key)) byKey.set(key, []);
    byKey.get(key).push(v);
  }
  const bases = [];
  for (const [key, variants] of byKey) {
    const [cls, archStr] = key.split("\u0000");
    const arch = archStr ? archStr.split(",") : [];
    const rep = variants.reduce((a, b) => ((b.drop_level ?? 0) > (a.drop_level ?? 0) ? b : a));
    const baseId = CLASS_TO_ID[cls] + (arch.length ? "_" + arch.map((a) => a.replace("_armour", "")).join("_") : "");
    const label = arch.length === 1 ? (ARCHETYPE_LABEL[arch[0]] ?? "") : arch.length ? arch.map((a) => ARCHETYPE_LABEL[a] ?? a).join("/") : "";
    // les joyaux ont un vrai nom canonique bien connu (Ruby/Sapphire/Emerald/Diamond) — on le garde tel
    // quel plutôt que de reconstruire un nom générique comme pour les autres classes.
    const name = cls === "Jewel" ? rep.name : label ? `${label} ${cls}`.trim() : cls;
    bases.push({ id: baseId, name, item_class: cls, tags: rep.tags ?? [], implicit: null });
  }
  bases.sort((a, b) => (a.item_class + a.id).localeCompare(b.item_class + b.id));
  return bases;
}

function parseArgs(argv) {
  const pos = [];
  let out = "data/sample/dataset.json";
  let carry = "data/sample/dataset.json";
  let indexFile = null;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "-o" || argv[i] === "--out") out = argv[++i];
    else if (argv[i] === "--carry-prices-from") carry = argv[++i];
    else if (argv[i] === "--index") indexFile = argv[++i];
    else pos.push(argv[i]);
  }
  if (pos.length < 2) {
    console.error("usage: node tools/import_repoe.mjs <mods.min.json> <base_items.min.json> [-o data/sample/dataset.json] [--index index.html]");
    process.exit(2);
  }
  return { modsFile: pos[0], baseItemsFile: pos[1], out, carry, indexFile };
}

function main() {
  const { modsFile, baseItemsFile, out, carry, indexFile } = parseArgs(process.argv.slice(2));
  const mods = JSON.parse(readFileSync(modsFile, "utf8"));
  const items = JSON.parse(readFileSync(baseItemsFile, "utf8"));
  const outMods = importMods(mods);
  const outBases = importBases(items);

  // le titre de la page d'accueil de RePoE contient son propre numéro de version, ex.
  // « RePoE - PoE2 version 4.5.5.2 » — à distinguer du nom de patch public du jeu (voir docs/DATA.md).
  let gameVersion = "inconnue (page d'accueil non fournie à l'import — voir https://repoe-fork.github.io/poe2/)";
  if (indexFile) {
    try {
      const html = readFileSync(indexFile, "utf8");
      const m = html.match(/PoE2\s+version\s+([\d.]+)/i);
      if (m) gameVersion = m[1];
      else console.error(`! numéro de version introuvable dans ${indexFile} (page RePoE modifiée ?)`);
    } catch {
      console.error(`! ${indexFile} introuvable : version RePoE inconnue`);
    }
  }

  let carried = {};
  try {
    carried = JSON.parse(readFileSync(carry, "utf8"));
  } catch {
    console.error(`! ${carry} introuvable : currencies/omens/prices/mods « desecrated » seront vides (à compléter à la main)`);
  }

  // les mods du domaine `desecrated` n'apparaissent JAMAIS dans un import brut (`importMods` les exclut
  // volontairement, voir plus haut) : ceux déjà présents dans le dataset précédent sont donc à la main
  // et doivent être reportés, sous peine de perdre silencieusement la Désécration à chaque régénération.
  const carriedDesecrated = (carried.mods ?? []).filter((m) => m.desecrated);
  const knownIds = new Set(outMods.map((m) => m.id));
  for (const m of carriedDesecrated) {
    if (!knownIds.has(m.id)) outMods.push(m);
  }

  // seuls les tags RÉFÉRENCÉS PAR AU MOINS UN MOD comptent : c'est la seule chose que lit le bitmask
  // (`Affix.tags`) construit à partir d'eux. Les tags de BASE (item_class, archétype...) sont filtrés
  // et comparés en texte brut ailleurs, jamais via ce bitmask — les y inclure ne fait que gâcher les
  // 64 bits disponibles (`tags: u64`, un bit par tag) pour rien.
  const usedTags = [...new Set(outMods.flatMap((m) => m.tags))].sort().slice(0, 64);
  const dataset = {
    meta: {
      schema: 1,
      source: "repoe-fork.github.io/poe2 (mods.min.json + base_items.min.json)",
      game_version: gameVersion,
      generated_at: new Date().toISOString().slice(0, 10),
      notice: "Données réelles du jeu (poids de spawn, niveaux, tiers). Les prix restent ceux de poe.ninja/l'onglet Réglages.",
      price_unit: "Exalted Orb",
    },
    tags: usedTags,
    bases: outBases,
    mods: outMods,
    currencies: carried.currencies ?? [],
    essences: carried.essences ?? [],
    omens: carried.omens ?? [],
    prices: carried.prices ?? {},
    price_sources: carried.price_sources ?? {},
  };

  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify(dataset));

  const famSizes = new Map();
  for (const m of outMods) famSizes.set(m.group, (famSizes.get(m.group) ?? 0) + 1);
  console.log(`→ ${out}`);
  console.log(`  ${outBases.length} bases, ${outMods.length} affixes, ${famSizes.size} groupes d'exclusion`);
  console.log(`  monnaies : ${dataset.currencies.length}, Omens : ${dataset.omens.length}, prix : ${Object.keys(dataset.prices).length}`);
  console.log(`  version RePoE : ${gameVersion}`);
}

main();
