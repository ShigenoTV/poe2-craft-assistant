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
    /// origine des poids d'apparition (vide = poids du jeu, tous égaux en PoE2)
    #[serde(default)]
    pub weights_source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaseItem {
    pub id: String,
    pub name: String,
    pub item_class: String,
    pub tags: Vec<String>,
    /// Textes des implicites propres à cette base (bijoux : chaque vraie base est importée avec le
    /// sien). Affichage et reconnaissance d'un objet collé seulement : un implicite n'occupe aucun
    /// emplacement de préfixe/suffixe et ne change pas le pool d'affixes.
    #[serde(default)]
    pub implicits: Vec<String>,
    /// Décalage du nombre de préfixes / suffixes autorisés venant de l'implicite (ex. Dusk Ring : 1 / -1,
    /// Absent Amulet : -1 / -1). 0 sur une base ordinaire.
    #[serde(default)]
    pub prefix_cap_delta: i8,
    #[serde(default)]
    pub suffix_cap_delta: i8,
    /// (max préfixes, max suffixes) d'un objet Rare de cette base avant décalage, quand il diffère du 3/3
    /// habituel : [2, 2] pour tous les joyaux, Time-Lost compris (Path of Building, Item.lua : `affixLimit`
    /// 4 pour un Rare de type Jewel). Le plafond Magique (1/1) n'est pas concerné.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rare_cap: Option<[u8; 2]>,
    /// Base Craft of Exile dont cette base prend les poids d'apparition (`ModDef::weights`). Absent :
    /// pas de poids connus, la base garde ceux du jeu (tous égaux).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight_key: Option<String>,
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
    /// Poids estimés par Craft of Exile, par `BaseItem::weight_key` ; « * » = valeur par défaut. Remplace
    /// le poids du jeu (toujours 1 en PoE2) sur une base qui a un `weight_key`, seulement là où le jeu
    /// autorise le mod (poids > 0).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub weights: BTreeMap<String, u32>,
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
    /// Bases autorisées (au moins un de ces tags) ; vide = toutes. Ex. Rib = « Desecrates a Rare Armour ».
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_tags: Vec<String>,
}
fn yes() -> bool {
    true
}

/// Une cible d'Essence pour une catégorie d'objet donnée : le premier `item_tags` qui matche au
/// moins un tag de la base (même règle que le poids de spawn des mods) fixe le mod garanti.
/// Une entrée « a&b » exige TOUS ces tags (ex. le Diamond, seul joyau à porter strjewel, dexjewel et
/// intjewel, placé avant la cible du Ruby).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EssenceTarget {
    pub item_tags: Vec<String>,
    /// identifiant d'un `ModDef` déjà présent dans `mods` (l'Essence garantit CE mod précis, au tier
    /// que sa valeur réelle représente — pas un nouvel affixe inventé). Vide : l'Essence ne
    /// s'applique PAS à cette catégorie (ex. la plupart des Liquid Emotions sur un Diamond).
    pub mod_id: String,
}

impl EssenceTarget {
    pub fn matches(&self, base_tags: &[String]) -> bool {
        self.item_tags.iter().any(|t| t.split('&').all(|part| base_tags.iter().any(|bt| bt == part)))
    }
}

/// Recette d'instillation d'amulette (The Withered Willow) : trois Liquid Emotions, dans cet ordre,
/// enchantent l'amulette avec ce passif. N'occupe ni préfixe ni suffixe.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct InstillDef {
    /// identifiant du passif dans l'arbre du jeu (deux passifs peuvent porter le même nom)
    pub skill: u32,
    pub name: String,
    #[serde(default)]
    pub stats: Vec<String>,
    /// `price_id` des trois émotions, dans l'ordre de la recette
    pub emotions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EssenceDef {
    pub id: String,
    pub label: String,
    pub price_id: String,
    #[serde(default = "yes")]
    pub default_enabled: bool,
    /// `false` (Essence normale) : Magique → Rare. `true` (Essence Perfect, Alloy Verisium) : objet
    /// déjà Rare, retire un mod au hasard puis ajoute l'affixe garanti.
    #[serde(default)]
    pub requires_rare: bool,
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
    /// recettes d'instillation d'amulette (voir `tools/import_liquid_emotions.mjs`)
    #[serde(default)]
    pub instills: Vec<InstillDef>,
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
    /// groupe d'exclusion (deux affixes du même groupe ne coexistent jamais)
    pub group: GroupId,
    /// sous-famille dans ce groupe : chaque famille a ses propres tiers et sa propre entrée d'objectif
    pub family_id: u16,
    /// clé d'objectif : clé brute du groupe, suivie de « ::famille » si le groupe en a plusieurs sur cette base
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

impl BaseItem {
    /// Pool d'affixes de cette base avec son plafond propre (Rare de la classe, décalage de l'implicite).
    pub fn affix_pool(&self, affixes: Vec<Affix>) -> AffixPool {
        AffixPool {
            affixes,
            cap_delta: (self.prefix_cap_delta, self.suffix_cap_delta),
            rare_cap: self.rare_cap.map_or(Rarity::Rare.cap(), |[p, s]| (p, s)),
        }
    }
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
            for t in e.targets.iter().filter(|t| !t.mod_id.is_empty()) {
                if !mod_ids.contains(t.mod_id.as_str()) {
                    return Err(format!("essence « {} » : mod_id inconnu « {} »", e.id, t.mod_id));
                }
            }
        }
        for i in &self.instills {
            if i.emotions.len() != 3 || i.emotions.iter().any(|e| !self.prices.contains_key(e)) {
                return Err(format!("instillation « {} » : il faut trois émotions ayant un prix ({:?})", i.name, i.emotions));
            }
        }
        Ok(())
    }

    /// Recette d'instillation du passif `skill` et son coût (somme des trois émotions aux `prices`).
    pub fn instill_cost(&self, skill: u32, prices: &BTreeMap<String, f64>) -> Result<(&InstillDef, f64), String> {
        let i = self.instills.iter().find(|i| i.skill == skill).ok_or_else(|| format!("instillation inconnue : {skill}"))?;
        let cost = i.emotions.iter().map(|e| prices.get(e).copied().ok_or_else(|| format!("prix manquant : {e}"))).sum::<Result<f64, String>>()?;
        Ok((i, cost))
    }

    pub fn base(&self, id: &str) -> Option<&BaseItem> {
        self.bases.iter().find(|b| b.id == id)
    }

    /// Construit le pool d'affixes d'une base : poids résolus par la règle du premier tag correspondant,
    /// tiers numérotés au sein de chaque groupe (1 = niveau le plus haut).
    pub fn build_pool(&self, base_id: &str) -> Result<BasePool, String> {
        let base = self.base(base_id).ok_or_else(|| format!("base inconnue : {base_id}"))?.clone();
        let tag_bit: HashMap<&str, u64> = self.tags.iter().enumerate().map(|(i, t)| (t.as_str(), 1u64 << i)).collect();
        // certains mods (cibles d'Alloy/Essence exclusives, ex. AlloyMaximumElementalInfusions1) ont un
        // poids nul PARTOUT dans les vraies données du jeu : jamais tirés au hasard, uniquement obtenus
        // via une monnaie qui les ajoute de force. Il faut quand même leur réserver une entrée dans le
        // pool (poids 0, donc jamais piochée normalement) pour que la résolution de cible les trouve —
        // mais UNIQUEMENT sur les bases où ils sont vraiment une cible (via `item_tags`), sinon un mod
        // comme « +Vie » se retrouverait listé même sur une baguette, qui n'en a jamais en vrai jeu.
        // Seule la PREMIÈRE cible qui matche compte (même règle qu'`essence_currencies`) : un arc porte
        // aussi le tag `two_hand_weapon`, il ne doit pas recevoir en plus la variante « deux mains ».
        let essence_target_ids: HashSet<&str> = self
            .essences
            .iter()
            .filter_map(|e| e.targets.iter().find(|t| t.matches(&base.tags)))
            .map(|t| t.mod_id.as_str())
            .filter(|id| !id.is_empty())
            .collect();

        let mut by_group: BTreeMap<&str, Vec<(&ModDef, u32)>> = BTreeMap::new();
        for m in &self.mods {
            let w = m
                .spawn
                .iter()
                .find(|s| s.tag == "default" || base.tags.iter().any(|t| *t == s.tag))
                .map(|s| s.weight)
                .unwrap_or(0);
            let w = match &base.weight_key {
                Some(k) if w > 0 => m.weights.get(k).or_else(|| m.weights.get("*")).copied().unwrap_or(w),
                _ => w,
            };
            if w > 0 || essence_target_ids.contains(m.id.as_str()) {
                by_group.entry(m.group.as_str()).or_default().push((m, w));
            }
        }

        let mut affixes = Vec::new();
        let mut groups = Vec::new();
        for (gi, (key, list)) in by_group.into_iter().enumerate() {
            let gid = gi as GroupId;
            // familles du groupe (champ `family` du mod = son `type` dans le jeu), dans l'ordre d'apparition
            let mut families: Vec<(&str, Vec<(&ModDef, u32)>)> = Vec::new();
            for (m, w) in list {
                match families.iter_mut().find(|(f, _)| *f == m.family.as_str()) {
                    Some((_, v)) => v.push((m, w)),
                    None => families.push((m.family.as_str(), vec![(m, w)])),
                }
            }
            let several = families.len() > 1;
            for (fi, (fam, mut members)) in families.into_iter().enumerate() {
                members.sort_by(|a, b| b.0.level.cmp(&a.0.level));
                let mut tiers = Vec::new();
                for (ti, (m, w)) in members.iter().enumerate() {
                    let idx = affixes.len() as u16;
                    affixes.push(Affix {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        family: m.family.clone(),
                        text: m.text.clone(),
                        group: gid,
                        family_id: fi as u16,
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
                    family_id: fi as u16,
                    key: if several { format!("{key}::{fam}") } else { key.to_string() },
                    family: fam.to_string(),
                    slot: members[0].0.slot,
                    total_weight: tiers.iter().map(|t| t.weight).sum(),
                    tiers,
                });
            }
        }
        groups.sort_by(|a, b| (a.slot as u8, &a.family).cmp(&(b.slot as u8, &b.family)));
        Ok(BasePool { pool: base.affix_pool(affixes), base, groups })
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
                    requires_rare: false,
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
                    requires_rare: false,
                    unit_cost: base_price + price(&o.price_id)?,
                });
            }
        }
        Ok(out)
    }

    /// L'action `id` (monnaie, éventuellement « monnaie+Omen ») peut-elle s'utiliser sur cette base ?
    /// Seules les monnaies à `item_tags` sont restreintes (os Abyssaux : armure, bijou, arme).
    pub fn currency_applies(&self, id: &str, base: &BaseItem) -> bool {
        let cid = id.split('+').next().unwrap_or(id);
        self.currencies.iter().find(|c| c.id == cid).map_or(true, |c| c.item_tags.is_empty() || c.item_tags.iter().any(|t| base.tags.contains(t)))
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
            let Some(t) = e.targets.iter().find(|t| t.matches(&bp.base.tags)).filter(|t| !t.mod_id.is_empty()) else {
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
                requires_rare: e.requires_rare,
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

    /// Chaque vraie base de bijou (RePoE 4.5.5.2 : Anneau 28, Amulette 25, Ceinture 19, Carquois 11) est
    /// importée avec son implicite ; les 13 qui changent le plafond de préfixes/suffixes le portent.
    #[test]
    fn every_real_jewellery_base_is_imported_with_its_implicit() {
        let ds = Dataset::embedded();
        let count = |cls: &str| ds.bases.iter().filter(|b| b.item_class == cls).count();
        assert_eq!(count("Ring"), 28);
        assert_eq!(count("Amulet"), 25);
        assert_eq!(count("Belt"), 19);
        assert_eq!(count("Quiver"), 11);
        let mut ids = HashSet::new();
        for b in ds.bases.iter().filter(|b| ["Ring", "Amulet", "Belt", "Quiver"].contains(&b.item_class.as_str())) {
            assert!(ids.insert(&b.id), "identifiant dupliqué : {}", b.id);
            // seule exception : la base « Ring » (FourRingBase) n'a aucun implicite dans les données du jeu
            assert!(!b.implicits.is_empty() || b.id == "ring", "{} sans implicite", b.id);
            assert!(b.implicits.iter().all(|t| !t.contains('[')), "{}: {:?}", b.id, b.implicits);
            // décalage de plafond présent si et seulement si l'implicite le dit
            let says = b.implicits.iter().any(|t| t.contains("Modifier allowed") || t.contains("Modifiers allowed"));
            assert_eq!(says, (b.prefix_cap_delta, b.suffix_cap_delta) != (0, 0), "{}", b.id);
        }
        let caps: Vec<(&str, i8, i8)> = ds.bases.iter().filter(|b| (b.prefix_cap_delta, b.suffix_cap_delta) != (0, 0)).map(|b| (b.id.as_str(), b.prefix_cap_delta, b.suffix_cap_delta)).collect();
        assert_eq!(caps.len(), 13, "{caps:?}");
        for expected in [("dusk_ring", 1, -1), ("penumbra_amulet", 2, -2), ("absent_amulet", -1, -1), ("lament_amulet", -1, 0), ("distorted_amulet", 0, -1)] {
            assert!(caps.contains(&expected), "{expected:?} absent de {caps:?}");
        }
        assert_eq!(ds.build_pool("tenebrous_ring").unwrap().pool.cap(Rarity::Rare), (1, 5));
        let two_stone: Vec<_> = ds.bases.iter().filter(|b| b.name == "Two-Stone Ring").collect();
        assert_eq!(two_stone.len(), 3);
        assert!(two_stone.iter().any(|b| b.implicits == ["+(12-16)% to Fire and Cold Resistances"]));
        // un implicite ne change pas le pool : toutes les bases d'une même classe de bijou ont les mêmes groupes
        for cls in ["Ring", "Amulet", "Belt", "Quiver"] {
            let pools: Vec<Vec<String>> = ds
                .bases
                .iter()
                .filter(|b| b.item_class == cls)
                .map(|b| ds.build_pool(&b.id).unwrap().groups.iter().map(|g| g.key.clone()).collect())
                .collect();
            assert!(pools.windows(2).all(|w| w[0] == w[1]), "{cls} : pools différents selon la base");
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

    /// Poids Craft of Exile (poec_data.json, 2026-10-03) : « niveau de tous les sorts de feu » sur baguette
    /// = 1000/750/500/250/100 du T5 au T1, comme sur leur site ; une base sans poids connus (griffe) garde
    /// les poids du jeu ; et sur toute base pondérée, aucun mod normal n'est resté au poids 1 du jeu.
    #[test]
    fn craftofexile_weights_are_applied_per_base() {
        let ds = Dataset::embedded();
        assert!(ds.meta.weights_source.starts_with("craftofexile.com"));
        let wand = ds.build_pool("wand").unwrap();
        let fire = wand.groups.iter().find(|g| g.family == "+# to Level of all Fire Spell Skills").unwrap();
        assert_eq!(fire.tiers.iter().map(|t| t.weight).collect::<Vec<_>>(), vec![100, 250, 500, 750, 1000]);
        // Strength : 1000 sur une ceinture, 500 sur des gants str/dex, 250 sur un sceptre
        let str1 = |base: &str| ds.build_pool(base).unwrap().pool.affixes.iter().find(|a| a.id == "Strength1").map(|a| a.weight);
        assert_eq!((str1("double_belt"), str1("gloves_str_dex"), str1("sceptre")), (Some(1000), Some(500), Some(250)));
        let claw = ds.build_pool("claw").unwrap();
        assert!(claw.pool.affixes.iter().all(|a| a.weight <= 1), "griffe : pas de poids connus, poids du jeu");
        for b in ds.bases.iter().filter(|b| b.weight_key.is_some()) {
            let bp = ds.build_pool(&b.id).unwrap();
            let stale: Vec<_> = bp.pool.affixes.iter().filter(|a| !a.desecrated && a.weight == 1).map(|a| a.id.as_str()).collect();
            assert!(stale.is_empty(), "{} : mods restés au poids 1 du jeu : {stale:?}", b.id);
        }
    }
}
