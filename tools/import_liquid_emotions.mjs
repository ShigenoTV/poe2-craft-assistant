#!/usr/bin/env node
// Liquid Emotions (joyaux) et recettes d'instillation d'amulette, depuis les données du jeu exportées par
// Path of Building PoE2 (dépôt PathOfBuildingCommunity/PathOfBuilding-PoE2, fichiers générés depuis le
// client du jeu) :
//   - src/Data/LiquidEmotions.lua : chaque Liquid Emotion → mod garanti par type de joyau (Ruby,
//     Sapphire, Emerald, Diamond), joyau normal ou Time-Lost (`radiusJewel`) ;
//   - src/TreeData/<version>/tree.lua : champ `recipe` des passifs = les trois émotions, dans l'ordre,
//     qui instillent ce passif sur une amulette (The Withered Willow).
// Recoupé avec poe2db (pages des 26 émotions, et recettes de Fast Acting Toxins et Splinters).
//
// Utilisation (idempotent, réécrit seulement les entrées `liquid_*`, les prix/sources `liquid_*` et
// `instills` du dataset) :
//   node tools/import_liquid_emotions.mjs <LiquidEmotions.lua> <tree.lua> <mods.json RePoE> [-o data/sample/dataset.json]
//
// Mécanique (texte du jeu) : « Removes a random modifier and Augments a Rare Basic Jewel [ou Time-Lost
// Jewel] with a new guaranteed Crafted modifier » = une Essence sur objet Rare (`requires_rare`).
// Potent Ferocity, Potent Contempt et Ancient Potent Contempt proposent un préfixe OU un suffixe sur un
// même joyau (infobulle du jeu : « Ruby Prefix: … / Ruby Suffix: … ») : `mod_id` = le préfixe,
// `alt_mod_id` = le suffixe, ajoutés à 50/50 (règle donnée par Max le 2026-10-05, non écrite dans le
// jeu) parmi ceux qui ont la place. Les mods « +1 Prefix/Suffix Modifier allowed » portent leur décalage
// de plafond (`prefix_cap_delta` / `suffix_cap_delta`, stats `local_maximum_*_allowed_+` de RePoE).
import { readFileSync, writeFileSync } from "node:fs";

const args = process.argv.slice(2);
let out = "data/sample/dataset.json";
const pos = [];
for (let i = 0; i < args.length; i++) {
  if (args[i] === "-o") out = args[++i];
  else pos.push(args[i]);
}
if (pos.length < 3) {
  console.error("usage: node tools/import_liquid_emotions.mjs <LiquidEmotions.lua> <tree.lua> <mods.json> [-o dataset.json]");
  process.exit(2);
}
const [emotionsFile, treeFile, modsFile] = pos;
const ds = JSON.parse(readFileSync(out, "utf8"));
const repoeMods = JSON.parse(readFileSync(modsFile, "utf8"));

// ── Liquid Emotions ──
const emotions = [];
for (const line of readFileSync(emotionsFile, "utf8").split("\n")) {
  const m = line.match(/^\s*\["([^"]+)"\] = \{ name = "([^"]+)", radiusJewel = (true|false), .*?mods = \{(.*)\}, \},\s*$/);
  if (!m) continue;
  const [, path, name, radius, modsStr] = m;
  const byJewel = {};
  for (const j of modsStr.matchAll(/\["(Ruby|Sapphire|Emerald|Diamond)"\] = \{([^}]*)\}/g)) {
    byJewel[j[1]] = [...j[2].matchAll(/\["(Prefix|Suffix)"\] = "([^"]+)"/g)].map((x) => ({ slot: x[1].toLowerCase(), mod: x[2] }));
  }
  emotions.push({ path, name, radius: radius === "true", byJewel });
}
if (emotions.length !== 26) throw new Error(`26 Liquid Emotions attendues, ${emotions.length} lues dans ${emotionsFile}`);

// « Ancient Potent Liquid Melancholy » → melancholy ; id `liquid_<émotion>[_ancient]` (ids existants gardés)
const word = (name) => name.split(" ").pop().toLowerCase();
const emotionId = (e) => `liquid_${word(e.name)}${e.radius ? "_ancient" : ""}`;
const ninjaId = (name) => name.toLowerCase().replace(/ /g, "-");

// tags de base : joyau normal (strjewel...) ou Time-Lost (str_radius_jewel...). Le Diamond porte les
// trois tags : il vient EN PREMIER (tous les tags requis, « & »), sinon il prendrait la cible du Ruby.
const JEWEL_TAGS = {
  false: { Diamond: "strjewel&dexjewel&intjewel", Ruby: "strjewel", Sapphire: "intjewel", Emerald: "dexjewel" },
  true: { Diamond: "str_radius_jewel&dex_radius_jewel&int_radius_jewel", Ruby: "str_radius_jewel", Sapphire: "int_radius_jewel", Emerald: "dex_radius_jewel" },
};

const BRACKET = /\[([^\]|]+)(?:\|([^\]]+))?\]/g;
const clean = (t) => t.replace(BRACKET, (_, a, b) => b ?? a);
const template = (t) => clean(t).replace(/\n/g, " / ").replace(/\(-?\d+(?:\.\d+)?--?\d+(?:\.\d+)?\)|-?\d+(?:\.\d+)?/g, "#");

const CAP_STATS = { "local_maximum_prefixes_allowed_+": "prefix_cap_delta", "local_maximum_suffixes_allowed_+": "suffix_cap_delta" };
const known = new Set(ds.mods.map((m) => m.id));
const addedMods = [];
const essences = [];
for (const e of emotions) {
  const targets = [];
  for (const jewel of ["Diamond", "Ruby", "Sapphire", "Emerald"]) {
    const list = e.byJewel[jewel] ?? [];
    if (list.length > 2 || (list.length === 2 && (list[0].slot !== "prefix" || list[1].slot !== "suffix"))) {
      throw new Error(`${e.name} sur ${jewel} : un préfixe puis un suffixe attendus, lu ${JSON.stringify(list)}`);
    }
    // mod_id vide : l'émotion ne s'applique pas à ce joyau (cas du Diamond pour la plupart)
    const target = { item_tags: [JEWEL_TAGS[e.radius][jewel]], mod_id: list[0]?.mod ?? "" };
    if (list[1]) target.alt_mod_id = list[1].mod;
    targets.push(target);
    for (const t of list) {
      const r = repoeMods[t.mod];
      if (!r) throw new Error(`${e.name} : mod ${t.mod} absent de RePoE`);
      if (r.generation_type !== t.slot) throw new Error(`${e.name} : ${t.mod} est un ${r.generation_type}, pas un ${t.slot}`);
      if (!known.has(t.mod)) {
        // mod « Crafted » à poids nul partout : jamais importé, ajouté ici (reporté ensuite par l'import
        // RePoE comme toute cible d'Essence)
        known.add(t.mod);
        const mod = {
          id: t.mod,
          group: r.groups[0],
          family: `${template(r.text)} (Liquid Emotion)`,
          name: r.name || t.mod,
          slot: r.generation_type,
          level: r.required_level ?? 1,
          text: clean(r.text).replace(/\n/g, " / "),
          tags: [],
          spawn: [{ tag: "default", weight: 0 }],
        };
        for (const st of r.stats ?? []) {
          if (!CAP_STATS[st.id] || !st.min) continue;
          if (st.min !== st.max) throw new Error(`${t.mod} : ${st.id} variable, non géré`);
          mod[CAP_STATS[st.id]] = st.min;
        }
        addedMods.push(mod);
      }
    }
  }
  // sans Diamond, la cible vide en tête ne sert qu'à l'exclure : on la garde pour la lisibilité
  essences.push({ id: emotionId(e), label: e.name, price_id: emotionId(e), default_enabled: true, requires_rare: true, targets });
}

// ── Recettes d'instillation ──
const tree = readFileSync(treeFile, "utf8");
const byWord = new Map(emotions.filter((e) => !e.radius).map((e) => [word(e.name), emotionId(e)]));
const instills = [];
for (const m of tree.matchAll(/\n\t\t\[(\d+)\]=\{(.*?)\n\t\t\}/gs)) {
  const body = m[2];
  const rec = body.match(/recipe=\{(.*?)\}/s);
  if (!rec) continue;
  const name = body.match(/\n\t\t\tname="((?:[^"\\]|\\.)*)"/)[1].replace(/\\(.)/g, "$1");
  const recipe = [...rec[1].matchAll(/"(\w+)"/g)].map((x) => {
    const id = byWord.get(x[1].toLowerCase());
    if (!id) throw new Error(`${name} : émotion inconnue ${x[1]}`);
    return id;
  });
  const st = body.match(/\n\t\t\tstats=\{(.*?)\n\t\t\t\}/s);
  const stats = st ? [...st[1].matchAll(/\]="((?:[^"\\]|\\.)*)"/g)].map((x) => x[1].replace(/\\(.)/g, "$1")) : [];
  instills.push({ skill: +m[1], name, stats, emotions: recipe });
}
instills.sort((a, b) => a.name.localeCompare(b.name) || a.skill - b.skill);

// ── Écriture ──
ds.essences = [...ds.essences.filter((e) => !e.id.startsWith("liquid_")), ...essences];
ds.mods.push(...addedMods);
for (const e of emotions) {
  const id = emotionId(e);
  ds.price_sources[id] = { ninja_type: "Delirium", ninja_id: ninjaId(e.name) };
  ds.prices[id] ??= 1;
}
ds.instills = instills;
writeFileSync(out, JSON.stringify(ds));

const counts = essences.map((e) => e.targets.filter((t) => t.mod_id).length + e.targets.filter((t) => t.alt_mod_id).length);
console.log(`→ ${out}`);
console.log(`  ${emotions.length} Liquid Emotions dans le jeu, ${essences.length} importées (${counts.reduce((a, b) => a + b, 0)} cibles), ${addedMods.length} mods Crafted ajoutés`);
console.log(`  à deux mods possibles (préfixe OU suffixe) : ${essences.filter((e) => e.targets.some((t) => t.alt_mod_id)).map((e) => e.label).join(", ")}`);
console.log(`  ${instills.length} recettes d'instillation`);
