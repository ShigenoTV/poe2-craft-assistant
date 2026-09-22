use crate::model::*;
use crate::solve::*;
use crate::state::*;
use craft_core::*;
use rand::{rngs::SmallRng, SeedableRng};
use rayon::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub trials: u64,
    pub mean_cost: f64,
    pub ci95_mean: (f64, f64),
    pub median_cost: f64,
    pub p90_cost: f64,
    pub p99_cost: f64,
    pub mean_steps: f64,
    pub mean_abandons: f64,
    /// essais interrompus (max_steps atteint) : exclus des statistiques
    pub censored: u64,
}

/// Exécute la politique sur le moteur EXACT (tirage pondéré réel, exclusion de groupes complète).
/// Écart avec `V(s0)` = erreur d'abstraction.
pub fn verify_policy(
    model: &Model,
    sol: &Solution,
    start: ItemState,
    trials: u64,
    max_steps: u32,
    seed: u64,
    mut on_progress: impl FnMut(u64, u64) -> bool,
) -> Option<VerifyResult> {
    const CHUNK: u64 = 512;
    const BATCH: u64 = 64;
    let chunks = (trials + CHUNK - 1) / CHUNK;
    let mut all: Vec<(f32, u32, u32, bool)> = Vec::with_capacity(trials as usize); // coût, étapes, abandons, censuré
    let mut c0 = 0;
    while c0 < chunks {
        let c1 = (c0 + BATCH).min(chunks);
        let part: Vec<Vec<(f32, u32, u32, bool)>> = (c0..c1)
            .into_par_iter()
            .map(|c| {
                let n = CHUNK.min(trials - c * CHUNK);
                let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(c.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
                (0..n).map(|_| one_trial(model, sol, start, max_steps, &mut rng)).collect()
            })
            .collect();
        for p in part {
            all.extend(p);
        }
        c0 = c1;
        if !on_progress(all.len() as u64, trials) {
            return None;
        }
    }
    let censored = all.iter().filter(|r| r.3).count() as u64;
    let mut ok: Vec<&(f32, u32, u32, bool)> = all.iter().filter(|r| !r.3).collect();
    if ok.is_empty() {
        return None;
    }
    let n = ok.len() as f64;
    let mean = ok.iter().map(|r| r.0 as f64).sum::<f64>() / n;
    let var = ok.iter().map(|r| (r.0 as f64 - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let se = (var / n).sqrt();
    ok.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let q = |p: f64| ok[((ok.len() as f64 - 1.0) * p).round() as usize].0 as f64;
    Some(VerifyResult {
        trials: ok.len() as u64,
        mean_cost: mean,
        ci95_mean: (mean - 1.96 * se, mean + 1.96 * se),
        median_cost: q(0.5),
        p90_cost: q(0.9),
        p99_cost: q(0.99),
        mean_steps: ok.iter().map(|r| r.1 as f64).sum::<f64>() / n,
        mean_abandons: ok.iter().map(|r| r.2 as f64).sum::<f64>() / n,
        censored,
    })
}

fn one_trial(model: &Model, sol: &Solution, start: ItemState, max_steps: u32, rng: &mut SmallRng) -> (f32, u32, u32, bool) {
    let fresh = ItemState::new(Rarity::Normal, start.ilvl);
    let (mut item, mut cost, mut abandons) = (start, 0.0f64, 0u32);
    for step in 0..max_steps {
        if model.goal.is_met(&item) {
            return (cost as f32, step, abandons, false);
        }
        let act = match project(&model.goal, &model.pool, &item).and_then(|s| sol.id(&s)) {
            Some(i) if sol.policy[i] != NONE => sol.policy[i] as usize,
            _ => model.actions.iter().position(|a| matches!(a.kind, ActionKind::Abandon)).unwrap_or(0),
        };
        match &model.actions[act].kind {
            ActionKind::Abandon => {
                cost += model.abandon_extra;
                abandons += 1;
                item = fresh;
            }
            ActionKind::Currency(c) => {
                cost += model.actions[act].cost;
                model.pool.apply(&mut item, c, rng);
            }
        }
        // objet « mort » (fracturé sur un mauvais affixe) : abandon immédiat
        if project(&model.goal, &model.pool, &item).is_none() {
            cost += model.abandon_extra;
            abandons += 1;
            item = fresh;
        }
    }
    (cost as f32, max_steps, abandons, true)
}
