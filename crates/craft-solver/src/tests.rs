use crate::*;
use craft_core::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn aff(id: String, group: u16, slot: Slot, tier: u8, w: u32) -> Affix {
    Affix { id: id.clone(), name: id.clone(), family: id, text: String::new(), group, slot, tier, req_ilvl: 1, weight: w, tags: 0, desecrated: false }
}

/// Pool synthétique : 2 groupes voulus (1 préfixe, 1 suffixe) avec T1/T2, et beaucoup de groupes « inutiles »
/// (l'approximation « exclusion de groupe des mauvais affixes ignorée » devient négligeable).
fn pool(n_bad: u16) -> AffixPool {
    let mut v = vec![
        aff("P_T1".into(), 1, Slot::Prefix, 1, 60),
        aff("P_T2".into(), 1, Slot::Prefix, 2, 140),
        aff("S_T1".into(), 2, Slot::Suffix, 1, 60),
        aff("S_T2".into(), 2, Slot::Suffix, 2, 140),
    ];
    for i in 0..n_bad {
        v.push(aff(format!("bp{i}"), 100 + i, Slot::Prefix, 1, 100));
        v.push(aff(format!("bs{i}"), 500 + i, Slot::Suffix, 1, 100));
    }
    AffixPool { affixes: v }
}

fn cur(id: &str, kind: CurrencyKind, cost: f64) -> Action {
    Action {
        id: id.into(),
        label: id.into(),
        cost,
        kind: ActionKind::Currency(Currency { id: id.into(), label: id.into(), kind, min_mod_level: 0, add_slot: None, remove_slot: None, target: None, require_tag: None, remove_desecrated_only: false, remove_lowest_level: false, unit_cost: cost }),
    }
}

fn build(n_bad: u16, max_tier: u8, extra: Vec<Action>) -> (Model, Solution) {
    let pool = Arc::new(pool(n_bad));
    let goal = Arc::new(Goal::new(&pool, &[WantedAffix { group: 1, max_tier }, WantedAffix { group: 2, max_tier }]).unwrap());
    let mut actions = vec![
        cur("transmute", CurrencyKind::Transmute, 0.1),
        cur("augment", CurrencyKind::Augment, 0.1),
        cur("regal", CurrencyKind::Regal, 0.3),
        cur("alchemy", CurrencyKind::Alchemy, 0.4),
        cur("exalt", CurrencyKind::Exalt, 1.0),
        cur("chaos", CurrencyKind::Chaos, 0.8),
        cur("annul", CurrencyKind::Annul, 2.0),
    ];
    actions.extend(extra);
    actions.push(Action { id: "abandon".into(), label: "Abandon".into(), cost: 0.0, kind: ActionKind::Abandon });
    let model = Model::new(pool, goal, 80, actions, 0.5, 0.0);
    let sol = solve(&model, &[MacroState::empty(Rarity::Normal)], &SolveConfig::default(), &AtomicBool::new(false)).unwrap();
    (model, sol)
}

fn start_id(model: &Model, sol: &Solution) -> usize {
    { let _ = model; sol.id(&MacroState::empty(Rarity::Normal)).unwrap() }
}

#[test]
fn solver_converges_and_costs_are_finite() {
    let (m, s) = build(20, 1, vec![]);
    assert!(s.converged, "sweeps={}", s.sweeps);
    let v0 = s.value[start_id(&m, &s)];
    assert!(v0.is_finite() && v0 > 0.0);
}

#[test]
fn visits_cost_equals_value_at_root() {
    // contrôle d'intégrité : Σ visites × coût par visite == V(s0)
    let (m, s) = build(20, 1, vec![]);
    let root = start_id(&m, &s);
    let inputs = PlanInputs {
        model: &m,
        sol: &s,
        start: MacroState::empty(Rarity::Normal),
        base_id: "t",
        base_cost: 0.5,
        salvage: 0.0,
        goal_items: vec![
            GoalItem { label: "P".into(), slot: Slot::Prefix, group: 1, max_tier: 1 },
            GoalItem { label: "S".into(), slot: Slot::Suffix, group: 2, max_tier: 1 },
        ],
        prices_source: "test".into(),
    };
    let plan = build_plan(&inputs, &PlanConfig::default()).unwrap();
    let rel = (plan.solver.cost_from_visits - s.value[root]).abs() / s.value[root];
    assert!(rel < 1e-6, "V={} visites={}", s.value[root], plan.solver.cost_from_visits);
    // les probabilités de branches somment à 1 (avec les branches mineures)
    for n in plan.nodes.values() {
        if let CraftNode::Action(a) = n {
            let sum: f64 = a.branches.iter().map(|b| b.probability).sum::<f64>() + a.merged_minor_probability;
            assert!((sum - 1.0).abs() < 1e-9, "{} somme={}", a.id, sum);
        }
    }
    assert!(plan.nodes.contains_key("goal"));
}

#[test]
fn analytic_cost_matches_exact_engine_monte_carlo() {
    // Le coût espéré du solveur (abstraction) doit coller au coût mesuré sur le moteur exact.
    let (m, s) = build(40, 2, vec![]);
    let root = start_id(&m, &s);
    let mc = verify_policy(&m, &s, ItemState::new(Rarity::Normal, 80), 60_000, 5_000, 1234, |_, _| true).unwrap();
    let v = s.value[root];
    let se = (mc.ci95_mean.1 - mc.ci95_mean.0) / 3.92;
    let rel = (mc.mean_cost - v).abs() / v;
    assert!(mc.censored == 0);
    assert!(rel < 0.06, "V analytique={v:.3} MC={:.3} ± {:.3} (écart {:.1} %)", mc.mean_cost, se, rel * 100.0);
}

#[test]
fn fracture_and_omens_are_handled() {
    let mut extra = vec![cur("fracture", CurrencyKind::Fracture, 3.0)];
    let mut dext = cur("exalt+dextral", CurrencyKind::Exalt, 1.6);
    if let ActionKind::Currency(c) = &mut dext.kind {
        c.add_slot = Some(Slot::Suffix);
    }
    let mut sinis = cur("annul+sinistral", CurrencyKind::Annul, 3.0);
    if let ActionKind::Currency(c) = &mut sinis.kind {
        c.remove_slot = Some(Slot::Prefix);
    }
    extra.push(dext);
    extra.push(sinis);
    let (m, s) = build(40, 2, extra);
    let root = start_id(&m, &s);
    let (v_plain, _) = {
        let (m0, s0) = build(40, 2, vec![]);
        (s0.value[start_id(&m0, &s0)], 0)
    };
    // plus d'options ne peut que réduire le coût optimal
    assert!(s.value[root] <= v_plain + 1e-9, "{} > {}", s.value[root], v_plain);
    let mc = verify_policy(&m, &s, ItemState::new(Rarity::Normal, 80), 40_000, 5_000, 99, |_, _| true).unwrap();
    let rel = (mc.mean_cost - s.value[root]).abs() / s.value[root];
    assert!(rel < 0.07, "V={:.3} MC={:.3} (écart {:.1} %)", s.value[root], mc.mean_cost, rel * 100.0);
}

#[test]
fn unreachable_goal_is_reported() {
    // ilvl trop bas : les affixes T1 exigent un niveau supérieur
    let mut p = pool(5);
    for a in p.affixes.iter_mut().filter(|a| a.id.ends_with("T1")) {
        a.req_ilvl = 90;
    }
    let pool = Arc::new(p);
    let goal = Arc::new(Goal::new(&pool, &[WantedAffix { group: 1, max_tier: 1 }]).unwrap());
    let actions = vec![cur("transmute", CurrencyKind::Transmute, 0.1), cur("exalt", CurrencyKind::Exalt, 1.0)];
    let m = Model::new(pool, goal, 80, actions, 0.5, 0.0);
    let r = solve(&m, &[MacroState::empty(Rarity::Normal)], &SolveConfig::default(), &AtomicBool::new(false));
    assert!(r.is_err());
}

#[test]
fn advise_from_a_mid_craft_item() {
    let (m, s) = build(20, 1, vec![]);
    let mut st = MacroState::empty(Rarity::Rare);
    st.held = 0b01;
    st.bad_s = 2;
    // état possiblement non énuméré depuis s0 : on résout depuis lui
    let s2 = solve(&m, &[st], &SolveConfig::default(), &AtomicBool::new(false)).unwrap();
    let adv = advise(&m, &s2, &st, &["P".into(), "S".into()]).unwrap();
    assert!(adv.action.is_some() && adv.cost_to_go.unwrap() > 0.0);
    let _ = s;
}
