use crate::model::*;
use crate::state::MacroState;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub to: u32,
    pub p: f64,
    pub extra: f64,
    pub abandon: bool,
}

#[derive(Clone, Debug)]
pub struct SolveConfig {
    pub eps: f64,
    pub max_sweeps: u32,
    pub max_millis: u128,
    pub max_states: usize,
    /// Policy iteration (évaluation exacte de chaque politique par un système linéaire) avant la value
    /// iteration, qui ne fait plus que confirmer la convergence. `false` = value iteration seule
    /// (l'ancienne méthode, gardée comme référence pour les tests).
    pub policy_iteration: bool,
}
impl Default for SolveConfig {
    fn default() -> Self {
        Self { eps: 1e-10, max_sweeps: 50_000_000, max_millis: 45_000, max_states: 600_000, policy_iteration: true }
    }
}

pub struct Solution {
    pub states: Vec<MacroState>,
    pub index: HashMap<MacroState, u32>,
    /// coût espéré restant V(s) ; +∞ si le but est inatteignable
    pub value: Vec<f64>,
    /// indice d'action optimale (NONE pour les états but)
    pub policy: Vec<u32>,
    pub n_actions: usize,
    graph: Arc<Graph>,
    pub sweeps: u32,
    /// itérations de policy iteration (0 = value iteration seule)
    pub pi_iters: u32,
    pub converged: bool,
    pub millis: u128,
}

impl Solution {
    pub fn edges_of(&self, s: usize, a: usize) -> &[Edge] {
        self.graph.edges_of(s, a)
    }
    pub fn id(&self, s: &MacroState) -> Option<usize> {
        self.index.get(s).map(|&i| i as usize)
    }
    /// (p_self, coût attendu d'une tentative « jusqu'à changement d'état »)
    pub fn self_loop(&self, s: usize, a: usize) -> f64 {
        self.edges_of(s, a).iter().filter(|e| e.to as usize == s).map(|e| e.p).sum()
    }
}

/// États atteignables et transitions d'un modèle : ne dépend pas des prix, sauf `Edge::extra` (coût d'un
/// abandon), recalculé à la volée quand seul ce prix change (voir `SolveCache`).
pub struct Graph {
    states: Vec<MacroState>,
    index: HashMap<MacroState, u32>,
    n_actions: usize,
    act_off: Vec<u32>,
    edges: Vec<Edge>,
    abandon_extra: f64,
}

impl Graph {
    fn edges_of(&self, s: usize, a: usize) -> &[Edge] {
        let i = s * self.n_actions + a;
        &self.edges[self.act_off[i] as usize..self.act_off[i + 1] as usize]
    }

    /// Même graphe, coût d'abandon `extra` (seul coût porté par les transitions).
    fn with_abandon_extra(&self, extra: f64) -> Self {
        let edges = self.edges.iter().map(|e| Edge { extra: if e.abandon { extra } else { e.extra }, ..*e }).collect();
        Self { states: self.states.clone(), index: self.index.clone(), n_actions: self.n_actions, act_off: self.act_off.clone(), edges, abandon_extra: extra }
    }
}

/// Énumère les états atteignables depuis `starts` (+ l'état de redémarrage).
fn enumerate(model: &Model, starts: &[MacroState], cfg: &SolveConfig, cancel: &AtomicBool) -> Result<Graph, String> {
    let na = model.actions.len();
    let mut states: Vec<MacroState> = Vec::new();
    let mut index: HashMap<MacroState, u32> = HashMap::new();
    let mut ins = |s: MacroState, states: &mut Vec<MacroState>| -> u32 {
        *index.entry(s).or_insert_with(|| {
            states.push(s);
            (states.len() - 1) as u32
        })
    };
    for &s in starts {
        ins(s, &mut states);
    }
    ins(model.restart, &mut states);

    let mut act_off: Vec<u32> = vec![0];
    let mut edges: Vec<Edge> = Vec::new();
    let mut out = Vec::new();
    let mut i = 0;
    while i < states.len() {
        if states.len() > cfg.max_states {
            return Err(format!("trop d'états ({}) : réduire l'objectif ou les actions autorisées", states.len()));
        }
        let s = states[i];
        for a in 0..na {
            if !model.is_goal(&s) {
                out.clear();
                model.outcomes(s, a, &mut out);
                for tr in &out {
                    let to = ins(tr.to, &mut states);
                    edges.push(Edge { to, p: tr.p, extra: tr.extra, abandon: tr.abandon });
                }
            }
            act_off.push(edges.len() as u32);
        }
        i += 1;
        if i % 2048 == 0 && cancel.load(Ordering::Relaxed) {
            return Err("annulé".into());
        }
    }
    Ok(Graph { states, index, n_actions: na, act_off, edges, abandon_extra: model.abandon_extra })
}

/// Énumère les états atteignables depuis `starts` (+ l'état de redémarrage) puis résout le SSP.
pub fn solve(model: &Model, starts: &[MacroState], cfg: &SolveConfig, cancel: &AtomicBool) -> Result<Solution, String> {
    let t0 = Instant::now();
    let graph = Arc::new(enumerate(model, starts, cfg, cancel)?);
    solve_graph(model, graph, cfg, cancel, None, t0)
}

/// Graphes déjà énumérés et dernière politique optimale de chacun, par structure de modèle (objectif,
/// base, monnaies autorisées, objet de départ ; voir `Model::structure_key`). Quand seuls les prix
/// changent, le graphe est repris tel quel et la policy iteration repart de l'ancienne politique optimale
/// (toujours propre : seuls les coûts ont bougé), ce qui la fait converger en une ou deux évaluations.
pub struct SolveCache {
    cap: usize,
    entries: Mutex<Vec<(u64, Arc<Graph>, Vec<u32>)>>,
}

impl SolveCache {
    pub fn new(cap: usize) -> Self {
        Self { cap, entries: Mutex::new(Vec::new()) }
    }

    /// Cache partagé de l'application (plan, conseil overlay, comparateur).
    pub fn global() -> &'static SolveCache {
        static CACHE: OnceLock<SolveCache> = OnceLock::new();
        CACHE.get_or_init(|| SolveCache::new(8))
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

/// Comme `solve`, en reprenant du cache le graphe et la politique d'un modèle de même structure.
pub fn solve_cached(cache: &SolveCache, model: &Model, starts: &[MacroState], cfg: &SolveConfig, cancel: &AtomicBool) -> Result<Solution, String> {
    let t0 = Instant::now();
    let key = model.structure_key(starts);
    let hit = cache.entries.lock().unwrap().iter().find(|e| e.0 == key).map(|e| (e.1.clone(), e.2.clone()));
    let (graph, warm) = match hit {
        Some((g, pol)) if g.abandon_extra.to_bits() == model.abandon_extra.to_bits() => (g, Some(pol)),
        Some((g, pol)) => (Arc::new(g.with_abandon_extra(model.abandon_extra)), Some(pol)),
        None => (Arc::new(enumerate(model, starts, cfg, cancel)?), None),
    };
    let sol = solve_graph(model, graph.clone(), cfg, cancel, warm.as_deref(), t0)?;
    let mut entries = cache.entries.lock().unwrap();
    entries.retain(|e| e.0 != key);
    entries.insert(0, (key, graph, sol.policy.clone()));
    entries.truncate(cache.cap);
    Ok(sol)
}

fn solve_graph(model: &Model, graph: Arc<Graph>, cfg: &SolveConfig, cancel: &AtomicBool, warm: Option<&[u32]>, t0: Instant) -> Result<Solution, String> {
    let na = model.actions.len();
    let (states, act_off, edges) = (&graph.states, &graph.act_off, &graph.edges);
    let index = &graph.index;
    let n = states.len();
    let is_goal: Vec<bool> = states.iter().map(|s| model.is_goal(s)).collect();

    // Atteignabilité du but (graphe inverse)
    let mut rev: Vec<Vec<u32>> = vec![Vec::new(); n];
    for s in 0..n {
        for a in 0..na {
            let (lo, hi) = (act_off[s * na + a] as usize, act_off[s * na + a + 1] as usize);
            for e in &edges[lo..hi] {
                rev[e.to as usize].push(s as u32);
            }
        }
    }
    let mut good = is_goal.clone();
    let mut q: VecDeque<usize> = (0..n).filter(|&s| is_goal[s]).collect();
    while let Some(t) = q.pop_front() {
        for &f in &rev[t] {
            if !good[f as usize] {
                good[f as usize] = true;
                q.push_back(f as usize);
            }
        }
    }
    drop(rev);

    let mut v: Vec<f64> = (0..n).map(|s| if good[s] { 0.0 } else { f64::INFINITY }).collect();
    let mut pi = vec![NONE; n];
    let costs: Vec<f64> = model.actions.iter().map(|a| a.cost).collect();

    // q(s,a) pour la valeur courante `v`
    let qval = |s: usize, a: usize, v: &[f64]| -> f64 {
        let (lo, hi) = (act_off[s * na + a] as usize, act_off[s * na + a + 1] as usize);
        if lo == hi {
            return f64::INFINITY;
        }
        let (mut p_self, mut acc) = (0.0, 0.0);
        for e in &edges[lo..hi] {
            acc += e.p * e.extra;
            if e.to as usize == s {
                p_self += e.p;
            } else {
                let vt = v[e.to as usize];
                if !vt.is_finite() {
                    return f64::INFINITY;
                }
                acc += e.p * vt;
            }
        }
        if p_self > 1.0 - 1e-12 {
            return f64::INFINITY;
        }
        (costs[a] + acc) / (1.0 - p_self) // self-loop résolu analytiquement
    };

    let mut sweeps = 0u32;
    let mut converged = false;
    let order: Vec<usize> = (0..n).filter(|&s| good[s] && !is_goal[s]).collect();
    let r = index[&model.restart] as usize;
    if !good[r] && !is_goal[r] {
        return Err("objectif inatteignable avec les actions autorisées (poids nuls ou niveau d'objet insuffisant ?)".into());
    }
    let mut pi_iters = 0u32;
    if cfg.policy_iteration {
        let ctx = PiCtx { n, na, act_off, edges, good: &good, is_goal: &is_goal, order: &order, costs: &costs };
        pi_iters = policy_iteration(&ctx, &qval, &mut v, warm, || cancel.load(Ordering::Relaxed) || t0.elapsed().as_millis() > cfg.max_millis);
    }
    while sweeps < cfg.max_sweeps {
        let mut delta = 0.0f64;
        // balayage symétrique (avant/arrière) : propage l'information plus vite qu'un sens unique
        let fwd = sweeps % 2 == 0;
        let mut step = |s: usize, v: &mut Vec<f64>| {
            let mut best = f64::INFINITY;
            for a in 0..na {
                let qa = qval(s, a, v);
                if qa < best {
                    best = qa;
                }
            }
            if best.is_finite() {
                let d = (v[s] - best).abs() / best.max(1.0);
                if d > delta {
                    delta = d;
                }
                v[s] = best;
            }
        };
        if fwd {
            for &s in order.iter() {
                step(s, &mut v);
            }
        } else {
            for &s in order.iter().rev() {
                step(s, &mut v);
            }
        }
        sweeps += 1;
        if delta < cfg.eps {
            converged = true;
            break;
        }
        if sweeps % 64 == 0 && (cancel.load(Ordering::Relaxed) || t0.elapsed().as_millis() > cfg.max_millis) {
            break;
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("annulé".into());
    }
    for &s in order.iter() {
        let (mut best, mut bi) = (f64::INFINITY, NONE);
        for a in 0..na {
            let qa = qval(s, a, &v);
            if qa.is_finite() && (best.is_infinite() || qa < best - 1e-12 * best.abs().max(1.0)) {
                best = qa;
                bi = a as u32;
            }
        }
        pi[s] = bi;
    }
    Ok(Solution { states: states.clone(), index: index.clone(), value: v, policy: pi, n_actions: na, graph, sweeps, converged, millis: t0.elapsed().as_millis(), pi_iters })
}

struct PiCtx<'a> {
    n: usize,
    na: usize,
    act_off: &'a [u32],
    edges: &'a [Edge],
    good: &'a [bool],
    is_goal: &'a [bool],
    order: &'a [usize],
    costs: &'a [f64],
}

/// Policy iteration : part d'une politique propre (chaque état choisit une action qui le rapproche du
/// but), évalue chaque politique exactement (système linéaire creux) puis l'améliore, jusqu'à ce qu'elle
/// ne change plus. Laisse dans `v` la valeur de la dernière politique évaluée (majorant de la valeur
/// optimale, égal à elle à l'arrêt) ; en cas d'échec (temps, système mal conditionné), `v` est laissé tel
/// quel ou à la dernière évaluation réussie, et la value iteration qui suit fait le reste.
/// Renvoie le nombre d'évaluations réussies.
fn policy_iteration(c: &PiCtx, qval: &impl Fn(usize, usize, &[f64]) -> f64, v: &mut Vec<f64>, warm: Option<&[u32]>, mut stop: impl FnMut() -> bool) -> u32 {
    let (n, na) = (c.n, c.na);
    let range = |s: usize, a: usize| c.act_off[s * na + a] as usize..c.act_off[s * na + a + 1] as usize;
    // action utilisable : non vide, aucun successeur hors d'atteinte du but, pas une boucle pure
    let usable = |s: usize, a: usize| {
        let r = range(s, a);
        !r.is_empty() && c.edges[r.clone()].iter().all(|e| c.good[e.to as usize]) && c.edges[r].iter().any(|e| e.to as usize != s)
    };
    // politique de départ : celle d'un calcul précédent sur le même graphe (cache), si elle est complète
    if let Some(w) = warm.filter(|w| w.len() == n && c.order.iter().all(|&s| w[s] != NONE && (w[s] as usize) < na && usable(s, w[s] as usize))) {
        return improve_loop(c, qval, v, w.to_vec(), &mut stop);
    }
    // sinon une politique propre : distance (en coups) au but par relaxation, chaque état prend une action
    // qui atteint un état strictement plus proche avec une probabilité non nulle
    let mut dist = vec![u32::MAX; n];
    let mut pol = vec![NONE; n];
    for s in 0..n {
        if c.is_goal[s] {
            dist[s] = 0;
        }
    }
    loop {
        let mut changed = false;
        for &s in c.order {
            for a in 0..na {
                if !usable(s, a) {
                    continue;
                }
                let d = c.edges[range(s, a)].iter().filter(|e| e.to as usize != s).map(|e| dist[e.to as usize]).min().unwrap_or(u32::MAX);
                if d != u32::MAX && d + 1 < dist[s] {
                    dist[s] = d + 1;
                    pol[s] = a as u32;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
        if stop() {
            return 0;
        }
    }
    if c.order.iter().any(|&s| pol[s] == NONE) {
        return 0; // un état sans action propre : laissé à la value iteration
    }
    improve_loop(c, qval, v, pol, &mut stop)
}

/// Évaluation exacte puis amélioration, jusqu'à ce que la politique (propre) `pol` ne change plus.
fn improve_loop(c: &PiCtx, qval: &impl Fn(usize, usize, &[f64]) -> f64, v: &mut Vec<f64>, mut pol: Vec<u32>, stop: &mut impl FnMut() -> bool) -> u32 {
    use crate::linsolve::{gmres, Csr};
    let (n, na) = (c.n, c.na);
    let range = |s: usize, a: usize| c.act_off[s * na + a] as usize..c.act_off[s * na + a + 1] as usize;
    let mut local = vec![u32::MAX; n];
    for (i, &s) in c.order.iter().enumerate() {
        local[s] = i as u32;
    }
    let m = c.order.len();
    let mut x = vec![0.0f64; m];
    let mut evals = 0u32;
    for _ in 0..200 {
        // évaluation : v_s = c'_s + Σ p'_t v_t (boucle sur soi résolue, but = 0)
        let mut b = vec![0.0f64; m];
        let a = Csr::identity_minus(m, |i, row| {
            let s = c.order[i];
            let act = pol[s] as usize;
            let es = &c.edges[range(s, act)];
            let p_self: f64 = es.iter().filter(|e| e.to as usize == s).map(|e| e.p).sum();
            let d = 1.0 - p_self;
            let mut acc = 0.0;
            for e in es {
                acc += e.p * e.extra;
                let t = e.to as usize;
                if t != s && !c.is_goal[t] {
                    row.push((local[t], e.p / d));
                }
            }
            b[i] = (c.costs[act] + acc) / d;
        });
        let mut y = x.clone();
        let res = gmres(&a, &b, &mut y, 1e-13, 1e-10, 4000, &mut *stop);
        if res.is_none() || y.iter().any(|t| !(t.is_finite() && *t >= 0.0)) {
            break;
        }
        x = y;
        evals += 1;
        for (i, &s) in c.order.iter().enumerate() {
            v[s] = x[i];
        }
        // amélioration : on ne change d'action que pour un gain net (évite les va-et-vient d'arrondis)
        let mut changed = 0usize;
        for &s in c.order {
            let cur = v[s];
            let (mut best, mut bi) = (cur, pol[s]);
            for a in 0..na {
                if a as u32 == pol[s] {
                    continue;
                }
                let q = qval(s, a, v);
                if q < best - 1e-9 * cur.abs().max(1.0) {
                    best = q;
                    bi = a as u32;
                }
            }
            if bi != pol[s] {
                pol[s] = bi;
                changed += 1;
            }
        }
        if changed == 0 || stop() {
            break;
        }
    }
    evals
}
