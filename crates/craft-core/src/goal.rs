use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MAX_WANTED: usize = 6;

/// « Ce groupe d'affixes, au tier `max_tier` ou mieux ».
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WantedAffix {
    pub group: GroupId,
    pub max_tier: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Affixe accepté pour le k-ième voulu.
    Wanted(usize),
    /// Même groupe que le k-ième voulu mais tier insuffisant : occupe le groupe (bloque).
    Blocked(usize),
    Other,
}

#[derive(Clone, Debug)]
pub struct Goal {
    pub wanted: Vec<WantedAffix>,
    pub slots: Vec<Slot>,
    by_group: HashMap<GroupId, usize>,
    accepted: Vec<Vec<bool>>, // accepted[k][affix_idx]
}

impl Goal {
    pub fn new(pool: &AffixPool, wanted: &[WantedAffix]) -> Result<Self, String> {
        if wanted.is_empty() || wanted.len() > MAX_WANTED {
            return Err(format!("l'objectif doit contenir 1 à {MAX_WANTED} affixes (reçu {})", wanted.len()));
        }
        let mut by_group = HashMap::new();
        let mut slots = Vec::new();
        for (k, w) in wanted.iter().enumerate() {
            if by_group.insert(w.group, k).is_some() {
                return Err("deux affixes voulus appartiennent au même groupe".into());
            }
            let slot = pool
                .affixes
                .iter()
                .find(|a| a.group == w.group)
                .map(|a| a.slot)
                .ok_or_else(|| format!("groupe {} absent du pool de cette base", w.group))?;
            slots.push(slot);
        }
        let p = slots.iter().filter(|s| **s == Slot::Prefix).count();
        let s = slots.len() - p;
        if p > 3 || s > 3 {
            return Err("plus de 3 préfixes ou 3 suffixes voulus : objectif impossible".into());
        }
        let accepted = wanted
            .iter()
            .map(|w| pool.affixes.iter().map(|a| a.group == w.group && a.tier <= w.max_tier).collect())
            .collect();
        Ok(Self { wanted: wanted.to_vec(), slots, by_group, accepted })
    }

    pub fn len(&self) -> usize {
        self.wanted.len()
    }
    pub fn is_empty(&self) -> bool {
        self.wanted.is_empty()
    }

    #[inline]
    pub fn classify(&self, pool: &AffixPool, idx: AffixIdx) -> Class {
        let g = pool.affixes[idx as usize].group;
        match self.by_group.get(&g) {
            None => Class::Other,
            Some(&k) if self.accepted[k][idx as usize] => Class::Wanted(k),
            Some(&k) => Class::Blocked(k),
        }
    }

    pub fn group_index(&self, g: GroupId) -> Option<usize> {
        self.by_group.get(&g).copied()
    }

    #[inline]
    pub fn is_met(&self, item: &ItemState) -> bool {
        self.accepted.iter().all(|s| item.mods().iter().any(|m| s[m.idx as usize]))
    }
}
