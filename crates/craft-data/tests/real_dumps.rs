//! Tests sur de VRAIS textes copiés depuis le client (tests/fixtures/*.txt), face à un mini-dataset qui reprend
//! les mods de ces objets. Ils figent : format des en-têtes, mods sans plage, mods à deux valeurs, désécration, runes.

use craft_core::{Rarity, Slot};
use craft_data::*;
use serde_json::json;

const BASES: [(&str, &str, &str, &str); 3] = [
    ("cuffs", "Ornate Cuffs", "Gloves", "gloves"),
    ("leggings", "Shamanistic Leggings", "Boots", "boots"),
    ("staff", "Roaring Staff", "Staves", "staff"),
];

/// (id, groupe, famille, nom, slot, niveau, texte, bases où il apparaît)
type M = (&'static str, &'static str, &'static str, &'static str, &'static str, u8, &'static str, &'static [&'static str]);
const MODS: &[M] = &[
    ("life_1", "life", "Maximum Life", "Athlete's", "prefix", 78, "+(120-149) to maximum Life", &["gloves", "boots"]),
    ("life_2", "life", "Maximum Life", "Stalwart", "prefix", 65, "+(100-119) to maximum Life", &["gloves", "boots"]),
    ("acc_1", "acc", "Accuracy Rating", "Exact", "prefix", 60, "+(85-123) to Accuracy Rating", &["gloves"]),
    ("acc_2", "acc", "Accuracy Rating", "Focused", "prefix", 40, "+(61-84) to Accuracy Rating", &["gloves"]),
    ("ltn_atk_1", "ltn_atk", "Lightning Damage to Attacks", "Bolting", "prefix", 40, "Adds (1-2) to (7-9) Lightning damage to Attacks", &["gloves"]),
    ("ltn_atk_2", "ltn_atk", "Lightning Damage to Attacks", "Humming", "prefix", 20, "Adds 1 to (4-6) Lightning damage to Attacks", &["gloves"]),
    ("fire_res_1", "fire_res", "Fire Resistance", "of Magma", "suffix", 60, "+(36-40)% to Fire Resistance", &["gloves", "boots"]),
    ("fire_res_2", "fire_res", "Fire Resistance", "of the Kiln", "suffix", 36, "+(21-25)% to Fire Resistance", &["gloves", "boots"]),
    ("cold_res_1", "cold_res", "Cold Resistance", "of the Lagoon", "suffix", 40, "+(21-25)% to Cold Resistance", &["gloves"]),
    ("cold_res_2", "cold_res", "Cold Resistance", "of the Narwhal", "suffix", 30, "+(16-20)% to Cold Resistance", &["gloves"]),
    ("ltn_res_1", "ltn_res", "Lightning Resistance", "of the Lightning", "suffix", 60, "+(36-40)% to Lightning Resistance", &["gloves", "boots"]),
    ("ltn_res_2", "ltn_res", "Lightning Resistance", "of the Cloud", "suffix", 40, "+(31-35)% to Lightning Resistance", &["gloves", "boots"]),
    ("move_1", "move", "Movement Speed", "Gazelle's", "prefix", 65, "25% increased Movement Speed", &["boots"]),
    ("move_2", "move", "Movement Speed", "Runner's", "prefix", 50, "20% increased Movement Speed", &["boots"]),
    ("arm_es_1", "arm_es", "Armour and Energy Shield", "Infused", "prefix", 60, "(56-67)% increased Armour and Energy Shield", &["boots"]),
    ("rarity_1", "rarity", "Item Rarity", "of Raiding", "suffix", 60, "(11-14)% increased Rarity of Items found", &["boots"]),
    ("spell_1", "spell", "Spell Damage", "Occultist's", "prefix", 60, "(129-148)% increased Spell Damage", &["staff"]),
    ("mana_1", "mana", "Maximum Mana", "Cobalt", "prefix", 10, "+(29-48) to maximum Mana", &["staff"]),
    ("fire_dmg_1", "fire_dmg", "Fire Damage", "Cauterising", "prefix", 50, "(109-128)% increased Fire Damage", &["staff"]),
    ("fire_lvl_1", "fire_lvl", "Fire Spell Skill Level", "of Cinders", "suffix", 80, "+2 to Level of all Fire Spell Skills", &["staff"]),
    ("fire_lvl_2", "fire_lvl", "Fire Spell Skill Level", "of Embers", "suffix", 50, "+1 to Level of all Fire Spell Skills", &["staff"]),
    ("crit_1", "crit", "Critical Spell Damage", "of Ire", "suffix", 40, "(15-21)% increased Critical Spell Damage Bonus", &["staff"]),
    ("int_1", "int", "Intelligence", "of the Augur", "suffix", 30, "+(17-20) to Intelligence", &["staff"]),
];

fn dataset() -> Dataset {
    let mods: Vec<_> = MODS
        .iter()
        .map(|(id, g, fam, name, slot, lvl, text, on)| {
            let mut spawn: Vec<_> = on.iter().map(|t| json!({"tag": t, "weight": 1000})).collect();
            spawn.push(json!({"tag": "default", "weight": 0}));
            json!({"id": id, "group": g, "family": fam, "name": name, "slot": slot, "level": lvl, "text": text, "tags": [], "spawn": spawn})
        })
        .collect();
    let bases: Vec<_> = BASES.iter().map(|(id, n, c, t)| json!({"id": id, "name": n, "item_class": c, "tags": [t]})).collect();
    let ds = json!({"meta": {"schema": 1, "source": "test"}, "tags": [], "bases": bases, "mods": mods, "currencies": [], "prices": {}});
    Dataset::from_json(&ds.to_string()).unwrap()
}

fn base_of(ds: &Dataset, parsed: &ParsedItem) -> BasePool {
    let name = parsed.base_type.as_deref().unwrap();
    let b = ds.bases.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("base « {name} » absente"));
    ds.build_pool(&b.id).unwrap()
}

fn matched_ids(ds: &Dataset, text: &str) -> (ParsedItem, Vec<String>, Vec<String>, ResolvedItem) {
    let parsed = parse_item(text).unwrap();
    let bp = base_of(ds, &parsed);
    let r = resolve(&parsed, &bp, 80).unwrap();
    let ids = r.matched.iter().map(|m| bp.pool.affixes[m.affix_idx as usize].id.clone()).collect();
    let un = r.unmatched.clone();
    (parsed, ids, un, r)
}

const STAFF: &str = include_str!("fixtures/staff_rare.txt");
const GLOVES: &str = include_str!("fixtures/gloves_rare.txt");
const BOOTS: &str = include_str!("fixtures/boots_corrupted.txt");

#[test]
fn gloves_advanced_all_six_mods_match_including_two_value_mod() {
    let ds = dataset();
    let (p, ids, un, r) = matched_ids(&ds, GLOVES);
    assert!(p.advanced && p.rarity == Some(Rarity::Rare) && p.item_level == Some(78) && !p.corrupted);
    assert_eq!(p.base_type.as_deref(), Some("Ornate Cuffs"));
    assert_eq!(ids, ["life_1", "acc_2", "ltn_atk_2", "fire_res_2", "cold_res_2", "ltn_res_1"]);
    assert!(un.is_empty(), "{un:?}");
    assert_eq!(r.item.len(), 6);
}

#[test]
fn boots_fixed_value_mods_desecrated_and_corrupted() {
    let ds = dataset();
    let (p, ids, un, _) = matched_ids(&ds, BOOTS);
    assert!(p.corrupted);
    // « 25% increased Movement Speed » n'a pas de plage : c'est le nom (Gazelle's) qui tranche
    assert_eq!(ids, ["move_1", "life_1", "arm_es_1", "ltn_res_1", "rarity_1", "fire_res_1"]);
    assert!(un.is_empty(), "{un:?}");
    let last = p.mods.last().unwrap();
    assert!(last.desecrated && last.slot == Some(Slot::Suffix) && last.tier == Some(2));
    // la rune « +18% to Cold Resistance (rune) » n'est pas un affixe
    assert!(p.mods.iter().all(|m| m.kind == ModKind::Explicit));
}

#[test]
fn staff_runes_and_grants_skill_are_ignored_and_level_mod_is_matched_by_name() {
    let ds = dataset();
    let (p, ids, un, _) = matched_ids(&ds, STAFF);
    assert_eq!(p.item_class.as_deref(), Some("Staves"));
    assert_eq!(ids, ["spell_1", "mana_1", "fire_dmg_1", "fire_lvl_1", "crit_1", "int_1"]);
    assert!(un.is_empty(), "{un:?}");
}

#[test]
fn basic_format_without_headers_matches_by_all_values() {
    // même objet, en-têtes { … } retirés : l'appariement ne peut plus s'appuyer que sur le texte et les plages annoncées
    let ds = dataset();
    let stripped: String = GLOVES.lines().filter(|l| !l.starts_with('{')).collect::<Vec<_>>().join("\n");
    let (p, ids, un, _) = matched_ids(&ds, &stripped);
    assert!(!p.advanced);
    assert_eq!(ids, ["life_1", "acc_2", "ltn_atk_2", "fire_res_2", "cold_res_2", "ltn_res_1"], "non appariés : {un:?}");
}

#[test]
fn fractured_header_is_detected() {
    // Hypothèse (à confirmer avec un vrai objet fracturé) : même forme que « Desecrated Suffix Modifier ».
    let ds = dataset();
    let txt = GLOVES.replace("{ Prefix Modifier \"Athlete's\"", "{ Fractured Prefix Modifier \"Athlete's\"");
    let (p, _, un, r) = matched_ids(&ds, &txt);
    assert!(p.mods[0].fractured && un.is_empty());
    assert!(r.item.has_fractured());
}
