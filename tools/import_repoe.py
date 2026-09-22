#!/usr/bin/env python3
"""Importe un vrai jeu de données PoE2 depuis l'export RePoE (repoe-fork.github.io/poe2).

Entrée : mods.min.json et base_items.min.json, téléchargés depuis
  https://repoe-fork.github.io/poe2/mods.min.json
  https://repoe-fork.github.io/poe2/base_items.min.json
(le site est généré par une action GitHub, pas stocké en clair dans un dépôt : il faut les télécharger
 à la main, ce script ne peut pas le faire lui-même).

Usage :
  python3 tools/import_repoe.py <mods.min.json> <base_items.min.json> [-o data/poe2/dataset.json]

Ce que fait l'import, et pourquoi :
- Mods retenus : domain == "item" (exclut monstres/zones/coffres), generation_type in
  {prefix, suffix} (exclut les mods d'objets uniques et les mods de corruption, qui ne sont pas du
  craft "normal"), is_essence_only == false (ces mods n'apparaissent que via une Essence, jamais par
  tirage classique — les inclure fausserait les poids).
- Groupe d'exclusion (« deux affixes de ce groupe ne coexistent jamais ») = le CHAMP BRUT `groups[0]`
  du jeu. C'est la seule source de vérité mécanique : la famille affichée dans l'interface (utilisée
  pour le sélecteur de tiers) s'appuie dessus mais peut être plus fine si `type` varie au sein d'un
  même groupe (~155 groupes sur 383 mélangent plusieurs stats qui s'excluent mutuellement mais ne sont
  pas des tiers d'un même affixe, ex. BaseLocalDefences = Armure locale OU Évasion locale OU Énergie
  Spirituelle locale, un seul à la fois). Le texte affiché à chaque tier reste toujours exact ; seul le
  NOM de famille au-dessus de la barre de tiers peut être générique dans ces cas-là.
- Bases retenues : équipement seulement (armures, armes, bijoux, carquois/bouclier/focus), release_state
  == "released", domain == "item". Pour les 4 classes d'armure principales + Shield, chaque archétype
  d'attribut (str/dex/int et hybrides) devient une base séparée ; pour le reste, un seul représentant
  par classe. Dans chaque groupe, on garde la variante au plus haut drop_level (l'équivalent « fin de
  jeu ») : c'est elle qui compte pour un craft à haut niveau d'objet.
"""
import json
import re
import sys
import argparse
from collections import defaultdict, Counter

BRACKET = re.compile(r"\[([^\]|]+)(?:\|([^\]]+))?\]")


def clean_text(t: str) -> str:
    """Retire les balises d'affichage du jeu : « [Resistances|Fire Resistance] » -> « Fire Resistance »."""
    return BRACKET.sub(lambda m: m.group(2) or m.group(1), t)


def prettify(key: str) -> str:
    """CamelCase interne -> libellé lisible : « FireResistanceAndMax » -> « Fire Resistance And Max »."""
    s = re.sub(r"(?<!^)(?=[A-Z])", " ", key)
    return re.sub(r"\s+", " ", s).strip()


ARCHETYPE_TAGS = {"str_armour", "dex_armour", "int_armour", "str_dex_armour", "str_int_armour", "dex_int_armour", "str_dex_int_armour"}
ARCHETYPE_LABEL = {
    "str_armour": "Armour", "dex_armour": "Evasion", "int_armour": "Energy Shield",
    "str_dex_armour": "Armour/Evasion", "str_int_armour": "Armour/ES", "dex_int_armour": "Evasion/ES",
    "str_dex_int_armour": "Armour/Evasion/ES",
}
ARCHETYPE_SPLIT_CLASSES = {"Gloves", "Boots", "Body Armour", "Helmet", "Shield"}
EQUIP_CLASSES = {
    "Gloves", "Boots", "Body Armour", "Helmet", "Shield", "Buckler", "Focus",
    "Amulet", "Ring", "Belt", "Quiver",
    "Claw", "Dagger", "Wand", "One Hand Sword", "One Hand Axe", "One Hand Mace",
    "Bow", "Staff", "Two Hand Sword", "Two Hand Axe", "Two Hand Mace",
    "Sceptre", "Spear", "Flail", "Warstaff", "Crossbow",
}
CLASS_TO_ID = {  # slug stable, indépendant du nom d'affichage (qui peut changer entre versions)
    "Gloves": "gloves", "Boots": "boots", "Body Armour": "body_armour", "Helmet": "helmet", "Shield": "shield",
    "Buckler": "buckler", "Focus": "focus", "Amulet": "amulet", "Ring": "ring", "Belt": "belt", "Quiver": "quiver",
    "Claw": "claw", "Dagger": "dagger", "Wand": "wand", "One Hand Sword": "sword_1h", "One Hand Axe": "axe_1h",
    "One Hand Mace": "mace_1h", "Bow": "bow", "Staff": "staff", "Two Hand Sword": "sword_2h", "Two Hand Axe": "axe_2h",
    "Two Hand Mace": "mace_2h", "Sceptre": "sceptre", "Spear": "spear", "Flail": "flail", "Warstaff": "warstaff",
    "Crossbow": "crossbow",
}


def import_mods(mods: dict) -> list[dict]:
    craft = {
        k: v for k, v in mods.items()
        if v.get("domain") == "item" and v.get("generation_type") in ("prefix", "suffix") and not v.get("is_essence_only")
    }
    # libellé de famille : le `type` le plus fréquent du groupe brut, prettifié ; sinon le nom du groupe lui-même
    type_by_group = defaultdict(Counter)
    for v in craft.values():
        g = (v.get("groups") or [None])[0]
        type_by_group[g][v.get("type") or g] += 1
    family_label = {g: prettify(c.most_common(1)[0][0]) for g, c in type_by_group.items()}

    out = []
    for mod_id, v in craft.items():
        g = (v.get("groups") or [None])[0]
        if not g or not v.get("stats") or not v.get("text"):
            continue
        spawn = [{"tag": s["tag"], "weight": s["weight"]} for s in v.get("spawn_weights", [])]
        if not any(s["tag"] != "default" and s["weight"] > 0 for s in spawn):
            continue  # ne tombe jamais (poids nul partout) : inutile de l'importer
        out.append({
            "id": mod_id,
            "group": g,
            "family": family_label.get(g, prettify(g)),
            "name": v.get("name") or mod_id,
            "slot": v["generation_type"],
            "level": v.get("required_level", 1),
            "text": clean_text(v["text"]),
            "tags": (v.get("implicit_tags") or [])[:8],
            "spawn": spawn,
        })
    return out


def import_bases(items: dict) -> list[dict]:
    eq = [v for v in items.values() if v.get("release_state") == "released" and v.get("domain") == "item" and v.get("item_class") in EQUIP_CLASSES]
    by_key = defaultdict(list)
    for v in eq:
        cls = v["item_class"]
        if cls in ARCHETYPE_SPLIT_CLASSES:
            arch = tuple(sorted(set(v.get("tags", [])) & ARCHETYPE_TAGS))
            if not arch:
                continue
            by_key[(cls, arch)].append(v)
        else:
            by_key[(cls, ())].append(v)

    bases = []
    for (cls, arch), variants in by_key.items():
        rep = max(variants, key=lambda v: v.get("drop_level", 0))
        base_id = CLASS_TO_ID[cls] + ("_" + "_".join(a.replace("_armour", "") for a in arch) if arch else "")
        label = ARCHETYPE_LABEL.get(arch[0], "") if len(arch) == 1 else ("/".join(ARCHETYPE_LABEL.get(a, a) for a in arch) if arch else "")
        name = f"{label} {cls}".strip() if label else cls
        bases.append({"id": base_id, "name": name, "item_class": cls, "tags": rep["tags"], "implicit": None})
    bases.sort(key=lambda b: (b["item_class"], b["id"]))
    return bases


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("mods_file")
    ap.add_argument("base_items_file")
    ap.add_argument("-o", "--out", default="data/poe2/dataset.json")
    ap.add_argument("--carry-prices-from", default="data/sample/dataset.json",
                     help="dataset existant dont on réutilise currencies/omens/prices/price_sources (mêmes pour toutes les versions du jeu)")
    args = ap.parse_args()

    mods = json.load(open(args.mods_file, encoding="utf-8"))
    items = json.load(open(args.base_items_file, encoding="utf-8"))
    out_mods = import_mods(mods)
    out_bases = import_bases(items)

    carried = {}
    try:
        carried = json.load(open(args.carry_prices_from, encoding="utf-8"))
    except FileNotFoundError:
        print(f"! {args.carry_prices_from} introuvable : currencies/omens/prices seront vides (à compléter à la main)", file=sys.stderr)

    used_tags = sorted({t for m in out_mods for t in m["tags"]} | {t for b in out_bases for t in b["tags"]})
    dataset = {
        "meta": {
            "schema": 1,
            "source": "repoe-fork.github.io/poe2 (mods.min.json + base_items.min.json)",
            "game_version": "voir https://repoe-fork.github.io/poe2/ pour la version exacte",
            "generated_at": __import__("datetime").date.today().isoformat(),
            "notice": "Données réelles du jeu (poids de spawn, niveaux, tiers). Les prix restent ceux de poe.ninja/l'onglet Réglages.",
            "price_unit": "Exalted Orb",
        },
        "tags": used_tags[:64],
        "bases": out_bases,
        "mods": out_mods,
        "currencies": carried.get("currencies", []),
        "omens": carried.get("omens", []),
        "prices": carried.get("prices", {}),
        "price_sources": carried.get("price_sources", {}),
    }

    import os
    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(dataset, f, ensure_ascii=False)

    fam_sizes = Counter()
    for m in out_mods:
        fam_sizes[m["group"]] += 1
    mixed = sum(1 for g in fam_sizes if len({mm["family"] for mm in out_mods if mm["group"] == g}) > 1)
    print(f"→ {args.out}")
    print(f"  {len(out_bases)} bases, {len(out_mods)} affixes, {len(fam_sizes)} groupes d'exclusion")
    print(f"  monnaies : {len(dataset['currencies'])}, Omens : {len(dataset['omens'])}, prix : {len(dataset['prices'])}")


if __name__ == "__main__":
    main()
