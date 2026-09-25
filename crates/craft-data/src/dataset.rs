use craft_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

const EMBEDDED: &str = include_str!("../../../data/sample/dataset.json");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub schema: u32,
    pub source: String,
    #[serde(default)]
    pub game_version: String,
    #[serde(default)]
    pub generated_at: String,
    #[serde(default)]
    pub notice: String,
    #[serde(default)]
    pub price_unit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaseItem {
    pub id: String,
    pub name: String,
    pub item_class: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpawnWeight {
    pub tag: String,
    pub weight: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModDef {
    pub id: String,
    pub group: String,
    pub family: String,
    pub name: String,
    pub slot: Slot,
    pub level: u8,
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Convention PoE : le PREMIER `tag` présent sur la base fixe le poids ("default" en dernier).
    pub spawn: Vec<SpawnWeight>,
    /// Domaine `desecrated` du jeu : jamais tirable par une monnaie normale, uniquement par
    /// `CurrencyKind::Desecrate` (voir `docs/DATA.md`).
    #[serde(default)]
    pub desecrated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CurrencyDef {
    pub id: String,
    pub label: String,
    pub kind: CurrencyKind,
    #[serde(default)]
    pub min_mod_level: u8,
    pub price_id: String,
    #[serde(default = "yes")]
    pub default_enabled: bool,
}
fn yes() -> bool {
    true
}

/// Une cible d'Essence pour une catégorie d'objet donnée : le premier `item_tags` qui matche au
/// moins un tag de la base (même règle que le poids de spawn des mods) fixe le mod garanti.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EssenceTarget {
    pub item_tags: Vec<String>,
    /// identifiant d'un `ModDef` déjà présent dans `mods` (l'Essence garantit CE mod précis, au tier
    /// que sa valeur réelle représente — pas un nouvel affixe inventé).
    pub mod_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EssenceDef {
    pub id: String,
    pub label: String,
    pub price_id: String,
    #[serde(default = "yes")]
    pub default_enabled: bool,
    pub targets: Vec<EssenceTarget>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OmenDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub add_slot: Option<Slot>,
    #[serde(default)]
    pub remove_slot: Option<Slot>,
    pub applies_to: Vec<CurrencyKind>,
    pub price_id: String,
    /// Omen « the Sovereign/Liege/Blackblooded » : nom d'un tag (résolu via `Dataset.tags`) qui
    /// restreint la Désécration à un sous-pool. Ignoré pour tout autre `CurrencyKind`.
    #[serde(default)]
    pub require_tag: Option<String>,
    /// Omen of Light : le retrait ne peut cibler qu'un affixe `desecrated`.
    #[serde(default)]
    pub remove_desecrated_only: bool,
    /// Omen of Whittling : le retrait cible toujours l'affixe tenu du niveau requis le plus bas.
    #[serde(default)]
    pub remove_lowest_level: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dataset {
    pub meta: Meta,
    #[serde(default)]
    pub tags: Vec<String>,
    pub bases: Vec<BaseItem>,
    pub mods: Vec<ModDef>,
    pub currencies: Vec<CurrencyDef>,
    #[serde(default)]
    pub essences: Vec<EssenceDef>,
    #[serde(default)]
    pub omens: Vec<OmenDef>,
    #[serde(default)]
    pub prices: BTreeMap<String, f64>,
    /// price_id -> objet poe.ninja correspondant (absent = prix saisi à la main uniquement)
    #[serde(default)]
    pub price_sources: BTreeMap<String, crate::prices::PriceSource>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierInfo {
    pub tier: u8,
    pub level: u8,
    pub weight: u32,
    pub name: String,
    pub text: String,
    pub affix_idx: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfo {
    pub group: GroupId,
    pub key: String,
    pub family: String,
    pub slot: Slot,
    pub total_weight: u32,
    pub tiers: Vec<TierInfo>,
}

#[derive(Clone, Debug)]
pub struct BasePool {
    pub base: BaseItem,
    pub pool: AffixPool,
    pub groups: Vec<GroupInfo>,
}

impl Dataset {
    pub fn embedded() -> Self {
        Self::from_json(EMBEDDED).expect("dataset embarqué invalide")
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let ds: Dataset = serde_json::from_str(s).map_err(|e| format!("dataset invalide : {e}"))?;
        ds.validate()?;
        Ok(ds)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.meta.schema != 1 {
            return Err(format!("schéma {} non supporté (attendu 1)", self.meta.schema));
        }
        if self.tags.len() > 64 {
            return Err("plus de 64 tags".into());
        }
        let mut seen = HashSet::new();
        for m in &self.mods {
            if !seen.insert(&m.id) {
                return Err(format!("identifiant de mod dupliqué : {}", m.id));
            }
        }
        if self.bases.is_empty() || self.mods.is_empty() {
            return Err("dataset vide".into());
        }
        let mod_ids: HashSet<&str> = self.mods.iter().map(|m| m.id.as_str()).collect();
        for e in &self.essences {
            for t in &e.targets {
                if !mod_ids.contains(t.mod_id.as_str()) {
                    return Err(format!("essence « {} » : mod_id inconnu « {} »", e.id, t.mod_id));
                }
            }
        }
        Ok(())
    }

    pub fn base(&self, id: &str) -> Option<&BaseItem> {
        self.bases.iter().find(|b| b.id == id)
    }

    /// Construit le pool d'affixes d'une base : poids résolus par la règle du premier tag correspondant,
    /// tiers numérotés au sein de chaque groupe (1 = niveau le plus haut).
    pub fn build_pool(&self, base_id: &str) -> Result<BasePool, String> {
        let base = self.base(base_id).ok_or_else(|| format!("base inconnue : {base_id}"))?.clone();
        let tag_bit: HashMap<&str, u64> = self.tags.iter().enumerate().map(|(i, t)| (t.as_str(), 1u64 << i)).collect();

        let mut by_group: BTreeMap<&str, Vec<(&ModDef, u32)>> = BTreeMap::new();
        for m in &self.mods {
            let w = m
                .spawn
                .iter()
                .find(|s| s.tag == "default" || base.tags.iter().any(|t| *t == s.tag))
                .map(|s| s.weight)
                .unwrap_or(0);
            if w > 0 {
                by_group.entry(m.group.as_str()).or_default().push((m, w));
            }
        }

        let mut affixes = Vec::new();
        let mut groups = Vec::new();
        for (gi, (key, mut list)) in by_group.into_iter().enumerate() {
            list.sort_by(|a, b| b.0.level.cmp(&a.0.level));
            let gid = gi as GroupId;
            let mut tiers = Vec::new();
            for (ti, (m, w)) in list.iter().enumerate() {
                let idx = affixes.len() as u16;
                affixes.push(Affix {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    family: m.family.clone(),
                    text: m.text.clone(),
                    group: gid,
                    slot: m.slot,
                    tier: ti as u8 + 1,
                    req_ilvl: m.level,
                    weight: *w,
                    tags: m.tags.iter().filter_map(|t| tag_bit.get(t.as_str())).fold(0, |a, b| a | b),
                    desecrated: m.desecrated,
                });
                tiers.push(TierInfo { tier: ti as u8 + 1, level: m.level, weight: *w, name: m.name.clone(), text: m.text.clone(), affix_idx: idx });
            }
            groups.push(GroupInfo {
                group: gid,
                key: key.to_string(),
                family: list[0].0.family.clone(),
                slot: list[0].0.slot,
                total_weight: tiers.iter().map(|t| t.weight).sum(),
                tiers,
            });
        }
        groups.sort_by(|a, b| (a.slot as u8, &a.family).cmp(&(b.slot as u8, &b.family)));
        Ok(BasePool { base, pool: AffixPool { affixes }, groups })
    }

    /// Liste des actions de craft (monnaies × Omens compatibles) avec coûts issus de `prices`.
    pub fn actions(&self, prices: &BTreeMap<String, f64>, enabled: Option<&HashSet<String>>) -> Result<Vec<Currency>, String> {
        let price = |id: &str| prices.get(id).copied().ok_or_else(|| format!("prix manquant : {id}"));
        let tag_bit: HashMap<&str, u64> = self.tags.iter().enumerate().map(|(i, t)| (t.as_str(), 1u64 << i)).collect();
        let mut out = Vec::new();
        for c in &self.currencies {
            let base_price = price(&c.price_id)?;
            if enabled.map_or(c.default_enabled, |e| e.contains(&c.id)) {
                out.push(Currency {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    kind: c.kind,
                    min_mod_level: c.min_mod_level,
                    add_slot: None,
                    remove_slot: None,
                    target: None,
                    require_tag: None,
                    remove_desecrated_only: false,
                    remove_lowest_level: false,
                    unit_cost: base_price,
                });
            }
            for o in self.omens.iter().filter(|o| o.applies_to.contains(&c.kind)) {
                let oid = format!("{}+{}", c.id, o.id);
                if !enabled.map_or(c.default_enabled, |e| e.contains(&oid)) {
                    continue;
                }
                out.push(Currency {
                    id: oid,
                    label: format!("{} + {}", c.label, o.label),
                    kind: c.kind,
                    min_mod_level: c.min_mod_level,
                    add_slot: o.add_slot,
                    remove_slot: o.remove_slot,
                    target: None,
                    require_tag: o.require_tag.as_deref().and_then(|t| tag_bit.get(t)).copied(),
                    remove_desecrated_only: o.remove_desecrated_only,
                    remove_lowest_level: o.remove_lowest_level,
                    unit_cost: base_price + price(&o.price_id)?,
                });
            }
        }
        Ok(out)
    }

    /// Actions Essence pour une base donnée : résout, pour chaque Essence, le premier `target` dont
    /// `item_tags` recoupe les tags de la base, puis retrouve l'affixe garanti dans le pool DÉJÀ
    /// CONSTRUIT de cette base (donc jamais dans la liste base-indépendante d'`actions()`).
    /// Une Essence sans cible correspondante, ou dont le mod garanti n'est pas dans le pool (poids nul
    /// pour cette base), est silencieusement omise plutôt que de fausser le craft.
    pub fn essence_currencies(&self, bp: &BasePool, prices: &BTreeMap<String, f64>, enabled: Option<&HashSet<String>>) -> Result<Vec<Currency>, String> {
        let price = |id: &str| prices.get(id).copied().ok_or_else(|| format!("prix manquant : {id}"));
        let mut out = Vec::new();
        for e in &self.essences {
            if !enabled.map_or(e.default_enabled, |en| en.contains(&e.id)) {
                continue;
            }
            let Some(t) = e.targets.iter().find(|t| t.item_tags.iter().any(|tag| bp.base.tags.iter().any(|bt| bt == tag))) else {
                continue;
            };
            let Some(idx) = bp.pool.affixes.iter().position(|a| a.id == t.mod_id) else {
                continue;
            };
            out.push(Currency {
                id: e.id.clone(),
                label: e.label.clone(),
                kind: CurrencyKind::Essence,
                min_mod_level: 0,
                add_slot: None,
                remove_slot: None,
                target: Some(idx as AffixIdx),
                require_tag: None,
                remove_desecrated_only: false,
                remove_lowest_level: false,
                unit_cost: price(&e.price_id)?,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_dataset_loads_and_pools_are_consistent() {
        let ds = Dataset::embedded();
        for b in &ds.bases {
            let bp = ds.build_pool(&b.id).unwrap();
            assert!(bp.pool.affixes.len() > 20, "{}: {} affixes", b.id, bp.pool.affixes.len());
            for g in &bp.groups {
                // tiers strictement décroissants en niveau, numérotés 1..n
                for (i, t) in g.tiers.iter().enumerate() {
                    assert_eq!(t.tier as usize, i + 1);
                    if i > 0 {
                        assert!(g.tiers[i - 1].level >= t.level);
                    }
                }
            }
        }
    }

    #[test]
    fn first_matching_tag_rule_and_zero_weight_exclusion() {
        let ds = Dataset::embedded();
        let gloves = ds.build_pool("gloves_dex").unwrap();
        let wand = ds.build_pool("wand").unwrap();
        // les mods de vie n'existent pas sur les armes dans ce jeu de données ; les dégâts de sort sont propres à la baguette
        assert!(gloves.groups.iter().any(|g| g.key == "IncreasedLife"));
        assert!(!wand.groups.iter().any(|g| g.key == "IncreasedLife"));
        assert!(wand.groups.iter().any(|g| g.key == "SpellDamageAndMana"), "{:?}", wand.groups.iter().map(|g| &g.key).collect::<Vec<_>>());
        assert!(!gloves.groups.iter().any(|g| g.key == "SpellDamageAndMana"));
    }

    #[test]
    fn actions_combine_currencies_and_omens() {
        let ds = Dataset::embedded();
        let acts = ds.actions(&ds.prices, None).unwrap();
        let combo = acts.iter().find(|a| a.id == "exalt_perfect+omen_dextral_exaltation").unwrap();
        assert_eq!(combo.add_slot, Some(Slot::Suffix));
        assert!((combo.unit_cost - (ds.prices["exalt_perfect"] + ds.prices["omen_dextral_exaltation"])).abs() < 1e-9);
        assert!(acts.iter().any(|a| a.id == "annul+omen_sinistral_annulment" && a.remove_slot == Some(Slot::Prefix)));
    }

    #[test]
    fn essence_currencies_resolves_the_right_target_by_base_tags() {
        let ds = Dataset::embedded();
        let sword = ds.build_pool("sword_1h").unwrap();
        let acts = ds.essence_currencies(&sword, &ds.prices, None).unwrap();
        let e = acts.iter().find(|c| c.id == "essence_abrasion").expect("essence_abrasion doit s'appliquer à une épée une main (tag one_hand_weapon)");
        assert_eq!(e.kind, CurrencyKind::Essence);
        let idx = e.target.expect("une Essence résolue doit avoir une cible");
        assert_eq!(sword.pool.affixes[idx as usize].id, "LocalAddedPhysicalDamage5");

        // une baguette (arme de lanceur de sort, pas de dégâts physiques plats) ne doit RIEN résoudre
        let wand = ds.build_pool("wand").unwrap();
        let wand_acts = ds.essence_currencies(&wand, &ds.prices, None).unwrap();
        assert!(wand_acts.iter().all(|c| c.id != "essence_abrasion"), "l'Essence d'Abrasion ne doit pas s'appliquer à une baguette");
    }
}
