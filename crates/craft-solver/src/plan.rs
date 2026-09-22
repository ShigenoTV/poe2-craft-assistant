use crate::model::*;
use crate::solve::*;
use crate::state::MacroState;
use craft_core::*;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalItem {
    pub label: String,
    pub slot: Slot,
    pub group: GroupId,
    pub max_tier: u8,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionInfo {
    pub id: String,
    pub label: String,
    pub unit_cost: f64,
    pub is_abandon: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repeat {
    pub self_loop_probability: f64,
    pub expected_attempts: f64,
    pub p90_attempts: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub id: String,
    pub kind: String, // "success" | "failure"
    pub label: String,
    pub probability: f64,
    pub to: String,
    pub loopback: bool,
    pub extra_cost: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionNode {
    pub id: String,
    pub state_key: String,
    pub state: ItemSummary,
    pub expected_visits: f64,
    pub cost_to_go: f64,
    pub action: ActionInfo,
    pub repeat: Option<Repeat>,
    pub branches: Vec<Branch>,
    pub merged_minor_probability: f64,
    pub merged_minor_count: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalNode {
    pub id: String,
    pub state_key: String,
    pub state: ItemSummary,
    pub expected_visits: f64,
    pub cost_to_go: f64,
    pub result: String, // "goal"
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CraftNode {
    Action(ActionNode),
    Terminal(TerminalNode),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShoppingLine {
    pub id: String,
    pub label: String,
    pub expected_count: f64,
    pub unit_cost: f64,
    pub expected_cost: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolverInfo {
    pub states: usize,
    pub sweeps: u32,
    pub converged: bool,
    pub millis: u128,
    /// coût recomputé à partir des visites attendues (doit égaler V(s0) : contrôle d'intégrité)
    pub cost_from_visits: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftPlan {
    pub version: u32,
    pub base_id: String,
    pub ilvl: u8,
    pub goal: Vec<GoalItem>,
    pub root_id: String,
    pub nodes: BTreeMap<String, CraftNode>,
    /// coût espéré HORS première base (= V(s0))
    pub expected_cost: f64,
    pub base_cost: f64,
    pub shopping: Vec<ShoppingLine>,
    pub solver: SolverInfo,
    pub mc: Option<crate::verify::VerifyResult>,
    pub prices_source: String,
}

#[derive(Clone, Debug)]
pub struct PlanConfig {
    pub node_cap: usize,
    pub minor_branch_threshold: f64,
}
impl Default for PlanConfig {
    fn default() -> Self {
        Self { node_cap: 220, minor_branch_threshold: 0.004 }
    }
}

/// Visites espérées de chaque état sous la politique (chaîne de Markov sans self-loops).
pub fn expected_visits(sol: &Solution, start: usize) -> Vec<f64> {
    let n = sol.states.len();
    // CSR de la chaîne conditionnée
    let mut off = vec![0usize; n + 1];
    let mut tr: Vec<(u32, f64)> = Vec::new();
    for s in 0..n {
        let a = sol.policy[s];
        if a != NONE {
            let es = sol.edges_of(s, a as usize);
            let p_self: f64 = es.iter().filter(|e| e.to as usize == s).map(|e| e.p).sum();
            let d = 1.0 - p_self;
            for e in es.iter().filter(|e| e.to as usize != s) {
                tr.push((e.to, e.p / d));
            }
        }
        off[s + 1] = tr.len();
    }
    let mut cur = vec![0.0f64; n];
    let mut nxt = vec![0.0f64; n];
    let mut vis = vec![0.0f64; n];
    cur[start] = 1.0;
    let t0 = std::time::Instant::now();
    for it in 0..2_000_000u32 {
        let mut mass = 0.0;
        for s in 0..n {
            vis[s] += cur[s];
        }
        nxt.iter_mut().for_each(|x| *x = 0.0);
        for s in 0..n {
            let c = cur[s];
            if c == 0.0 {
                continue;
            }
            for &(t, p) in &tr[off[s]..off[s + 1]] {
                nxt[t as usize] += c * p;
            }
        }
        for s in 0..n {
            mass += nxt[s];
        }
        std::mem::swap(&mut cur, &mut nxt);
        if mass < 1e-13 {
            break;
        }
        if it % 1024 == 0 && t0.elapsed().as_millis() > 8_000 {
            break;
        }
    }
    vis
}

pub struct PlanInputs<'a> {
    pub model: &'a Model,
    pub sol: &'a Solution,
    pub start: MacroState,
    pub base_id: &'a str,
    pub base_cost: f64,
    pub salvage: f64,
    pub goal_items: Vec<GoalItem>,
    pub prices_source: String,
}

fn action_info(model: &Model, a: usize) -> ActionInfo {
    let act = &model.actions[a];
    ActionInfo { id: act.id.clone(), label: act.label.clone(), unit_cost: act.cost, is_abandon: matches!(act.kind, ActionKind::Abandon) }
}

pub fn build_plan(inp: &PlanInputs, cfg: &PlanConfig) -> Result<CraftPlan, String> {
    let (model, sol) = (inp.model, inp.sol);
    let start = sol.id(&inp.start).ok_or("état de départ absent de la solution")?;
    if !sol.value[start].is_finite() {
        return Err("objectif inatteignable depuis cet état".into());
    }
    let labels: Vec<String> = inp.goal_items.iter().map(|g| g.label.clone()).collect();
    let vis = expected_visits(sol, start);
    let n = sol.states.len();

    // ── sélection des nœuds : départ, redémarrage, puis les états les plus visités
    let mut cand: Vec<usize> = (0..n).filter(|&s| sol.policy[s] != NONE && vis[s] > 1e-9).collect();
    cand.sort_by(|&a, &b| vis[b].partial_cmp(&vis[a]).unwrap());
    let mut selected: HashSet<usize> = HashSet::new();
    selected.insert(start);
    selected.insert(sol.id(&model.restart).unwrap());
    for s in cand {
        if selected.len() >= cfg.node_cap {
            break;
        }
        selected.insert(s);
    }
    let nid = |s: usize| format!("n{s}");
    let goal_id = "goal".to_string();

    let mut nodes: BTreeMap<String, CraftNode> = BTreeMap::new();
    let mut goal_visits = 0.0;
    let mut shop: HashMap<usize, f64> = HashMap::new(); // action -> nb d'utilisations attendu
    let mut abandons = 0.0;
    let mut cost_from_visits = 0.0;

    for s in 0..n {
        if sol.policy[s] == NONE || vis[s] <= 0.0 {
            continue;
        }
        let a = sol.policy[s] as usize;
        let es = sol.edges_of(s, a);
        let p_self: f64 = es.iter().filter(|e| e.to as usize == s).map(|e| e.p).sum();
        let d = 1.0 - p_self;
        *shop.entry(a).or_default() += vis[s] / d;
        cost_from_visits += vis[s] * (model.actions[a].cost / d);
        for e in es.iter().filter(|e| e.to as usize != s) {
            cost_from_visits += vis[s] * e.p / d * e.extra;
            if e.abandon {
                abandons += vis[s] * e.p / d;
            }
            if model.is_goal(&sol.states[e.to as usize]) {
                goal_visits += vis[s] * e.p / d;
            }
        }
    }
    if model.is_goal(&inp.start) {
        goal_visits = 1.0;
    }

    for &s in &selected {
        let st = sol.states[s];
        let a = sol.policy[s];
        if a == NONE {
            continue;
        }
        let a = a as usize;
        let es = sol.edges_of(s, a);
        let p_self: f64 = es.iter().filter(|e| e.to as usize == s).map(|e| e.p).sum();
        let d = 1.0 - p_self;
        // regroupe par (destination, abandon)
        let mut groups: Vec<(usize, bool, f64, f64)> = Vec::new(); // to, abandon, p, extra
        for e in es.iter().filter(|e| e.to as usize != s) {
            match groups.iter_mut().find(|g| g.0 == e.to as usize && g.1 == e.abandon) {
                Some(g) => g.2 += e.p / d,
                None => groups.push((e.to as usize, e.abandon, e.p / d, e.extra)),
            }
        }
        groups.sort_by(|x, y| y.2.partial_cmp(&x.2).unwrap());
        let mut branches = Vec::new();
        let (mut minor_p, mut minor_n) = (0.0, 0u32);
        for (to, ab, p, extra) in groups {
            let ts = sol.states[to];
            let is_goal = model.is_goal(&ts);
            if !is_goal && !selected.contains(&to) || (p < cfg.minor_branch_threshold && !is_goal && !ab) {
                minor_p += p;
                minor_n += 1;
                continue;
            }
            let succ = is_goal || (!ab && sol.value[to] + extra < sol.value[s]);
            branches.push(Branch {
                id: format!("{}-{}{}", nid(s), if is_goal { "goal".to_string() } else { format!("{to}") }, if ab { "x" } else { "" }),
                kind: if succ { "success" } else { "failure" }.into(),
                label: describe_transition(&st, &ts, &labels, ab),
                probability: p,
                to: if is_goal { goal_id.clone() } else { nid(to) },
                loopback: false,
                extra_cost: extra,
            });
        }
        // fusionne d'éventuelles arêtes vers l'unique nœud « goal »
        let mut merged: Vec<Branch> = Vec::new();
        for b in branches {
            match merged.iter_mut().find(|m| m.to == b.to && m.extra_cost == b.extra_cost) {
                Some(m) => {
                    m.probability += b.probability;
                    if b.kind == "success" {
                        m.kind = "success".into();
                    }
                }
                None => merged.push(b),
            }
        }
        let repeat = (p_self > 1e-9 && p_self < 1.0).then(|| Repeat {
            self_loop_probability: p_self,
            expected_attempts: 1.0 / d,
            p90_attempts: (0.1f64).ln() / p_self.ln(),
        });
        nodes.insert(
            nid(s),
            CraftNode::Action(ActionNode {
                id: nid(s),
                state_key: st.key(),
                state: summarize(&st),
                expected_visits: vis[s],
                cost_to_go: sol.value[s],
                action: action_info(model, a),
                repeat,
                branches: merged,
                merged_minor_probability: minor_p,
                merged_minor_count: minor_n,
            }),
        );
    }

    // nœud terminal « objectif atteint »
    let full = MacroState { held: model.goal_mask, ..MacroState::empty(Rarity::Rare) };
    nodes.insert(
        goal_id.clone(),
        CraftNode::Terminal(TerminalNode {
            id: goal_id.clone(),
            state_key: "goal".into(),
            state: summarize(&full),
            expected_visits: goal_visits,
            cost_to_go: 0.0,
            result: "goal".into(),
            note: Some("Tous les affixes voulus sont présents.".into()),
        }),
    );

    // marque les arêtes de retour (DFS depuis la racine)
    let root = nid(start);
    if !nodes.contains_key(&root) {
        // départ déjà au but
        return Ok(finish(inp, cfg, nodes, goal_id.clone(), cost_from_visits, vec![], sol));
    }
    let mut color: HashMap<String, u8> = HashMap::new();
    let mut back: HashSet<String> = HashSet::new();
    fn dfs(id: &str, nodes: &BTreeMap<String, CraftNode>, color: &mut HashMap<String, u8>, back: &mut HashSet<String>) {
        color.insert(id.to_string(), 1);
        if let Some(CraftNode::Action(n)) = nodes.get(id) {
            for b in &n.branches {
                match color.get(&b.to) {
                    Some(1) => {
                        back.insert(b.id.clone());
                    }
                    Some(_) => {}
                    None => dfs(&b.to, nodes, color, back),
                }
            }
        }
        color.insert(id.to_string(), 2);
    }
    dfs(&root, &nodes, &mut color, &mut back);
    for node in nodes.values_mut() {
        if let CraftNode::Action(n) = node {
            for b in &mut n.branches {
                b.loopback = back.contains(&b.id);
            }
        }
    }
    // nœuds non atteignables depuis la racine (élagués par le seuil) : on les retire
    nodes.retain(|k, _| color.contains_key(k));
    if !nodes.contains_key(&goal_id) {
        nodes.insert(
            goal_id.clone(),
            CraftNode::Terminal(TerminalNode {
                id: goal_id.clone(),
                state_key: "goal".into(),
                state: summarize(&full),
                expected_visits: goal_visits,
                cost_to_go: 0.0,
                result: "goal".into(),
                note: None,
            }),
        );
    }

    // liste de courses
    let mut lines: Vec<ShoppingLine> = shop
        .into_iter()
        .filter(|(a, _)| !matches!(model.actions[*a].kind, ActionKind::Abandon))
        .map(|(a, c)| ShoppingLine {
            id: model.actions[a].id.clone(),
            label: model.actions[a].label.clone(),
            expected_count: c,
            unit_cost: model.actions[a].cost,
            expected_cost: c * model.actions[a].cost,
        })
        .collect();
    lines.sort_by(|a, b| b.expected_cost.partial_cmp(&a.expected_cost).unwrap());
    let bases = 1.0 + abandons;
    lines.push(ShoppingLine {
        id: "__base".into(),
        label: "Bases neuves (départ + abandons)".into(),
        expected_count: bases,
        unit_cost: inp.base_cost,
        expected_cost: bases * inp.base_cost - abandons * inp.salvage,
    });
    Ok(finish(inp, cfg, nodes, goal_id, cost_from_visits, lines, sol))
}

fn finish(
    inp: &PlanInputs,
    _cfg: &PlanConfig,
    nodes: BTreeMap<String, CraftNode>,
    _goal_id: String,
    cost_from_visits: f64,
    shopping: Vec<ShoppingLine>,
    sol: &Solution,
) -> CraftPlan {
    let start = sol.id(&inp.start).unwrap();
    CraftPlan {
        version: 1,
        base_id: inp.base_id.to_string(),
        ilvl: inp.model.ilvl,
        goal: inp.goal_items.clone(),
        root_id: format!("n{start}"),
        nodes,
        expected_cost: sol.value[start],
        base_cost: inp.base_cost,
        shopping,
        solver: SolverInfo { states: sol.states.len(), sweeps: sol.sweeps, converged: sol.converged, millis: sol.millis, cost_from_visits },
        mc: None,
        prices_source: inp.prices_source.clone(),
    }
}

// ───────────────────────── Conseil en temps réel (overlay) ─────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdviceOutcome {
    pub label: String,
    pub probability: f64,
    pub kind: String,
    pub cost_to_go: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Advice {
    pub state_key: String,
    pub state: ItemSummary,
    pub goal_reached: bool,
    pub cost_to_go: Option<f64>,
    pub action: Option<ActionInfo>,
    pub repeat: Option<Repeat>,
    pub outcomes: Vec<AdviceOutcome>,
}

pub fn advise(model: &Model, sol: &Solution, state: &MacroState, labels: &[String]) -> Option<Advice> {
    let s = sol.id(state)?;
    let goal_reached = model.is_goal(state);
    let a = sol.policy[s];
    let mut adv = Advice {
        state_key: state.key(),
        state: summarize(state),
        goal_reached,
        cost_to_go: sol.value[s].is_finite().then(|| sol.value[s]),
        action: None,
        repeat: None,
        outcomes: vec![],
    };
    if a == NONE {
        return Some(adv);
    }
    let a = a as usize;
    adv.action = Some(action_info(model, a));
    let es = sol.edges_of(s, a);
    let p_self: f64 = es.iter().filter(|e| e.to as usize == s).map(|e| e.p).sum();
    let d = 1.0 - p_self;
    if p_self > 1e-9 {
        adv.repeat = Some(Repeat { self_loop_probability: p_self, expected_attempts: 1.0 / d, p90_attempts: 0.1f64.ln() / p_self.ln() });
    }
    let mut groups: Vec<(usize, bool, f64)> = Vec::new();
    for e in es.iter().filter(|e| e.to as usize != s) {
        match groups.iter_mut().find(|g| g.0 == e.to as usize && g.1 == e.abandon) {
            Some(g) => g.2 += e.p / d,
            None => groups.push((e.to as usize, e.abandon, e.p / d)),
        }
    }
    groups.sort_by(|x, y| y.2.partial_cmp(&x.2).unwrap());
    for (to, ab, p) in groups.into_iter().take(6) {
        let ts = sol.states[to];
        let succ = model.is_goal(&ts) || (!ab && sol.value[to] < sol.value[s]);
        adv.outcomes.push(AdviceOutcome {
            label: describe_transition(state, &ts, labels, ab),
            probability: p,
            kind: if succ { "success" } else { "failure" }.into(),
            cost_to_go: sol.value[to].is_finite().then(|| sol.value[to]),
        });
    }
    Some(adv)
}
