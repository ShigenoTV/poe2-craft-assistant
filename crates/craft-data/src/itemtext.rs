//! Lecture du texte copié depuis le jeu (Ctrl+C simple ou « avancé » Ctrl+Alt+C).
//!
//! ⚠ Formats à valider avec de vrais dumps de ton client (l'app propose un panneau « Presse-papiers brut »).
//! Le format avancé est le plus fiable : il donne le slot, le nom et le tier de chaque affixe.

use crate::dataset::BasePool;
use craft_core::*;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModKind {
    Implicit,
    Explicit,
    Rune,
    Enchant,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedMod {
    pub kind: ModKind,
    pub slot: Option<Slot>,
    pub name: Option<String>,
    pub tier: Option<u8>,
    pub tags: Vec<String>,
    pub lines: Vec<String>,
    pub fractured: bool,
    pub desecrated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedItem {
    pub item_class: Option<String>,
    pub rarity_label: Option<String>,
    pub rarity: Option<Rarity>,
    pub name: Option<String>,
    pub base_type: Option<String>,
    pub item_level: Option<u8>,
    pub corrupted: bool,
    pub advanced: bool,
    pub mods: Vec<ParsedMod>,
}

fn re_sep() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^-{4,}$").unwrap())
}
fn re_header() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r#"^\{\s*(?P<pre>[^"(—–}]*?)\s*(?:"(?P<name>[^"]*)")?\s*(?:\(Tier:\s*(?P<tier>\d+)\))?\s*(?:[—–-]\s*(?P<tags>[^}]*?))?\s*\}$"#).unwrap()
    })
}
fn re_num() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?:(\d+(?:\.\d+)?)\((\d+(?:\.\d+)?)[-–—](\d+(?:\.\d+)?)\))|(?:\((\d+(?:\.\d+)?)[-–—](\d+(?:\.\d+)?)\))|(\d+(?:\.\d+)?)").unwrap()
    })
}
fn re_suffix_tag() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\s*\((implicit|rune|enchant|fractured|desecrated|crafted|augmented)\)\s*$").unwrap())
}

const KEY_CLASS: &[&str] = &["Item Class", "Classe d'objet"];
const KEY_RARITY: &[&str] = &["Rarity", "Rareté", "Rarete"];
const KEY_ILVL: &[&str] = &["Item Level", "Niveau de l'objet"];
const NON_MOD_PREFIX: &[&str] = &[
    "Requires", "Requirements", "Level:", "Sockets", "Quality", "Evasion Rating", "Armour:", "Energy Shield:", "Block", "Physical Damage", "Critical",
    "Attacks per Second", "Item Level", "Item Class", "Rarity", "Corrupted", "Mirrored", "Unidentified", "Note:", "Stack Size",
];

fn strip_key<'a>(line: &'a str, keys: &[&str]) -> Option<&'a str> {
    for k in keys {
        if let Some(rest) = line.strip_prefix(k) {
            let rest = rest.trim_start();
            if let Some(v) = rest.strip_prefix(':') {
                return Some(v.trim());
            }
        }
    }
    None
}

pub fn looks_like_item(text: &str) -> bool {
    text.lines().take(4).any(|l| strip_key(l.trim(), KEY_CLASS).is_some() || strip_key(l.trim(), KEY_RARITY).is_some())
}

pub fn parse_item(text: &str) -> Result<ParsedItem, String> {
    let text = text.replace('\r', "");
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    if lines.is_empty() || !looks_like_item(&text) {
        return Err("le texte ne ressemble pas à un objet (« Item Class / Rarity » absent)".into());
    }
    // sections
    let mut sections: Vec<Vec<&str>> = vec![vec![]];
    for l in &lines {
        if re_sep().is_match(l) {
            sections.push(vec![]);
        } else if !l.is_empty() {
            sections.last_mut().unwrap().push(l);
        }
    }
    let mut it = ParsedItem {
        item_class: None,
        rarity_label: None,
        rarity: None,
        name: None,
        base_type: None,
        item_level: None,
        corrupted: false,
        advanced: false,
        mods: vec![],
    };
    // en-tête
    let mut names: Vec<&str> = Vec::new();
    for l in &sections[0] {
        if let Some(v) = strip_key(l, KEY_CLASS) {
            it.item_class = Some(v.to_string());
        } else if let Some(v) = strip_key(l, KEY_RARITY) {
            it.rarity_label = Some(v.to_string());
            it.rarity = match v.to_lowercase().as_str() {
                "normal" => Some(Rarity::Normal),
                "magic" | "magique" => Some(Rarity::Magic),
                "rare" => Some(Rarity::Rare),
                _ => None, // unique, gemme, monnaie…
            };
        } else {
            names.push(l);
        }
    }
    match names.as_slice() {
        [n, b, ..] => {
            it.name = Some(n.to_string());
            it.base_type = Some(b.to_string());
        }
        [n] => {
            it.name = Some(n.to_string());
            it.base_type = Some(n.to_string());
        }
        _ => {}
    }

    it.advanced = sections.iter().any(|s| s.iter().any(|l| re_header().is_match(l) && l.starts_with('{')));
    let mut past_ilvl = false;
    for sec in sections.iter().skip(1) {
        for l in sec {
            if let Some(v) = strip_key(l, KEY_ILVL) {
                it.item_level = v.split_whitespace().next().and_then(|x| x.parse().ok());
                past_ilvl = true;
            }
            if l.eq_ignore_ascii_case("corrupted") || l.eq_ignore_ascii_case("corrompu") {
                it.corrupted = true;
            }
        }
        if it.advanced {
            parse_advanced_section(sec, &mut it.mods);
        } else if past_ilvl && !sec.iter().any(|l| strip_key(l, KEY_ILVL).is_some()) {
            parse_basic_section(sec, &mut it.mods);
        }
    }
    Ok(it)
}

fn parse_advanced_section(sec: &[&str], out: &mut Vec<ParsedMod>) {
    let mut cur: Option<ParsedMod> = None;
    for l in sec {
        if l.starts_with('{') && l.ends_with('}') {
            if let Some(m) = cur.take() {
                out.push(m);
            }
            let Some(c) = re_header().captures(l) else { continue };
            let pre = c.name("pre").map(|m| m.as_str().to_lowercase()).unwrap_or_default();
            let kind = if pre.contains("implicit") {
                ModKind::Implicit
            } else if pre.contains("rune") {
                ModKind::Rune
            } else if pre.contains("enchant") {
                ModKind::Enchant
            } else {
                ModKind::Explicit
            };
            let slot = if pre.contains("prefix") {
                Some(Slot::Prefix)
            } else if pre.contains("suffix") {
                Some(Slot::Suffix)
            } else {
                None
            };
            cur = Some(ParsedMod {
                kind,
                slot,
                name: c.name("name").map(|m| m.as_str().to_string()),
                tier: c.name("tier").and_then(|m| m.as_str().parse().ok()),
                tags: c.name("tags").map(|m| m.as_str().split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect()).unwrap_or_default(),
                lines: vec![],
                fractured: pre.contains("fractured"),
                desecrated: pre.contains("desecrated"),
            });
        } else if let Some(m) = cur.as_mut() {
            let low = l.to_lowercase();
            if low.contains("(fractured)") {
                m.fractured = true;
            }
            m.lines.push(re_suffix_tag().replace(l, "").to_string());
        }
    }
    if let Some(m) = cur.take() {
        out.push(m);
    }
}

fn parse_basic_section(sec: &[&str], out: &mut Vec<ParsedMod>) {
    for l in sec {
        if NON_MOD_PREFIX.iter().any(|p| l.starts_with(p)) || l.ends_with(':') {
            continue;
        }
        let low = l.to_lowercase();
        let kind = if low.ends_with("(implicit)") {
            ModKind::Implicit
        } else if low.ends_with("(rune)") {
            ModKind::Rune
        } else if low.ends_with("(enchant)") {
            ModKind::Enchant
        } else {
            ModKind::Explicit
        };
        // texte d'ambiance / italique : sans chiffre ni signe, on l'ignore côté résolution (non apparié)
        out.push(ParsedMod {
            kind,
            slot: None,
            name: None,
            tier: None,
            tags: vec![],
            lines: vec![re_suffix_tag().replace(l, "").to_string()],
            fractured: low.contains("(fractured)"),
            desecrated: low.contains("(desecrated)"),
        });
    }
}

// ───────────────────────── Résolution contre le pool d'une base ─────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedMod {
    pub affix_idx: u16,
    pub tier: u8,
    pub name: String,
    pub text: String,
    pub slot: Slot,
    pub fractured: bool,
}

#[derive(Clone, Debug)]
pub struct ResolvedItem {
    pub item: ItemState,
    pub matched: Vec<MatchedMod>,
    pub unmatched: Vec<String>,
}

pub fn normalize(line: &str) -> String {
    let s = re_suffix_tag().replace(line, "");
    let s = re_num().replace_all(&s, "#");
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Un nombre d'une ligne : plage annoncée « 131(129-148) » / « (40-49) », ou valeur fixe (« 25 » ⇒ plage [25, 25]).
#[derive(Clone, Copy, Debug)]
struct Tok {
    value: Option<f64>,
    lo: f64,
    hi: f64,
    ranged: bool,
}

fn tokens(line: &str) -> Vec<Tok> {
    let f = |m: Option<regex::Match>| m.and_then(|x| x.as_str().parse::<f64>().ok());
    re_num()
        .captures_iter(line)
        .filter_map(|c| {
            if let (Some(v), Some(lo), Some(hi)) = (f(c.get(1)), f(c.get(2)), f(c.get(3))) {
                Some(Tok { value: Some(v), lo, hi, ranged: true })
            } else if let (Some(lo), Some(hi)) = (f(c.get(4)), f(c.get(5))) {
                Some(Tok { value: None, lo, hi, ranged: true })
            } else {
                f(c.get(6)).map(|v| Tok { value: Some(v), lo: v, hi: v, ranged: false })
            }
        })
        .collect()
}

/// L'objet est-il compatible avec le gabarit d'un affixe ? Toutes les valeurs doivent coller, position par position :
/// plage annoncée = plage identique ; valeur seule = comprise dans la plage du gabarit.
fn fits(item: &[Tok], tmpl: &[Tok]) -> bool {
    item.len() == tmpl.len()
        && item.iter().zip(tmpl).all(|(i, t)| {
            if i.ranged {
                (i.lo - t.lo).abs() < 1e-9 && (i.hi - t.hi).abs() < 1e-9
            } else {
                i.value.map_or(false, |v| v >= t.lo - 1e-9 && v <= t.hi + 1e-9)
            }
        })
}

pub fn resolve(parsed: &ParsedItem, bp: &BasePool, fallback_ilvl: u8) -> Result<ResolvedItem, String> {
    let rarity = parsed.rarity.ok_or("rareté non supportée (seuls Normal / Magique / Rare sont craftables)")?;
    let ilvl = parsed.item_level.unwrap_or(fallback_ilvl);
    let mut item = ItemState::new(rarity, ilvl);
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();

    // index : première ligne normalisée du gabarit -> candidats
    let mut idx: HashMap<String, Vec<u16>> = HashMap::new();
    for (i, a) in bp.pool.affixes.iter().enumerate() {
        let first = a.text.lines().next().unwrap_or("");
        idx.entry(normalize(first)).or_default().push(i as u16);
    }

    for pm in parsed.mods.iter().filter(|m| m.kind == ModKind::Explicit) {
        let Some(line) = pm.lines.first() else { continue };
        let Some(cands) = idx.get(&normalize(line)) else {
            unmatched.push(line.clone());
            continue;
        };
        let mut cands: Vec<u16> = cands.clone();
        if let Some(s) = pm.slot {
            cands.retain(|&c| bp.pool.affixes[c as usize].slot == s);
        }
        if let Some(name) = &pm.name {
            let by_name: Vec<u16> = cands.iter().copied().filter(|&c| bp.pool.affixes[c as usize].name.eq_ignore_ascii_case(name)).collect();
            if !by_name.is_empty() {
                cands = by_name;
            }
        }
        let chosen = if cands.len() == 1 {
            Some(cands[0]) // nom + slot suffisent : on accepte même si les plages du dataset ont légèrement changé
        } else {
            let item_toks = tokens(line);
            let ok: Vec<u16> = cands
                .iter()
                .copied()
                .filter(|&c| fits(&item_toks, &tokens(bp.pool.affixes[c as usize].text.lines().next().unwrap_or(""))))
                .collect();
            match ok.as_slice() {
                [one] => Some(*one),
                [] => None,
                many => pm.tier.and_then(|t| many.iter().copied().find(|&c| bp.pool.affixes[c as usize].tier == t)).or(Some(many[0])),
            }
        };
        match chosen {
            Some(c) if item.len() < 6 => {
                let a = &bp.pool.affixes[c as usize];
                item.push(Mod { idx: c, fractured: pm.fractured });
                matched.push(MatchedMod { affix_idx: c, tier: a.tier, name: a.name.clone(), text: a.text.clone(), slot: a.slot, fractured: pm.fractured });
            }
            _ => unmatched.push(line.clone()),
        }
    }
    Ok(ResolvedItem { item, matched, unmatched })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::Dataset;

    fn render_value(text: &str) -> String {
        // remplace la première plage « (a-b) » par « mid(a-b) » : format avancé du jeu
        let re = Regex::new(r"\((\d+)-(\d+)\)").unwrap();
        re.replace(text, |c: &regex::Captures| {
            let (a, b): (u32, u32) = (c[1].parse().unwrap(), c[2].parse().unwrap());
            format!("{}({}-{})", (a + b) / 2, a, b)
        })
        .to_string()
    }

    fn sample(bp: &BasePool) -> (String, String, Vec<u16>) {
        let pick = |key: &str, tier: u8| bp.groups.iter().find(|g| g.key == key).unwrap().tiers.iter().find(|t| t.tier == tier).unwrap().clone();
        let (life, fire, evas) = (pick("life_flat", 3), pick("fire_res", 2), pick("evasion_pct", 4));
        let adv = format!(
            "Item Class: Gloves\nRarity: Rare\nDoom Grip\nEvasion Gloves\n--------\nEvasion Rating: 190\n--------\nItem Level: 81\n--------\n\
             {{ Prefix Modifier \"{}\" (Tier: {}) — Life }}\n{}\n\
             {{ Suffix Modifier \"{}\" (Tier: {}) — Elemental, Fire, Resistance }}\n{}\n\
             {{ Fractured Prefix Modifier \"{}\" (Tier: {}) — Defences }}\n{}\n",
            life.name, life.tier, render_value(&life.text), fire.name, fire.tier, render_value(&fire.text), evas.name, evas.tier, render_value(&evas.text)
        );
        let basic = format!(
            "Item Class: Gloves\nRarity: Rare\nDoom Grip\nEvasion Gloves\n--------\nEvasion Rating: 190\n--------\nItem Level: 81\n--------\n{}\n{}\n{} (fractured)\n",
            render_value(&life.text),
            render_value(&fire.text),
            render_value(&evas.text)
        );
        (adv, basic, vec![life.affix_idx, fire.affix_idx, evas.affix_idx])
    }

    #[test]
    fn advanced_format_round_trip() {
        let ds = Dataset::embedded();
        let bp = ds.build_pool("gloves_dex").unwrap();
        let (adv, _, want) = sample(&bp);
        let p = parse_item(&adv).unwrap();
        assert!(p.advanced && p.rarity == Some(Rarity::Rare) && p.item_level == Some(81));
        assert_eq!(p.mods.iter().filter(|m| m.kind == ModKind::Explicit).count(), 3);
        let r = resolve(&p, &bp, 80).unwrap();
        assert!(r.unmatched.is_empty(), "{:?}", r.unmatched);
        let got: Vec<u16> = r.matched.iter().map(|m| m.affix_idx).collect();
        assert_eq!(got, want);
        assert!(r.matched[2].fractured);
        assert!(r.item.has_fractured());
    }

    #[test]
    fn basic_format_matches_by_value_range() {
        let ds = Dataset::embedded();
        let bp = ds.build_pool("gloves_dex").unwrap();
        let (_, basic, want) = sample(&bp);
        let p = parse_item(&basic).unwrap();
        assert!(!p.advanced);
        let r = resolve(&p, &bp, 80).unwrap();
        let got: Vec<u16> = r.matched.iter().map(|m| m.affix_idx).collect();
        assert_eq!(got, want, "non appariés : {:?}", r.unmatched);
    }

    #[test]
    fn rejects_non_items_and_unsupported_rarity() {
        assert!(parse_item("bonjour").is_err());
        let p = parse_item("Item Class: Gloves\nRarity: Unique\nX\nY\n--------\nItem Level: 80\n").unwrap();
        let ds = Dataset::embedded();
        assert!(resolve(&p, &ds.build_pool("gloves_dex").unwrap(), 80).is_err());
    }
}
