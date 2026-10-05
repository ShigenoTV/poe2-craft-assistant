#!/usr/bin/env node
// Importe un vrai jeu de données PoE2 depuis l'export RePoE (repoe-fork.github.io/poe2).
//
// D'habitude tu n'as pas besoin d'appeler ce fichier toi-même : `update-dataset.bat` télécharge les
// sources et l'appelle pour toi. Utilisation directe :
//   node tools/import_repoe.mjs <mods.min.json> <base_items.min.json> [-o data/sample/dataset.json]
//
// Ce que fait l'import, et pourquoi :
// - Mods retenus : domain == "item" (exclut monstres/zones/coffres) ou mod Désécré d'un des trois
//   seigneurs (domain == "desecrated", voir LORDS), generation_type in
//   {prefix, suffix} (exclut les mods d'objets uniques et les mods de corruption, qui ne sont pas du
//   craft « normal »), is_essence_only == false (ces mods n'apparaissent que via une Essence, jamais par
//   tirage classique — les inclure fausserait les poids).
// - Groupe d'exclusion (« deux affixes de ce groupe ne coexistent jamais ») = le CHAMP BRUT `groups[0]`
//   du jeu. C'est la seule source de vérité mécanique. Un groupe peut contenir plusieurs affixes
//   distincts qui s'excluent mutuellement (ex. IncreaseSocketedGemLevel = niveau des sorts, des sorts de
//   feu, de mêlée, des sbires...) : chaque `type` du jeu forme une famille avec ses propres tiers, choisie
//   directement dans l'interface. Libellé de famille = texte majoritaire du (groupe, type), nombres → « # ».
// - Bases retenues : équipement seulement (armures, armes, bijoux, carquois/bouclier/focus), release_state
//   == "released", domain == "item". Pour les 4 classes d'armure principales + Shield, chaque archétype
//   d'attribut (str/dex/int et hybrides) devient une base séparée ; pour le reste, un seul représentant
//   par classe. Dans chaque groupe, on garde la variante au plus haut drop_level (l'équivalent « fin de
//   jeu ») : c'est elle qui compte pour un craft à haut niveau d'objet.
// - Joyaux : Ruby/Emerald/Sapphire/Diamond et leurs versions Time-Lost (tags `*_radius_jewel`), chacun
//   sous son vrai nom.
// - Bijoux (Anneau, Amulette, Ceinture, Carquois) : CHAQUE vraie base est importée séparément avec son
//   implicite propre (texte dans `implicits`), au lieu d'un représentant par classe. Leur pool d'affixes
//   est le même au sein d'une classe (tags ring/amulet/belt/quiver + default), mais l'implicite change
//   l'objet réellement crafté et permet de reconnaître la base exacte d'un objet collé.
//   Les bases dont l'implicite change le nombre de préfixes/suffixes autorisés (stats
//   `local_maximum_prefixes_allowed_+` / `local_maximum_suffixes_allowed_+`, ex. Dusk Ring +1/-1, Absent
//   Amulet -1/-1) portent ce décalage dans `prefix_cap_delta` / `suffix_cap_delta`, appliqué par le
//   moteur et le solveur au plafond de la rareté.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { applyCoeWeights, parseCoe, report as reportWeights } from "./coe_weights.mjs";

const BRACKET = /\[([^\]|]+)(?:\|([^\]]+))?\]/g;
const clean = (t) => t.replace(BRACKET, (_, a, b) => b ?? a);
const prettify = (key) => key.replace(/(?<!^)(?=[A-Z])/g, " ").replace(/\s+/g, " ").trim();

const ARCHETYPE_TAGS = new Set([
  "str_armour", "dex_armour", "int_armour", "str_dex_armour", "str_int_armour", "dex_int_armour", "str_dex_int_armour",
  "strjewel", "dexjewel", "intjewel",
  // joyaux Time-Lost (« radius jewels ») : cibles des Liquid Emotions « Ancient »
  "str_radius_jewel", "dex_radius_jewel", "int_radius_jewel",
]);
const ARCHETYPE_LABEL = {
  str_armour: "Armour", dex_armour: "Evasion", int_armour: "Energy Shield",
  str_dex_armour: "Armour/Evasion", str_int_armour: "Armour/ES", dex_int_armour: "Evasion/ES",
  str_dex_int_armour: "Armour/Evasion/ES",
  strjewel: "Str", dexjewel: "Dex", intjewel: "Int",
  str_radius_jewel: "Str", dex_radius_jewel: "Dex", int_radius_jewel: "Int",
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

// Désécration : mods du domaine `desecrated` (jamais tirés par une monnaie normale, révélés après un
// os Abyssal). Seuls ceux d'un des trois seigneurs (tag `ulaman_mod`, `amanamu_mod`, `kurgal_mod`) sont des
// mods d'équipement : les autres (arbre Genesis, cartes, uniques de Kulemak/Watcher) ne se désécrent pas.
const LORDS = { ulaman_mod: "Ulaman", amanamu_mod: "Amanamu", kurgal_mod: "Kurgal" };
const lordOf = (v) => (v.domain === "desecrated" ? (v.implicit_tags ?? []).map((t) => LORDS[t]).find(Boolean) : undefined);

function importMods(mods) {
  const craft = Object.entries(mods).filter(
    ([, v]) =>
      (ALLOWED_MOD_DOMAINS.has(v.domain) || lordOf(v)) && (v.generation_type === "prefix" || v.generation_type === "suffix") && !v.is_essence_only,
  );
  // famille = (groupe, type) : un même groupe d'exclusion peut contenir plusieurs affixes distincts
  // (ex. IncreaseSocketedGemLevel = niveau des sorts, des sorts de feu, des compétences de mêlée...),
  // chacun avec ses propres tiers. Libellé = gabarit de texte majoritaire, nombres remplacés par « # ».
  const template = (t) => clean(t).replace(/\n/g, " / ").replace(/\(-?\d+(?:\.\d+)?--?\d+(?:\.\d+)?\)|-?\d+(?:\.\d+)?/g, "#");
  const textsByFamily = new Map();
  for (const [, v] of craft) {
    const g = (v.groups ?? [null])[0];
    if (!g || !v.text || lordOf(v)) continue;
    const k = `${g}\u0000${v.type || g}`;
    const counts = textsByFamily.get(k) ?? new Map();
    const t = template(v.text);
    counts.set(t, (counts.get(t) ?? 0) + 1);
    textsByFamily.set(k, counts);
  }
  const familyLabel = new Map();
  for (const [k, counts] of textsByFamily) {
    const top = [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0][0];
    familyLabel.set(k, top);
  }

  const out = [];
  for (const [modId, v] of craft) {
    const g = (v.groups ?? [null])[0];
    if (!g || !v.stats?.length || !v.text) continue;
    const spawn = (v.spawn_weights ?? []).map((s) => ({ tag: s.tag, weight: s.weight }));
    if (!spawn.some((s) => s.tag !== "default" && s.weight > 0)) continue;
    const lord = lordOf(v);
    // un mod Désécré forme sa propre famille, nommée d'après son seigneur, même s'il partage le groupe et
    // le type d'un mod normal (ex. « +1% to all maximum Resistances » d'Amanamu sur bouclier)
    const family = lord ? `${template(v.text)} (${lord})` : familyLabel.get(`${g}\u0000${v.type || g}`) ?? prettify(g);
    // tag du seigneur gardé en premier : les Omens the Sovereign/Liege/Blackblooded filtrent dessus
    const tags = lord ? [...new Set([Object.keys(LORDS).find((k) => LORDS[k] === lord), ...(v.implicit_tags ?? [])])] : v.implicit_tags ?? [];
    out.push({
      id: modId,
      group: g,
      family,
      name: v.name || modId,
      slot: v.generation_type,
      level: v.required_level ?? 1,
      text: clean(v.text),
      tags: tags.slice(0, 8),
      spawn,
      ...(lord ? { desecrated: true } : {}),
    });
  }
  return out;
}

// classes où chaque vraie base est importée telle quelle (voir l'en-tête) plutôt qu'un représentant
const PER_BASE_CLASSES = new Set(["Ring", "Amulet", "Belt", "Quiver"]);
const AFFIX_CAP_STATS = new Set(["local_maximum_prefixes_allowed_+", "local_maximum_suffixes_allowed_+"]);
const slug = (s) => s.replace(/(?<=[a-z0-9])(?=[A-Z])/g, " ").toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, "");

// décalage du plafond porté par les implicites ; valeur fixe dans les données (min == max), sinon on refuse
function capDelta(v, mods, statId) {
  let d = 0;
  for (const id of v.implicits ?? []) {
    for (const s of mods[id]?.stats ?? []) {
      if (s.id !== statId) continue;
      if (s.min !== s.max) throw new Error(`${v.name} : ${statId} variable (${s.min}..${s.max}), non géré`);
      d += s.min;
    }
  }
  return d;
}

function importJewelleryBases(eq, mods) {
  const bases = [];
  const capped = [];
  const kept = eq.filter(([, v]) => PER_BASE_CLASSES.has(v.item_class));
  const byName = new Map();
  for (const e of kept) byName.set(e[1].name, [...(byName.get(e[1].name) ?? []), e]);
  for (const [name, variants] of byName) {
    for (const [, v] of variants) {
      let id = slug(name);
      // plusieurs vraies bases au même nom (Two-Stone Ring, Runemastered ...) : suffixe tiré de
      // l'implicite qui les distingue, pour un identifiant lisible et stable d'un import à l'autre
      if (variants.length > 1) {
        const own = (v.implicits ?? []).filter((i) => variants.every(([, o]) => o === v || !(o.implicits ?? []).includes(i)));
        const key = (own[0] ?? v.implicits?.[0] ?? "").replace(/^.*Implicit/, "").replace(/\d+$/, "");
        id += "_" + slug(key);
      }
      const implicits = (v.implicits ?? []).map((i) => mods[i]?.text).filter(Boolean).map((t) => clean(t).replace(/\n/g, " / "));
      const base = { id, name, item_class: v.item_class, tags: v.tags ?? [], implicits };
      const [dp, ds] = [...AFFIX_CAP_STATS].map((st) => capDelta(v, mods, st));
      if (dp || ds) {
        Object.assign(base, { prefix_cap_delta: dp, suffix_cap_delta: ds });
        capped.push(`${name} ${3 + dp}/${3 + ds}`);
      }
      bases.push(base);
    }
  }
  return { bases, capped };
}

function importBases(items, mods) {
  const eq = Object.entries(items).filter(([, v]) => v.release_state === "released" && ALLOWED_MOD_DOMAINS.has(v.domain) && EQUIP_CLASSES.has(v.item_class));
  const jewellery = importJewelleryBases(eq, mods);
  const byKey = new Map();
  for (const [, v] of eq) {
    const cls = v.item_class;
    if (PER_BASE_CLASSES.has(cls)) continue;
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
    const base = { id: baseId, name, item_class: cls, tags: rep.tags ?? [], implicits: [] };
    // un joyau Rare a 2 préfixes / 2 suffixes au plus, Time-Lost compris (Path of Building, Item.lua :
    // affixLimit 4 pour un Rare de type Jewel) ; le moteur borne aussi le Magique par ce plafond
    if (cls === "Jewel") base.rare_cap = [2, 2];
    bases.push(base);
  }
  bases.push(...jewellery.bases);
  const ids = new Set(bases.map((b) => b.id));
  if (ids.size !== bases.length) throw new Error(`identifiants de base dupliqués : ${bases.map((b) => b.id).filter((id, i, a) => a.indexOf(id) !== i)}`);
  bases.sort((a, b) => (a.item_class + a.id).localeCompare(b.item_class + b.id));
  return { bases, capped: jewellery.capped };
}

function parseArgs(argv) {
  const pos = [];
  let out = "data/sample/dataset.json";
  let carry = "data/sample/dataset.json";
  let indexFile = null;
  let coeFile = null;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "-o" || argv[i] === "--out") out = argv[++i];
    else if (argv[i] === "--carry-prices-from") carry = argv[++i];
    else if (argv[i] === "--index") indexFile = argv[++i];
    else if (argv[i] === "--coe") coeFile = argv[++i];
    else pos.push(argv[i]);
  }
  if (pos.length < 2) {
    console.error("usage: node tools/import_repoe.mjs <mods.min.json> <base_items.min.json> [-o data/sample/dataset.json] [--index index.html] [--coe poec_data.json]");
    process.exit(2);
  }
  return { modsFile: pos[0], baseItemsFile: pos[1], out, carry, indexFile, coeFile };
}

function main() {
  const { modsFile, baseItemsFile, out, carry, indexFile, coeFile } = parseArgs(process.argv.slice(2));
  const mods = JSON.parse(readFileSync(modsFile, "utf8"));
  const items = JSON.parse(readFileSync(baseItemsFile, "utf8"));
  const outMods = importMods(mods);
  const { bases: outBases, capped: cappedBases } = importBases(items, mods);

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

  // les mods Désécrés viennent de l'import (voir LORDS) ; un mod `desecrated` du dataset précédent absent
  // de l'export (ajouté à la main) est quand même reporté, pour ne jamais perdre la Désécration en silence.
  // Les mods exclusifs aux Alloys/Essences Perfect (poids nul partout dans les vraies
  // données, ex. AlloyMysticHelmet) : ils ne peuvent jamais venir de l'import brut non plus. Règle
  // générale et robuste : on reporte tout mod du dataset précédent qui est `desecrated` OU référencé
  // comme cible par au moins une Essence — cette deuxième condition couvre tous les mods exclusifs sans
  // avoir à deviner leur nature un par un.
  const knownIds = new Set(outMods.map((m) => m.id));
  const carriedEssenceTargetIds = new Set((carried.essences ?? []).flatMap((e) => e.targets.flatMap((t) => [t.mod_id, t.alt_mod_id])).filter(Boolean));
  const toCarry = (carried.mods ?? []).filter((m) => m.desecrated || carriedEssenceTargetIds.has(m.id));
  for (const m of toCarry) {
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
    // recettes d'instillation d'amulette : tools/import_liquid_emotions.mjs, jamais dans l'export RePoE
    instills: carried.instills ?? [],
  };

  // poids d'apparition : Craft of Exile (voir tools/coe_weights.mjs) ; sans fichier fourni, on reprend
  // ceux du dataset précédent, pour ne pas revenir en silence à des tiers tous équiprobables
  if (coeFile) {
    reportWeights(applyCoeWeights(dataset, parseCoe(readFileSync(coeFile, "utf8"))));
    dataset.meta.weights_source = `craftofexile.com (poec_data.json, ${dataset.meta.generated_at})`;
  } else if (carried.meta?.weights_source) {
    const keys = new Map((carried.bases ?? []).filter((b) => b.weight_key).map((b) => [b.id, b.weight_key]));
    const weights = new Map((carried.mods ?? []).filter((m) => m.weights).map((m) => [m.id, m.weights]));
    for (const b of dataset.bases) if (keys.has(b.id)) b.weight_key = keys.get(b.id);
    for (const m of dataset.mods) if (weights.has(m.id)) m.weights = weights.get(m.id);
    dataset.meta.weights_source = carried.meta.weights_source;
    console.error(`! pas de --coe : poids Craft of Exile repris du dataset précédent (${carried.meta.weights_source}) ; les mods nouveaux restent à 1`);
  }

  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify(dataset));

  const famSizes = new Map();
  for (const m of outMods) famSizes.set(m.group, (famSizes.get(m.group) ?? 0) + 1);
  console.log(`→ ${out}`);
  console.log(`  ${outBases.length} bases, ${outMods.length} affixes, ${famSizes.size} groupes d'exclusion`);
  console.log(`  monnaies : ${dataset.currencies.length}, Omens : ${dataset.omens.length}, prix : ${Object.keys(dataset.prices).length}`);
  console.log(`  version RePoE : ${gameVersion}`);
  if (cappedBases.length) {
    console.log(`  ${cappedBases.length} bases à plafond préfixes/suffixes propre (Rare) : ${cappedBases.join(", ")}`);
  }
}

main();
