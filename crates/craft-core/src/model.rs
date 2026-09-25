use rand::Rng;
use serde::{Deserialize, Serialize};

pub type AffixIdx = u16; // index dans AffixPool.affixes
pub type GroupId = u16; // deux affixes du même groupe ne peuvent pas coexister

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Slot {
    Prefix,
    Suffix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
}

impl Rarity {
    /// (max préfixes, max suffixes)
    pub const fn cap(self) -> (u8, u8) {
        match self {
            Rarity::Normal => (0, 0),
            Rarity::Magic => (1, 1),
            Rarity::Rare => (3, 3),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Affix {
    pub id: String,
    pub name: String,   // "Sturdy"
    pub family: String, // libellé lisible du groupe, ex. "Vie maximale"
    pub text: String,   // gabarit avec plages, ex. "+(40-49) to maximum Life"
    pub group: GroupId,
    pub slot: Slot,
    pub tier: u8,      // 1 = meilleur
    pub req_ilvl: u8,  // niveau de modificateur (ilvl requis)
    pub weight: u32,   // poids de spawn résolu pour CETTE base
    pub tags: u64,     // bitmask de tags
    /// Domaine « desecrated » : jamais tirable par une monnaie normale (Transmute, Chaos, Exalt, ...),
    /// uniquement par `CurrencyKind::Desecrate`. Empêche par construction le bug qu'on avait repéré :
    /// laisser ces mods fuiter dans le pool normal fausserait silencieusement toutes les probabilités.
    #[serde(default)]
    pub desecrated: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mod {
    pub idx: AffixIdx,
    pub fractured: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ItemState {
    pub rarity: Rarity,
    pub ilvl: u8,
    mods: [Mod; 6],
    len: u8,
}

impl ItemState {
    pub fn new(rarity: Rarity, ilvl: u8) -> Self {
        Self { rarity, ilvl, mods: [Mod::default(); 6], len: 0 }
    }
    pub fn mods(&self) -> &[Mod] {
        &self.mods[..self.len as usize]
    }
    pub fn len(&self) -> usize {
        self.len as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn has_fractured(&self) -> bool {
        self.mods().iter().any(|m| m.fractured)
    }
    pub fn push(&mut self, m: Mod) {
        debug_assert!(self.len < 6);
        if self.len < 6 {
            self.mods[self.len as usize] = m;
            self.len += 1;
        }
    }
    pub fn remove(&mut self, pos: usize) {
        let l = self.len as usize;
        self.mods.copy_within(pos + 1..l, pos);
        self.len -= 1;
    }
    pub fn set_fractured(&mut self, pos: usize) {
        self.mods[pos].fractured = true;
    }
}

// ───────────────────────── Monnaies ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CurrencyKind {
    Transmute,
    Augment,
    Regal,
    Alchemy,
    Exalt,
    Chaos,
    Annul,
    Fracture,
    /// Essence : transforme un objet Magique en Rare en ajoutant un affixe GARANTI (`Currency.target`),
    /// pas un tirage pondéré. Retenu uniquement si l'affixe cible respecte la place de slot disponible
    /// et n'entre pas en conflit de groupe avec un mod déjà présent (sinon `NotApplicable`).
    Essence,
    /// Désécration : ajoute un mod « Désécré » non révélé (retire un mod au hasard si l'objet est plein
    /// à 6). Tiré UNIQUEMENT dans le sous-pool `desecrated` (jamais le pool normal). Un objet portant
    /// déjà un mod Désécré ne peut pas l'être une deuxième fois.
    Desecrate,
}

/// Une « action de craft » : monnaie (éventuellement Greater/Perfect) + Omen éventuel.
/// Les règles exactes sont pilotées par la donnée ; ce noyau n'en code que la mécanique générique.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Currency {
    pub id: String,
    pub label: String,
    pub kind: CurrencyKind,
    #[serde(default)]
    pub min_mod_level: u8,
    /// Omen « Sinistral/Dextral Exaltation » : force le slot de l'affixe ajouté.
    #[serde(default)]
    pub add_slot: Option<Slot>,
    /// Omen « Sinistral/Dextral Annulment/Erasure » : restreint le retrait à un slot.
    #[serde(default)]
    pub remove_slot: Option<Slot>,
    /// Affixe garanti pour `CurrencyKind::Essence` — résolu par base (dépend du pool), donc absent
    /// pour toutes les autres monnaies et ignoré si `kind != Essence`.
    #[serde(default)]
    pub target: Option<AffixIdx>,
    /// Omen « the Sovereign/Liege/Blackblooded » : restreint la Désécration à un sous-pool (Ulaman /
    /// Amanamu / Kurgal). Ignoré pour tout autre `CurrencyKind`.
    #[serde(default)]
    pub require_tag: Option<u64>,
    /// Omen of Light : le retrait (Annulment) ne peut cibler qu'un affixe `desecrated`.
    #[serde(default)]
    pub remove_desecrated_only: bool,
    /// Omen of Whittling : le retrait cible TOUJOURS l'affixe tenu du niveau requis le plus bas
    /// (déterministe), pas un tirage uniforme parmi les candidats.
    #[serde(default)]
    pub remove_lowest_level: bool,
    /// `false` (Essence normale/Lesser/Greater) : Magique → Rare, ajoute l'affixe garanti.
    /// `true` (Essence Perfect, Alloy Verisium) : objet déjà Rare, retire un mod au hasard PUIS ajoute
    /// l'affixe garanti — jamais les deux comportements sur la même entrée, comme dans le vrai jeu.
    #[serde(default)]
    pub requires_rare: bool,
    pub unit_cost: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DrawFilter {
    pub min_mod_level: u8,
    pub force_slot: Option<Slot>,
    /// `false` (monnaies normales) : exclut les affixes `desecrated`. `true` (Désécration) : ne tire
    /// QUE parmi eux — jamais les deux pools mélangées.
    pub require_desecrated: bool,
    /// Omen « the Sovereign/Liege/Blackblooded » : restreint le tirage à un sous-pool via bitmask de tag.
    pub require_tag: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    NotApplicable,
}

// ───────────────────────── Pool d'affixes ─────────────────────────

#[derive(Clone, Debug)]
pub struct AffixPool {
    pub affixes: Vec<Affix>,
}

impl AffixPool {
    pub fn has_desecrated(&self, item: &ItemState) -> bool {
        item.mods().iter().any(|m| self.affixes[m.idx as usize].desecrated)
    }

    pub fn count(&self, item: &ItemState, slot: Slot) -> u8 {
        item.mods().iter().filter(|m| self.affixes[m.idx as usize].slot == slot).count() as u8
    }

    /// Tirage pondéré exact parmi les affixes éligibles dans l'état courant.
    pub fn draw(&self, item: &ItemState, f: &DrawFilter, rng: &mut impl Rng) -> Option<AffixIdx> {
        let (cap_p, cap_s) = item.rarity.cap();
        let open_p = self.count(item, Slot::Prefix) < cap_p;
        let open_s = self.count(item, Slot::Suffix) < cap_s;

        let mut held = [GroupId::MAX; 6];
        for (i, m) in item.mods().iter().enumerate() {
            held[i] = self.affixes[m.idx as usize].group;
        }

        let eligible = |a: &Affix| -> bool {
            a.weight > 0
                && a.desecrated == f.require_desecrated
                && a.req_ilvl <= item.ilvl
                && a.req_ilvl >= f.min_mod_level
                && f.require_tag.map_or(true, |t| a.tags & t != 0)
                && match a.slot {
                    Slot::Prefix => open_p,
                    Slot::Suffix => open_s,
                }
                && f.force_slot.map_or(true, |s| s == a.slot)
                && !held.contains(&a.group)
        };

        let total: u64 = self.affixes.iter().filter(|a| eligible(a)).map(|a| a.weight as u64).sum();
        if total == 0 {
            return None;
        }
        let mut roll = rng.gen_range(0..total);
        for (i, a) in self.affixes.iter().enumerate() {
            if !eligible(a) {
                continue;
            }
            let w = a.weight as u64;
            if roll < w {
                return Some(i as AffixIdx);
            }
            roll -= w;
        }
        None
    }

    fn add_random(&self, item: &mut ItemState, f: &DrawFilter, rng: &mut impl Rng) {
        if let Some(idx) = self.draw(item, f, rng) {
            item.push(Mod { idx, fractured: false });
        }
    }

    /// Retire un affixe non fracturé (filtré par slot si Omen). `false` si aucun candidat.
    pub(crate) fn remove_random(&self, item: &mut ItemState, slot: Option<Slot>, desecrated_only: bool, lowest_level: bool, rng: &mut impl Rng) -> bool {
        let cands: Vec<usize> = item
            .mods()
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                !m.fractured
                    && slot.map_or(true, |s| self.affixes[m.idx as usize].slot == s)
                    && (!desecrated_only || self.affixes[m.idx as usize].desecrated)
            })
            .map(|(i, _)| i)
            .collect();
        if cands.is_empty() {
            return false;
        }
        let pos = if lowest_level {
            // déterministe : l'affixe TENU du niveau requis le plus bas (égalité -> le premier trouvé)
            *cands.iter().min_by_key(|&&i| self.affixes[item.mods()[i].idx as usize].req_ilvl).unwrap()
        } else {
            cands[rng.gen_range(0..cands.len())]
        };
        item.remove(pos);
        true
    }

    pub fn apply(&self, item: &mut ItemState, c: &Currency, rng: &mut impl Rng) -> Outcome {
        use CurrencyKind::*;
        let f = DrawFilter { min_mod_level: c.min_mod_level, force_slot: c.add_slot, ..Default::default() };
        let n = item.len();
        match c.kind {
            Transmute if item.rarity == Rarity::Normal => {
                item.rarity = Rarity::Magic;
                self.add_random(item, &f, rng);
            }
            Augment if item.rarity == Rarity::Magic && n < 2 => self.add_random(item, &f, rng),
            Regal if item.rarity == Rarity::Magic => {
                item.rarity = Rarity::Rare;
                self.add_random(item, &f, rng);
            }
            Alchemy if item.rarity == Rarity::Normal => {
                item.rarity = Rarity::Rare;
                for _ in 0..4 {
                    self.add_random(item, &f, rng);
                }
            }
            Exalt if item.rarity == Rarity::Rare && n < 6 => self.add_random(item, &f, rng),
            Chaos if item.rarity == Rarity::Rare => {
                // retrait PUIS ajout : le pool du tirage est calculé après le retrait
                if !self.remove_random(item, c.remove_slot, c.remove_desecrated_only, c.remove_lowest_level, rng) {
                    return Outcome::NotApplicable;
                }
                self.add_random(item, &f, rng);
            }
            Annul if item.rarity != Rarity::Normal => {
                if !self.remove_random(item, c.remove_slot, c.remove_desecrated_only, c.remove_lowest_level, rng) {
                    return Outcome::NotApplicable;
                }
            }
            Fracture if item.rarity == Rarity::Rare && n >= 4 && !item.has_fractured() => {
                let pos = rng.gen_range(0..n);
                item.set_fractured(pos);
            }
            Essence if (item.rarity == Rarity::Magic && !c.requires_rare) || (item.rarity == Rarity::Rare && c.requires_rare) => {
                let Some(target) = c.target else { return Outcome::NotApplicable };
                if c.requires_rare && !self.remove_random(item, None, false, false, rng) {
                    return Outcome::NotApplicable;
                }
                let a = &self.affixes[target as usize];
                let (cap_p, cap_s) = Rarity::Rare.cap();
                let room = match a.slot {
                    Slot::Prefix => self.count(item, Slot::Prefix) < cap_p,
                    Slot::Suffix => self.count(item, Slot::Suffix) < cap_s,
                };
                let held: Vec<GroupId> = item.mods().iter().map(|m| self.affixes[m.idx as usize].group).collect();
                if !room || held.contains(&a.group) {
                    return Outcome::NotApplicable;
                }
                item.rarity = Rarity::Rare;
                item.push(Mod { idx: target, fractured: false });
            }
            Desecrate if item.rarity == Rarity::Rare && !self.has_desecrated(item) => {
                if n == 6 && !self.remove_random(item, None, false, false, rng) {
                    return Outcome::NotApplicable;
                }
                let f = DrawFilter { min_mod_level: 0, force_slot: c.add_slot, require_desecrated: true, require_tag: c.require_tag };
                match self.draw(item, &f, rng) {
                    Some(idx) => item.push(Mod { idx, fractured: false }),
                    None => return Outcome::NotApplicable,
                }
            }
            _ => return Outcome::NotApplicable,
        }
        Outcome::Applied
    }
}
