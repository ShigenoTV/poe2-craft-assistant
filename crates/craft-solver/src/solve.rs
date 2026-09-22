use crate::model::*;
use crate::state::MacroState;
use std::collections::{HashMap, VecDeque};
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
}
impl Default for SolveConfig {
    fn default() -> Self {
        Self { eps: 1e-10, max_sweeps: 200_000, max_millis: 30_000, max_states: 600_000 }
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
    act_off: Vec<u32>,
    edges: Vec<Edge>,
    pub sweeps: u32,
    pub converged: bool,
    pub millis: u128,
}

impl Solution {
    pub fn edges_of(&self, s: usize, a: usize) -> &[Edge] {
        let i = s * self.n_actions + a;
        &self.edges[self.act_off[i] as usize..self.act_off[i + 1] as usize]
    }
    pub fn id(&self, s: &MacroState) -> Option<usize> {
        self.index.get(s).map(|&i| i as usize)
    }
    /// (p_self, coût attendu d'une tentative « jusqu'à changement d'état »)
    pub fn self_loop(&self, s: usize, a: usize) -> f64 {
        self.edges_of(s, a).iter().filter(|e| e.to as usize == s).map(|e| e.p).sum()
    }
}

/// Énumère les états atteignables depuis `starts` (+ l'état de redémarrage) puis résout le SSP.
pub fn solve(model: &Model, starts: &[MacroState], cfg: &SolveConfig, cancel: &AtomicBool) -> Result<Solution, String> {
    let t0 = Instant::now();
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
    Ok(Solution { states, index, value: v, policy: pi, n_actions: na, act_off, edges, sweeps, converged, millis: t0.elapsed().as_millis() })
}
