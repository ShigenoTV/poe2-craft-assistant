//! Comparateur de chemins : le plan optimal face aux meilleurs plans qui se passent d'une famille de
//! monnaies qu'il utilise (sans Chaos, sans Essences, sans Omens…). Chaque chemin est une vraie politique
//! optimale du même MDP, privée d'une famille, puis vérifiée sur le moteur exact (moyenne, médiane,
//! pire cas à 99 %, écart-type). Le moteur exact n'est pas modifié.

use crate::PlanContext;
use craft_core::*;
use craft_solver::*;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparedPath {
    /// « Plan optimal », « Sans Chaos »…
    pub label: String,
    /// famille écartée (`None` pour le plan optimal)
    pub excluded: Option<String>,
    /// identifiants des actions écartées : les retirer des monnaies autorisées redonne ce chemin
    pub excluded_actions: Vec<String>,
    /// coût espéré du solveur (hors première base), comme `CraftPlan::expected_cost`
    pub expected_cost: f64,
    /// monnaies les plus coûteuses du chemin (hors bases), par coût espéré décroissant
    pub main_currencies: Vec<ShoppingLine>,
    pub converged: bool,
    /// vérification sur le moteur exact (`None` si annulée ou aucun essai abouti)
    pub mc: Option<VerifyResult>,
}

/// Familles de monnaies auxquelles appartient une action (une action à Omen est aussi dans « omens »).
pub fn families(a: &Action) -> Vec<(&'static str, &'static str)> {
    let ActionKind::Currency(c) = &a.kind else { return vec![] };
    use CurrencyKind::*;
    let kind = match c.kind {
        Transmute => ("transmute", "Transmutation"),
        Augment => ("augment", "Augmentation"),
        Regal => ("regal", "Regal"),
        Alchemy => ("alchemy", "Alchimie"),
        Exalt => ("exalt", "Exaltation"),
        Chaos => ("chaos", "Chaos"),
        Annul => ("annul", "Annulation"),
        Fracture => ("fracture", "Fracture"),
        Essence => ("essence", "Essences et Alloys"),
        Desecrate => ("desecrate", "Désécration"),
    };
    let mut v = vec![kind];
    if a.id.contains('+') {
        v.push(("omens", "Omens"));
    }
    v
}

/// Familles candidates à écarter : celles que le plan optimal utilise, de la plus coûteuse à la moins chère.
const MAX_CANDIDATES: usize = 4;

struct Solved {
    label: String,
    excluded: Option<String>,
    excluded_actions: Vec<String>,
    /// modèle et solution du chemin ; `None` = ceux du plan optimal (`ctx`)
    solved: Option<(Arc<Model>, Solution)>,
    plan: CraftPlan,
}

fn summarize_plan(model: &Model, sol: &Solution, start: MacroState) -> Result<CraftPlan, String> {
    let inputs = PlanInputs { model, sol, start, base_id: "", base_cost: 0.0, salvage: 0.0, goal_items: vec![], prices_source: String::new() };
    build_plan(&inputs, &PlanConfig { node_cap: 2, ..Default::default() })
}

/// Actions (hors bases) réellement employées par un plan : sert à repérer deux chemins identiques.
fn used(plan: &CraftPlan) -> BTreeSet<String> {
    plan.shopping.iter().filter(|l| l.id != "__base" && l.expected_count > 1e-6).map(|l| l.id.clone()).collect()
}

/// Compare le plan optimal de `ctx` aux `max_alternatives` meilleurs chemins sans l'une de ses familles de
/// monnaies, chacun vérifié par `trials` essais sur le moteur exact. À appeler dans `pool.install(..)`.
/// `on_progress(fait, total)` renvoie `false` pour annuler.
pub fn compare_paths(
    ctx: &PlanContext,
    max_alternatives: usize,
    trials: u64,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u64, u64) -> bool,
) -> Result<Vec<ComparedPath>, String> {
    let model = ctx.model.clone();
    let start_item = match &ctx.req.starting_item {
        Some(view) => view.to_state(&model.pool)?,
        None => ItemState::new(Rarity::Normal, ctx.req.ilvl),
    };
    let plan0 = summarize_plan(&model, &ctx.solution, ctx.start)?;

    // poids de chaque famille dans le plan optimal
    let mut weight: Vec<(&'static str, &'static str, f64)> = Vec::new();
    for line in plan0.shopping.iter().filter(|l| l.expected_count > 1e-6) {
        let Some(a) = model.actions.iter().find(|a| a.id == line.id) else { continue };
        for (key, label) in families(a) {
            match weight.iter_mut().find(|w| w.0 == key) {
                Some(w) => w.2 += line.expected_cost,
                None => weight.push((key, label, line.expected_cost)),
            }
        }
    }
    weight.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    weight.truncate(MAX_CANDIDATES);

    let alts: Vec<Solved> = weight
        .par_iter()
        .filter_map(|&(key, label, _)| {
            let (kept, banned): (Vec<Action>, Vec<Action>) = model.actions.iter().cloned().partition(|a| !families(a).iter().any(|f| f.0 == key));
            if !kept.iter().any(|a| matches!(a.kind, ActionKind::Currency(_))) {
                return None;
            }
            let m = Arc::new(Model::new(model.pool.clone(), model.goal.clone(), model.ilvl, kept, ctx.base_cost, ctx.salvage));
            // les mods garantis suivis dépendent des actions : on reprojette l'objet de départ
            let start = m.project(&start_item)?;
            let sol = solve_cached(SolveCache::global(), &m, &[start], &SolveConfig::default(), cancel).ok()?;
            let id = sol.id(&start)?;
            if !sol.value[id].is_finite() {
                return None;
            }
            let plan = summarize_plan(&m, &sol, start).ok()?;
            Some(Solved {
                label: format!("Sans {label}"),
                excluded: Some(key.to_string()),
                excluded_actions: banned.into_iter().map(|a| a.id).collect(),
                solved: Some((m, sol)),
                plan,
            })
        })
        .collect();
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("annulé".into());
    }

    let mut alts = alts;
    alts.sort_by(|a, b| a.plan.expected_cost.partial_cmp(&b.plan.expected_cost).unwrap_or(std::cmp::Ordering::Equal));
    let mut chosen = vec![Solved {
        label: "Plan optimal".into(),
        excluded: None,
        excluded_actions: vec![],
        solved: None,
        plan: plan0,
    }];
    for alt in alts {
        if chosen.len() > max_alternatives {
            break;
        }
        // deux bannissements peuvent mener au même chemin (ex. sans Chaos et sans Omens de Chaos)
        let same = chosen.iter().any(|c| used(&c.plan) == used(&alt.plan) && (c.plan.expected_cost - alt.plan.expected_cost).abs() <= 1e-6 * c.plan.expected_cost.max(1.0));
        if !same {
            chosen.push(alt);
        }
    }

    let total = trials * chosen.len() as u64;
    let mut out = Vec::new();
    for (i, c) in chosen.into_iter().enumerate() {
        let base = trials * i as u64;
        let mut stop = false;
        let (m, sol) = match &c.solved {
            Some((m, sol)) => (m.as_ref(), sol),
            None => (model.as_ref(), &ctx.solution),
        };
        let converged = sol.converged;
        let mc = if trials > 0 {
            verify_policy(m, sol, start_item, trials, 20_000, ctx.req.seed, |done, _| {
                let go = on_progress(base + done, total);
                stop |= !go;
                go
            })
        } else {
            None
        };
        if stop {
            return Err("annulé".into());
        }
        let main_currencies = c.plan.shopping.iter().filter(|l| l.id != "__base" && l.expected_count > 1e-6).take(4).cloned().collect();
        out.push(ComparedPath {
            label: c.label,
            excluded: c.excluded,
            excluded_actions: c.excluded_actions,
            expected_cost: c.plan.expected_cost,
            main_currencies,
            converged,
            mc,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use std::collections::HashSet;

    /// Bout en bout : sur une épée, le plan optimal passe par l'Essence d'Abrasion ; le comparateur doit
    /// proposer un vrai chemin sans Essence (Transmutation/Augmentation), plus cher en moyenne, que le
    /// moteur exact confirme, avec pire cas et écart-type.
    #[test]
    fn compares_the_optimal_plan_with_a_path_without_its_essence() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let enabled: HashSet<String> = ["transmute", "augment", "essence_abrasion"].iter().map(|s| s.to_string()).collect();
        let req = PlanRequest {
            base_id: "sword_1h".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "PhysicalDamage".into(), max_tier: 5 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 7,
            prices_label: None,
            starting_item: None,
            instill: None,
        };
        let cancel = AtomicBool::new(false);
        let ctx = build_context(&ds, &req, &prices, &cancel).expect("build_context");
        let paths = compare_paths(&ctx, 2, 4_000, &cancel, |_, _| true).expect("compare_paths");
        assert_eq!(paths[0].label, "Plan optimal");
        assert!(paths[0].main_currencies.iter().any(|l| l.id == "essence_abrasion"), "le plan optimal doit utiliser l'Essence : {:?}", paths[0].main_currencies);
        let alt = paths.iter().find(|p| p.excluded.as_deref() == Some("essence")).expect("un chemin sans Essence doit être proposé");
        assert!(alt.excluded_actions.contains(&"essence_abrasion".to_string()));
        assert!(alt.main_currencies.iter().all(|l| l.id != "essence_abrasion"), "le chemin sans Essence ne doit pas en acheter");
        assert!(alt.main_currencies.iter().any(|l| l.id == "transmute" || l.id == "augment"));
        assert!(alt.expected_cost >= paths[0].expected_cost * (1.0 - 1e-9), "un chemin privé d'une monnaie ne peut pas être moins cher que l'optimal");
        for p in &paths {
            let mc = p.mc.as_ref().expect("vérification sur le moteur exact");
            assert!(mc.std_dev.is_finite() && mc.std_dev > 0.0, "{} : écart-type {}", p.label, mc.std_dev);
            assert!(mc.p99_cost >= mc.median_cost);
            let gap = (mc.mean_cost / p.expected_cost - 1.0).abs();
            assert!(gap < 0.1, "{} : moyenne simulée {} contre solveur {}", p.label, mc.mean_cost, p.expected_cost);
        }
    }
}
