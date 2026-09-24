use crate::{goal::*, model::*};
use rand::{rngs::SmallRng, SeedableRng};
use rayon::prelude::*;
use serde::Serialize;

/// Simulation « bac à sable » : applique `currency` en boucle depuis `start` jusqu'au succès,
/// à l'inapplicabilité ou à `max_orbs`.
pub struct SimSpec {
    pub start: ItemState,
    pub currency: Currency,
    pub goal: Goal,
    pub max_orbs: u32,
    pub base_cost: f64,
}

pub struct SimConfig {
    pub trials: u64,
    pub seed: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimResult {
    pub trials: u64,
    pub successes: u64,
    pub p_hat: f64,
    pub ci95: (f64, f64),
    pub mean_orbs_all: f64,
    pub mean_orbs_on_success: Option<f64>,
    pub cost_per_success: Option<f64>,
}

#[derive(Default, Clone, Copy)]
struct Acc {
    trials: u64,
    successes: u64,
    orbs_all: u64,
    orbs_succ: u64,
}
impl Acc {
    fn merge(self, o: Self) -> Self {
        Self {
            trials: self.trials + o.trials,
            successes: self.successes + o.successes,
            orbs_all: self.orbs_all + o.orbs_all,
            orbs_succ: self.orbs_succ + o.orbs_succ,
        }
    }
}

fn run_trial(pool: &AffixPool, spec: &SimSpec, rng: &mut SmallRng) -> (bool, u32) {
    let mut item = spec.start;
    if spec.goal.is_met(&item) {
        return (true, 0);
    }
    for n in 1..=spec.max_orbs {
        if pool.apply(&mut item, &spec.currency, rng) == Outcome::NotApplicable {
            return (false, n - 1);
        }
        if spec.goal.is_met(&item) {
            return (true, n);
        }
    }
    (false, spec.max_orbs)
}

/// Intervalle de Wilson 95 % (robuste près de 0 ou 1).
pub fn wilson95(k: f64, n: f64) -> (f64, f64) {
    let z = 1.959_964;
    let p = k / n;
    let d = 1.0 + z * z / n;
    let c = p + z * z / (2.0 * n);
    let m = z * ((p * (1.0 - p) + z * z / (4.0 * n)) / n).sqrt();
    (((c - m) / d).max(0.0), ((c + m) / d).min(1.0))
}

/// Une graine par *chunk* (pas par thread) : résultat déterministe quel que soit le nombre de threads.
/// `on_progress(done, total) -> bool` : renvoyer `false` annule le calcul (`None`).
pub fn simulate(
    pool: &AffixPool,
    spec: &SimSpec,
    cfg: &SimConfig,
    mut on_progress: impl FnMut(u64, u64) -> bool,
) -> Option<SimResult> {
    const CHUNK: u64 = 1024;
    const BATCH_CHUNKS: u64 = 64;
    let total_chunks = (cfg.trials + CHUNK - 1) / CHUNK;
    let mut acc = Acc::default();
    let mut c0 = 0u64;
    while c0 < total_chunks {
        let c1 = (c0 + BATCH_CHUNKS).min(total_chunks);
        let part = (c0..c1)
            .into_par_iter()
            .map(|c| {
                let first = c * CHUNK;
                let n = CHUNK.min(cfg.trials - first);
                let mut rng = SmallRng::seed_from_u64(cfg.seed.wrapping_add(c.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
                let mut a = Acc::default();
                for _ in 0..n {
                    let (ok, orbs) = run_trial(pool, spec, &mut rng);
                    a.trials += 1;
                    a.orbs_all += orbs as u64;
                    if ok {
                        a.successes += 1;
                        a.orbs_succ += orbs as u64;
                    }
                }
                a
            })
            .reduce(Acc::default, Acc::merge);
        acc = acc.merge(part);
        c0 = c1;
        if !on_progress(acc.trials, cfg.trials) {
            return None;
        }
    }
    let (n, k) = (acc.trials as f64, acc.successes as f64);
    let unit = spec.currency.unit_cost;
    Some(SimResult {
        trials: acc.trials,
        successes: acc.successes,
        p_hat: k / n,
        ci95: wilson95(k, n),
        mean_orbs_all: acc.orbs_all as f64 / n,
        mean_orbs_on_success: (acc.successes > 0).then(|| acc.orbs_succ as f64 / k),
        cost_per_success: (acc.successes > 0).then(|| (acc.orbs_all as f64 * unit + n * spec.base_cost) / k),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn aff(id: &str, group: GroupId, slot: Slot, w: u32) -> Affix {
        Affix {
            id: id.into(),
            name: id.into(),
            family: id.into(),
            text: id.into(),
            group,
            slot,
            tier: 1,
            req_ilvl: 1,
            weight: w,
            tags: 0,
        }
    }
    pub fn cur(kind: CurrencyKind) -> Currency {
        Currency { id: format!("{kind:?}"), label: format!("{kind:?}"), kind, min_mod_level: 0, add_slot: None, remove_slot: None, target: None, unit_cost: 1.0 }
    }

    #[test]
    fn transmute_matches_analytic_probability() {
        // A(préfixe)=100, B(préfixe)=300, C(suffixe)=600 => P(A) = 0,10
        let pool = AffixPool { affixes: vec![aff("A", 1, Slot::Prefix, 100), aff("B", 2, Slot::Prefix, 300), aff("C", 3, Slot::Suffix, 600)] };
        let spec = SimSpec {
            start: ItemState::new(Rarity::Normal, 80),
            currency: cur(CurrencyKind::Transmute),
            goal: Goal::new(&pool, &[WantedAffix { group: 1, max_tier: 1 }]).unwrap(),
            max_orbs: 1,
            base_cost: 0.0,
        };
        let n = 200_000u64;
        let r = simulate(&pool, &spec, &SimConfig { trials: n, seed: 42 }, |_, _| true).unwrap();
        let sigma = (0.1f64 * 0.9 / n as f64).sqrt();
        assert!((r.p_hat - 0.1).abs() < 4.0 * sigma, "p_hat = {}", r.p_hat);
    }

    #[test]
    fn deterministic_for_a_seed() {
        let pool = AffixPool { affixes: vec![aff("A", 1, Slot::Prefix, 100), aff("B", 2, Slot::Prefix, 300)] };
        let mk = || SimSpec {
            start: ItemState::new(Rarity::Normal, 80),
            currency: cur(CurrencyKind::Transmute),
            goal: Goal::new(&pool, &[WantedAffix { group: 1, max_tier: 1 }]).unwrap(),
            max_orbs: 1,
            base_cost: 0.0,
        };
        let a = simulate(&pool, &mk(), &SimConfig { trials: 50_000, seed: 7 }, |_, _| true).unwrap();
        let b = simulate(&pool, &mk(), &SimConfig { trials: 50_000, seed: 7 }, |_, _| true).unwrap();
        assert_eq!(a.successes, b.successes);
    }

    #[test]
    fn caps_and_groups_are_respected() {
        // 4 groupes de préfixes seulement : un Alchemy ne peut jamais dépasser 3 préfixes.
        let pool = AffixPool {
            affixes: (0..4).map(|i| aff("P", i, Slot::Prefix, 100)).chain((10..14).map(|i| aff("S", i, Slot::Suffix, 100))).collect(),
        };
        let mut rng = rand::rngs::SmallRng::seed_from_u64(1);
        for _ in 0..500 {
            let mut it = ItemState::new(Rarity::Normal, 80);
            assert_eq!(pool.apply(&mut it, &cur(CurrencyKind::Alchemy), &mut rng), Outcome::Applied);
            assert_eq!(it.len(), 4);
            assert!(pool.count(&it, Slot::Prefix) <= 3 && pool.count(&it, Slot::Suffix) <= 3);
            let mut gs: Vec<_> = it.mods().iter().map(|m| pool.affixes[m.idx as usize].group).collect();
            gs.sort();
            gs.dedup();
            assert_eq!(gs.len(), 4, "groupes dupliqués");
        }
    }

    #[test]
    fn fractured_mod_is_never_removed() {
        let pool = AffixPool { affixes: (0..3).map(|i| aff("P", i, Slot::Prefix, 100)).chain((10..13).map(|i| aff("S", i, Slot::Suffix, 100))).collect() };
        let mut rng = rand::rngs::SmallRng::seed_from_u64(3);
        for _ in 0..300 {
            let mut it = ItemState::new(Rarity::Rare, 80);
            for i in [0u16, 1, 3, 4] {
                it.push(Mod { idx: i, fractured: false });
            }
            it.set_fractured(0);
            let keep = it.mods()[0].idx;
            pool.apply(&mut it, &cur(CurrencyKind::Annul), &mut rng);
            pool.apply(&mut it, &cur(CurrencyKind::Chaos), &mut rng);
            assert!(it.mods().iter().any(|m| m.idx == keep && m.fractured));
        }
    }
}
