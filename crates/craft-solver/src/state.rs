use craft_core::*;
use serde::Serialize;

/// Abstraction de l'objet vis-à-vis de l'objectif (au plus 6 affixes voulus).
/// Les affixes « inutiles » ne sont comptés que par slot : leur identité n'influe pas sur la décision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroState {
    pub rarity: Rarity,
    /// bit k : le k-ième affixe voulu est présent (tier accepté)
    pub held: u8,
    /// bit k : le groupe du k-ième voulu est occupé par un tier trop bas
    pub blocked: u8,
    /// 0 = rien de fracturé ; k+1 = le k-ième voulu est fracturé
    pub frac: u8,
    pub bad_p: u8,
    pub bad_s: u8,
}

impl MacroState {
    pub fn empty(rarity: Rarity) -> Self {
        Self { rarity, held: 0, blocked: 0, frac: 0, bad_p: 0, bad_s: 0 }
    }
    pub fn key(&self) -> String {
        let r = match self.rarity {
            Rarity::Normal => "n",
            Rarity::Magic => "m",
            Rarity::Rare => "r",
        };
        format!("{r}:h{:06b}:b{:06b}:f{}:p{}:s{}", self.held, self.blocked, self.frac, self.bad_p, self.bad_s)
    }
    pub fn total(&self) -> u32 {
        self.held.count_ones() + self.blocked.count_ones() + self.bad_p as u32 + self.bad_s as u32
    }
}

/// Projette un objet réel vers son état abstrait.
/// `None` = objet « mort » : un affixe fracturé qui n'est pas un affixe voulu accepté.
pub fn project(goal: &Goal, pool: &AffixPool, item: &ItemState) -> Option<MacroState> {
    let mut s = MacroState::empty(item.rarity);
    for m in item.mods() {
        match goal.classify(pool, m.idx) {
            Class::Wanted(k) => {
                s.held |= 1 << k;
                if m.fractured {
                    s.frac = k as u8 + 1;
                }
            }
            Class::Blocked(k) => {
                if m.fractured {
                    return None;
                }
                s.blocked |= 1 << k;
            }
            Class::Other => {
                if m.fractured {
                    return None;
                }
                match pool.affixes[m.idx as usize].slot {
                    Slot::Prefix => s.bad_p += 1,
                    Slot::Suffix => s.bad_s += 1,
                }
            }
        }
    }
    Some(s)
}
