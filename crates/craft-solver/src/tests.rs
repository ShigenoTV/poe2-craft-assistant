use crate::*;
use craft_core::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn aff(id: String, group: u16, slot: Slot, tier: u8, w: u32) -> Affix {
    Affix { id: id.clone(), name: id.clone(), family: id, text: String::new(), group, family_id: 0, slot, tier, req_ilvl: 1, weight: w, tags: 0, desecrated: false, cap_shift: (0, 0) }
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
    AffixPool::new(v)
}

fn cur(id: &str, kind: CurrencyKind, cost: f64) -> Action {
    Action {
        id: id.into(),
        label: id.into(),
        cost,
        kind: ActionKind::Currency(Currency { id: id.into(), label: id.into(), kind, min_mod_level: 0, add_slot: None, remove_slot: None, target: None, alt_target: None, require_tag: None, remove_desecrated_only: false, remove_lowest_level: false, requires_rare: false, unit_cost: cost }),
    }
}

fn build(n_bad: u16, max_tier: u8, extra: Vec<Action>) -> (Model, Solution) {
    let pool = Arc::new(pool(n_bad));
    let goal = Arc::new(Goal::new(&pool, &[WantedAffix { group: 1, family: 0, max_tier }, WantedAffix { group: 2, family: 0, max_tier }]).unwrap());
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
            GoalItem { label: "P".into(), slot: Slot::Prefix, group: 1, family_id: 0, max_tier: 1 },
            GoalItem { label: "S".into(), slot: Slot::Suffix, group: 2, family_id: 0, max_tier: 1 },
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
    let goal = Arc::new(Goal::new(&pool, &[WantedAffix { group: 1, family: 0, max_tier: 1 }]).unwrap());
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

/// Bout en bout, base à plafond décalé (Penumbra : 5 préfixes / 1 suffixe en Rare) : un objectif à 4
/// préfixes, impossible sur une base ordinaire, devient atteignable ; le coût du solveur colle à celui
/// mesuré sur le moteur exact, qui applique le même plafond.
#[test]
fn shifted_base_cap_allows_four_prefixes_and_matches_monte_carlo() {
    let mut v: Vec<Affix> = (1..=4).map(|g| aff(format!("P{g}"), g, Slot::Prefix, 1, 600)).collect();
    for i in 0..12 {
        v.push(aff(format!("bp{i}"), 100 + i, Slot::Prefix, 1, 100));
        v.push(aff(format!("bs{i}"), 500 + i, Slot::Suffix, 1, 100));
    }
    let wanted: Vec<WantedAffix> = (1..=4).map(|g| WantedAffix { group: g, family: 0, max_tier: 1 }).collect();
    assert!(Goal::new(&AffixPool::new(v.clone()), &wanted).is_err(), "4 préfixes impossibles sur une base 3/3");

    let pool = Arc::new(AffixPool { affixes: v, cap_delta: (2, -2), ..AffixPool::default() });
    let goal = Arc::new(Goal::new(&pool, &wanted).unwrap());
    let mut actions = vec![
        cur("transmute", CurrencyKind::Transmute, 0.1),
        cur("augment", CurrencyKind::Augment, 0.1),
        cur("regal", CurrencyKind::Regal, 0.3),
        cur("exalt", CurrencyKind::Exalt, 1.0),
        cur("chaos", CurrencyKind::Chaos, 0.8),
        cur("annul", CurrencyKind::Annul, 2.0),
    ];
    actions.push(Action { id: "abandon".into(), label: "Abandon".into(), cost: 0.0, kind: ActionKind::Abandon });
    let m = Model::new(pool, goal, 80, actions, 0.5, 0.0);
    let s = solve(&m, &[MacroState::empty(Rarity::Normal)], &SolveConfig::default(), &AtomicBool::new(false)).unwrap();
    assert!(s.converged);
    let v0 = s.value[start_id(&m, &s)];
    assert!(v0.is_finite() && v0 > 0.0);
    let mc = verify_policy(&m, &s, ItemState::new(Rarity::Normal, 80), 40_000, 5_000, 4321, |_, _| true).unwrap();
    assert!(mc.censored == 0);
    let rel = (mc.mean_cost - v0).abs() / v0;
    assert!(rel < 0.07, "V={v0:.3} MC={:.3} (écart {:.1} %)", mc.mean_cost, rel * 100.0);
}

/// Budget, bout en bout : Transmutation (0,1) puis, si raté, abandon (base 0,5) et on recommence. Une
/// Transmutation pose le préfixe voulu avec p = 200/1000 = 0,2, donc avec k tentatives réussies au plus
/// P(coût ≤ 0,6·k − 0,5) = 1 − 0,8^k. La politique du solveur, rejouée sur le moteur exact, doit donner
/// cette loi du coût (quantiles, P90, probabilité de réussir avec un budget donné).
#[test]
fn budget_success_probability_matches_closed_form() {
    let pool = Arc::new(pool(3));
    let goal = Arc::new(Goal::new(&pool, &[WantedAffix { group: 1, family: 0, max_tier: 2 }]).unwrap());
    let actions = vec![
        cur("transmute", CurrencyKind::Transmute, 0.1),
        Action { id: "abandon".into(), label: "Abandon".into(), cost: 0.0, kind: ActionKind::Abandon },
    ];
    let m = Model::new(pool, goal, 80, actions, 0.5, 0.0);
    let s = solve(&m, &[MacroState::empty(Rarity::Normal)], &SolveConfig::default(), &AtomicBool::new(false)).unwrap();
    // espérance : 5 tentatives, 4 abandons → 5 × 0,1 + 4 × 0,5 = 2,5
    assert!((s.value[start_id(&m, &s)] - 2.5).abs() < 1e-6, "V={}", s.value[start_id(&m, &s)]);
    let mc = verify_policy(&m, &s, ItemState::new(Rarity::Normal, 80), 100_000, 5_000, 777, |_, _| true).unwrap();
    assert_eq!(mc.censored, 0);
    assert_eq!(mc.cost_quantiles.len(), QUANTILE_STEPS + 1);
    assert!(mc.cost_quantiles.windows(2).all(|w| w[0] <= w[1]), "quantiles croissants");
    let exact = |budget: f64| {
        let k = ((budget + 0.5) / 0.6 + 1e-9).floor().max(0.0);
        1.0 - 0.8f64.powf(k)
    };
    for budget in [0.05, 0.1, 0.7, 1.3, 2.5, 4.3, 6.1, 10.0, 20.0] {
        let p = mc.success_probability(budget);
        assert!((p - exact(budget)).abs() < 0.012, "budget {budget} : {p:.4} contre {:.4} attendu", exact(budget));
    }
    // 1 − 0,8^k ≥ 0,9 dès k = 11 tentatives : 11 × 0,1 + 10 × 0,5 = 6,1
    assert!((mc.p90_cost - 6.1).abs() < 1e-4, "P90 = {}", mc.p90_cost);
    assert_eq!(mc.success_probability(-1.0), 0.0);
    assert!(mc.success_probability(1e9) > 0.999);
}

/// Une monnaie que le moteur exact refuse n'est pas consommée (rien à payer), et un plan qui la redemande
/// sans fin interrompt l'essai vite, au lieu de facturer `max_steps` fois la monnaie.
#[test]
fn refused_currency_is_not_charged_and_stops_the_trial() {
    use rand::SeedableRng;
    let (m, mut s) = build(20, 1, vec![]);
    let root = start_id(&m, &s);
    // plan faussé : Exalted sur la base Normale, que le moteur exact refuse toujours
    s.policy[root] = m.actions.iter().position(|a| a.id == "exalt").unwrap() as u32;
    let mut rng = rand::rngs::SmallRng::seed_from_u64(1);
    let (cost, steps, abandons, censored) = crate::verify::one_trial(&m, &s, ItemState::new(Rarity::Normal, 80), 20_000, &mut rng);
    assert!(censored);
    assert_eq!((cost, abandons), (0.0, 0));
    assert!(steps <= 64, "essai interrompu après {steps} étapes");
}
