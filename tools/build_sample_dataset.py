#!/usr/bin/env python3
"""Génère data/sample/dataset.json : jeu de données ILLUSTRATIF (poids et niveaux inventés).

Il sert à développer/tester l'application hors ligne. Remplace-le par un dataset importé de poe2db
(voir docs/DATA.md) : le schéma est identique, seuls les nombres changent.
"""
import json, pathlib, datetime

LEVELS = [1, 11, 22, 33, 46, 58, 70, 80]
TIER_W = [1.0, 1.0, 1.0, 0.9, 0.65, 0.4, 0.22, 0.1]  # tiers élevés (niveau haut) plus rares

def ranges(lo0, hi0, lo1, hi1, n):
    out = []
    for i in range(n):
        t = i / (n - 1)
        lo = round(lo0 + (lo1 - lo0) * t); hi = round(hi0 + (hi1 - hi0) * t)
        out.append((lo, max(lo, hi)))
    return out

def fam(fid, label, slot, tpl, span, weights, tags, n=8, tpl2=None, span2=None):
    """tpl : gabarit avec {r} ; weights : {tag_de_base: poids_max}"""
    r1 = ranges(*span, n)
    r2 = ranges(*span2, n) if span2 else None
    mods = []
    lv = LEVELS[-n:] if n < 8 else LEVELS
    tw = TIER_W[-n:] if n < 8 else TIER_W
    for i in range(n):
        tier_from_top = n - i  # i=0 → plus bas niveau → tier n
        text = tpl.format(r=f"({r1[i][0]}-{r1[i][1]})", r2=f"({r2[i][0]}-{r2[i][1]})" if r2 else "")
        mods.append({
            "id": f"{fid}_{tier_from_top}",
            "group": fid, "family": label, "name": f"{label} {['','I','II','III','IV','V','VI','VII','VIII'][tier_from_top]}",
            "slot": slot, "level": lv[i], "text": text, "tags": tags,
            "spawn": [{"tag": t, "weight": max(1, round(w * tw[i]))} for t, w in weights.items()] + [{"tag": "default", "weight": 0}],
        })
    return mods

DEX, STR, WAND = "dex_armour", "str_armour", "wand"
mods = []
# ── Préfixes armures
mods += fam("evasion_flat", "Evasion flat", "prefix", "+{r} to Evasion Rating", (10, 20, 190, 240), {DEX: 1000}, ["defences"])
mods += fam("evasion_pct", "Evasion %", "prefix", "{r}% increased Evasion Rating", (15, 26, 89, 100), {DEX: 800}, ["defences"])
mods += fam("armour_flat", "Armour flat", "prefix", "+{r} to Armour", (12, 25, 290, 340), {STR: 1000}, ["defences"])
mods += fam("armour_pct", "Armour %", "prefix", "{r}% increased Armour", (15, 26, 89, 100), {STR: 800}, ["defences"])
mods += fam("life_flat", "Maximum Life", "prefix", "+{r} to maximum Life", (10, 19, 90, 109), {DEX: 1000, STR: 1000}, ["life"])
mods += fam("es_flat", "Maximum Energy Shield", "prefix", "+{r} to maximum Energy Shield", (5, 9, 49, 60), {DEX: 500, STR: 300}, ["defences"], n=7)
mods += fam("phys_atk", "Physical Damage to Attacks", "prefix", "Adds {r} to {r2} Physical Damage to Attacks", (1, 2, 10, 17), {DEX: 700, STR: 500}, ["damage", "physical", "attack"], n=7, span2=(3, 5, 22, 34))
mods += fam("fire_atk", "Fire Damage to Attacks", "prefix", "Adds {r} to {r2} Fire Damage to Attacks", (2, 3, 21, 35), {DEX: 600, STR: 600}, ["damage", "fire", "attack"], n=7, span2=(5, 8, 42, 62))
mods += fam("cold_atk", "Cold Damage to Attacks", "prefix", "Adds {r} to {r2} Cold Damage to Attacks", (2, 3, 19, 30), {DEX: 600, STR: 600}, ["damage", "cold", "attack"], n=7, span2=(4, 7, 38, 56))
# ── Suffixes communs
for fid, lab, res in [("fire_res", "Fire Resistance", "Fire"), ("cold_res", "Cold Resistance", "Cold"), ("lightning_res", "Lightning Resistance", "Lightning")]:
    mods += fam(fid, lab, "suffix", "+{r}% to " + res + " Resistance", (6, 10, 36, 45), {DEX: 1000, STR: 1000, WAND: 700}, ["resistance"])
mods += fam("chaos_res", "Chaos Resistance", "suffix", "+{r}% to Chaos Resistance", (4, 7, 21, 27), {DEX: 500, STR: 500, WAND: 300}, ["resistance", "chaos"], n=5)
mods += fam("dexterity", "Dexterity", "suffix", "+{r} to Dexterity", (5, 8, 33, 38), {DEX: 700, WAND: 200}, ["attribute"])
mods += fam("strength", "Strength", "suffix", "+{r} to Strength", (5, 8, 33, 38), {STR: 700, DEX: 200, WAND: 200}, ["attribute"])
mods += fam("intelligence", "Intelligence", "suffix", "+{r} to Intelligence", (5, 8, 33, 38), {WAND: 700, DEX: 200}, ["attribute"])
mods += fam("attack_speed", "Attack Speed", "suffix", "{r}% increased Attack Speed", (3, 4, 9, 10), {DEX: 400, STR: 200}, ["attack", "speed"], n=6)
mods += fam("accuracy", "Accuracy Rating", "suffix", "+{r} to Accuracy Rating", (20, 40, 350, 480), {DEX: 600, STR: 300}, ["attack"])
mods += fam("stun_thresh", "Stun Threshold", "suffix", "+{r} to Stun Threshold", (12, 24, 170, 200), {STR: 500}, ["defences"], n=6)
mods += fam("mana_regen", "Mana Regeneration", "suffix", "{r}% increased Mana Regeneration Rate", (10, 19, 60, 69), {DEX: 300, STR: 300, WAND: 800}, ["mana"], n=6)
mods += fam("rarity", "Item Rarity", "suffix", "{r}% increased Rarity of Items found", (6, 10, 21, 25), {DEX: 250, STR: 250}, ["drop"], n=5)
# ── Baguette
mods += fam("spell_dmg", "Spell Damage", "prefix", "{r}% increased Spell Damage", (5, 9, 60, 74), {WAND: 1000}, ["caster", "damage"])
mods += fam("fire_spell", "Fire Damage (spell)", "prefix", "Adds {r} to {r2} Fire Damage", (2, 3, 37, 56), {WAND: 700}, ["caster", "fire", "damage"], n=7, span2=(4, 6, 68, 102))
mods += fam("cold_spell", "Cold Damage (spell)", "prefix", "Adds {r} to {r2} Cold Damage", (2, 3, 33, 50), {WAND: 700}, ["caster", "cold", "damage"], n=7, span2=(3, 5, 60, 92))
mods += fam("light_spell", "Lightning Damage (spell)", "prefix", "Adds {r} to {r2} Lightning Damage", (1, 2, 46, 70), {WAND: 700}, ["caster", "lightning", "damage"], n=7, span2=(5, 8, 86, 130))
mods += fam("phys_wand", "Physical Damage (wand)", "prefix", "{r}% increased Physical Damage", (15, 29, 150, 169), {WAND: 500}, ["damage", "physical"], n=6)
mods += fam("mana_flat", "Maximum Mana", "prefix", "+{r} to maximum Mana", (10, 19, 90, 109), {WAND: 700}, ["mana"], n=6)
mods += fam("cast_speed", "Cast Speed", "suffix", "{r}% increased Cast Speed", (8, 11, 28, 33), {WAND: 700}, ["caster", "speed"], n=6)
mods += fam("spell_crit", "Spell Critical Hit Chance", "suffix", "{r}% increased Critical Hit Chance for Spells", (10, 19, 90, 109), {WAND: 600}, ["caster", "critical"], n=6)
mods += fam("crit_bonus", "Critical Damage Bonus", "suffix", "{r}% increased Critical Damage Bonus", (10, 14, 30, 34), {WAND: 500}, ["critical"], n=5)
mods += fam("skill_level", "Spell Skill Level", "suffix", "+{r} to Level of all Spell Skills", (1, 1, 5, 5), {WAND: 60}, ["caster"], n=3)

bases = [
    {"id": "gloves_dex", "name": "Evasion Gloves", "item_class": "Gloves", "tags": ["gloves", DEX, "armour"], "implicit": None},
    {"id": "body_str", "name": "Armour Body Armour", "item_class": "Body Armours", "tags": ["body_armour", STR, "armour"], "implicit": None},
    {"id": "wand_int", "name": "Wand", "item_class": "Wands", "tags": ["wand", WAND, "weapon"], "implicit": None},
]

currencies = [
    # id, label, kind, min_mod_level, price_id, default_enabled
    ("transmute", "Orb of Transmutation", "transmute", 0, "transmute", True),
    ("transmute_greater", "Greater Orb of Transmutation", "transmute", 35, "transmute_greater", True),
    ("transmute_perfect", "Perfect Orb of Transmutation", "transmute", 50, "transmute_perfect", True),
    ("augment", "Orb of Augmentation", "augment", 0, "augment", True),
    ("augment_greater", "Greater Orb of Augmentation", "augment", 35, "augment_greater", True),
    ("augment_perfect", "Perfect Orb of Augmentation", "augment", 50, "augment_perfect", True),
    ("regal", "Regal Orb", "regal", 0, "regal", True),
    ("regal_greater", "Greater Regal Orb", "regal", 35, "regal_greater", True),
    ("regal_perfect", "Perfect Regal Orb", "regal", 50, "regal_perfect", True),
    ("alchemy", "Orb of Alchemy", "alchemy", 0, "alchemy", True),
    ("exalt", "Exalted Orb", "exalt", 0, "exalt", True),
    ("exalt_greater", "Greater Exalted Orb", "exalt", 35, "exalt_greater", True),
    ("exalt_perfect", "Perfect Exalted Orb", "exalt", 50, "exalt_perfect", True),
    ("chaos", "Chaos Orb", "chaos", 0, "chaos", True),
    ("chaos_greater", "Greater Chaos Orb", "chaos", 35, "chaos_greater", True),
    ("chaos_perfect", "Perfect Chaos Orb", "chaos", 50, "chaos_perfect", True),
    ("annul", "Orb of Annulment", "annul", 0, "annul", True),
    ("fracture", "Fracturing Orb", "fracture", 0, "fracture", True),
]
omens = [
    {"id": "omen_sinistral_exaltation", "label": "Omen of Sinistral Exaltation", "add_slot": "prefix", "remove_slot": None, "applies_to": ["exalt"], "price_id": "omen_sinistral_exaltation"},
    {"id": "omen_dextral_exaltation", "label": "Omen of Dextral Exaltation", "add_slot": "suffix", "remove_slot": None, "applies_to": ["exalt"], "price_id": "omen_dextral_exaltation"},
    {"id": "omen_sinistral_annulment", "label": "Omen of Sinistral Annulment", "add_slot": None, "remove_slot": "prefix", "applies_to": ["annul"], "price_id": "omen_sinistral_annulment"},
    {"id": "omen_dextral_annulment", "label": "Omen of Dextral Annulment", "add_slot": None, "remove_slot": "suffix", "applies_to": ["annul"], "price_id": "omen_dextral_annulment"},
    {"id": "omen_sinistral_erasure", "label": "Omen of Sinistral Erasure", "add_slot": None, "remove_slot": "prefix", "applies_to": ["chaos"], "price_id": "omen_sinistral_erasure"},
    {"id": "omen_dextral_erasure", "label": "Omen of Dextral Erasure", "add_slot": None, "remove_slot": "suffix", "applies_to": ["chaos"], "price_id": "omen_dextral_erasure"},
]
# Prix par défaut : relevé poe.ninja du 21/09/2026 (ligue « Forbidden Rites »), en Exalted Orb.
# Ils servent hors ligne ; l'application les remplace par les prix du marché (bouton « Actualiser depuis poe.ninja »).
prices = {
    "base_white": 2.0, "base_salvage": 0.0,
    "transmute": 1.53, "transmute_greater": 1.78, "transmute_perfect": 15.5,
    "augment": 2.88, "augment_greater": 5.34, "augment_perfect": 135.0,
    "regal": 2.53, "regal_greater": 2.70, "regal_perfect": 19.7,
    "alchemy": 3.17,
    "exalt": 1.0, "exalt_greater": 4.78, "exalt_perfect": 1379.0,
    "chaos": 56.4, "chaos_greater": 167.0, "chaos_perfect": 2814.0,
    "annul": 304.0, "fracture": 4024.0,
    "omen_sinistral_exaltation": 37.6, "omen_dextral_exaltation": 19.2,
    "omen_sinistral_annulment": 7464.0, "omen_dextral_annulment": 4066.0,
    "omen_sinistral_erasure": 6979.0, "omen_dextral_erasure": 3944.0,
}
# Correspondance price_id -> objet poe.ninja (catégorie de l'API + identifiant). `base_*` : saisie manuelle uniquement.
price_sources = {
    "transmute": ("Currency", "transmute"), "transmute_greater": ("Currency", "greater-orb-of-transmutation"), "transmute_perfect": ("Currency", "perfect-orb-of-transmutation"),
    "augment": ("Currency", "aug"), "augment_greater": ("Currency", "greater-orb-of-augmentation"), "augment_perfect": ("Currency", "perfect-orb-of-augmentation"),
    "regal": ("Currency", "regal"), "regal_greater": ("Currency", "greater-regal-orb"), "regal_perfect": ("Currency", "perfect-regal-orb"),
    "alchemy": ("Currency", "alch"),
    "exalt": ("Currency", "exalted"), "exalt_greater": ("Currency", "greater-exalted-orb"), "exalt_perfect": ("Currency", "perfect-exalted-orb"),
    "chaos": ("Currency", "chaos"), "chaos_greater": ("Currency", "greater-chaos-orb"), "chaos_perfect": ("Currency", "perfect-chaos-orb"),
    "annul": ("Currency", "annul"), "fracture": ("Currency", "fracturing-orb"),
    **{f"omen_{side}_{kind}": ("Ritual", f"omen-of-{side}-{kind}") for side in ("sinistral", "dextral") for kind in ("exaltation", "annulment", "erasure")},
}
tags = sorted({t for m in mods for t in m["tags"]} | {t for b in bases for t in b["tags"]})
ds = {
    "meta": {
        "schema": 1, "source": "sample-placeholder", "game_version": "n/a",
        "generated_at": datetime.date.today().isoformat(),
        "notice": "JEU DE DONNÉES ILLUSTRATIF : poids, niveaux et prix sont inventés. Remplacer par un import poe2db.",
        "price_unit": "Exalted Orb", "prices_note": "Prix par défaut relevés sur poe.ninja le 2026-09-21 (Forbidden Rites).",
    },
    "tags": tags, "bases": bases, "mods": mods,
    "currencies": [dict(id=c[0], label=c[1], kind=c[2], min_mod_level=c[3], price_id=c[4], default_enabled=c[5]) for c in currencies],
    "omens": omens, "prices": prices,
    "price_sources": {k: {"ninja_type": t, "ninja_id": i} for k, (t, i) in price_sources.items()},
}
out = pathlib.Path(__file__).resolve().parents[1] / "data" / "sample" / "dataset.json"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(ds, indent=1, ensure_ascii=False))
print(f"{len(mods)} mods, {len(bases)} bases -> {out}")
