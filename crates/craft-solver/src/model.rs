use crate::state::*;
use craft_core::*;
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ActionKind {
    Currency(Currency),
    /// Jette l'objet et repart d'une base neuve (coût = prix de la base − revente).
    Abandon,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub kind: ActionKind,
    pub cost: f64,
}

/// Transition élémentaire : `extra` = coût additionnel (abandon), `abandon` = retour à `restart`.
#[derive(Clone, Copy, Debug)]
pub struct Tr {
    pub to: MacroState,
    pub p: f64,
    pub extra: f64,
    pub abandon: bool,
}

#[derive(Clone, Debug, Default)]
struct AddW {
    good: [f64; MAX_WANTED],
    blocked: [f64; MAX_WANTED],
    /// poids restant des groupes « inutiles » quand k mauvais affixes de ce slot sont déjà posés
    /// (espérance sous tirages pondérés sans remise : approximation de champ moyen de l'exclusion de groupe)
    /// indexé par k = 0..=plafond du slot (jusqu'à 6 sur une base qui décale le plafond)
    other_p: [f64; 7],
    other_s: [f64; 7],
}

/// P(groupe j absent des k premiers tirages pondérés sans remise), pour k = 0..=kmax (plafond du slot).
fn survival(w: &[f64], kmax: usize) -> Vec<Vec<f64>> {
    let mut inset = vec![vec![0.0f64; w.len()]; kmax + 1];
    fn dfs(w: &[f64], depth: usize, kmax: usize, used: &mut Vec<usize>, remaining: f64, prob: f64, inset: &mut Vec<Vec<f64>>) {
        for &j in used.iter() {
            inset[depth][j] += prob;
        }
        if depth == kmax || remaining <= 0.0 {
            return;
        }
        for j in 0..w.len() {
            if used.contains(&j) || w[j] <= 0.0 {
                continue;
            }
            used.push(j);
            dfs(w, depth + 1, kmax, used, remaining - w[j], prob * w[j] / remaining, inset);
            used.pop();
        }
    }
    let total: f64 = w.iter().sum();
    dfs(w, 0, kmax, &mut Vec::new(), total, 1.0, &mut inset);
    inset.into_iter().map(|row| row.into_iter().map(|p| 1.0 - p).collect()).collect()
}

#[derive(Clone)]
pub struct Model {
    pub pool: Arc<AffixPool>,
    pub goal: Arc<Goal>,
    pub ilvl: u8,
    pub actions: Vec<Action>,
    weights: Vec<Option<AddW>>,
    /// Pour les actions `Essence` : (affixe, slot, classification) de chaque affixe garanti possible (un
    /// seul, ou deux à 50/50 pour Potent Liquid Ferocity/Contempt), précalculé une fois (ne dépend pas de
    /// l'état, contrairement aux monnaies à tirage pondéré).
    essence_class: Vec<Vec<(AffixIdx, Slot, Class)>>,
    pub restart: MacroState,
    pub abandon_extra: f64,
    pub goal_mask: u8,
    pmask: u8,
    /// groupes non voulus : position dans les tables de survie, par slot (un même groupe peut avoir des
    /// tiers préfixes ET suffixes sur une base, ex. AilmentEffect Désécré : une position par table)
    other_pos: std::collections::HashMap<(GroupId, Slot), usize>,
    surv_p: Vec<Vec<f64>>,
    surv_s: Vec<Vec<f64>>,
    desec_other_pos: std::collections::HashMap<(GroupId, Slot), usize>,
    desec_surv_p: Vec<Vec<f64>>,
    desec_surv_s: Vec<Vec<f64>>,
    /// Mods garantis inutiles des remplisseurs gardés (voir `Model::new`), suivis un par un dans
    /// `MacroState::ess` (bit i = `tracked[i]`) : (groupe, slot).
    pub tracked: Vec<(GroupId, Slot)>,
    /// actions écartées d'office (remplisseurs dominés par un moins cher, voir `Model::new`)
    pruned: Vec<bool>,
    /// Loi du niveau requis d'un mod posé, par catégorie de l'état abstrait (Omen of Whittling : le Chaos
    /// retire le mod tenu du niveau le plus bas, or l'état ne garde pas les niveaux).
    levels: Levels,
}

/// Loi discrète d'un niveau requis : (niveau, probabilité), niveaux croissants.
type Dist = Vec<(u8, f64)>;

#[derive(Clone, Debug, Default)]
struct Levels {
    held: Vec<Dist>,
    blocked: Vec<Dist>,
    /// mauvais affixes du pool normal, [préfixe, suffixe]
    bad: [Dist; 2],
    /// mauvais affixes Désécrés, [préfixe, suffixe]
    desec_bad: [Dist; 2],
}

/// Loi des niveaux d'une liste (niveau, poids) ; sans poids (mods obtenus seulement par Essence), niveaux
/// équiprobables.
fn dist(items: impl Iterator<Item = (u8, u32)>) -> Dist {
    let items: Vec<(u8, u32)> = items.collect();
    let total: f64 = items.iter().map(|i| i.1 as f64).sum();
    let mut d: std::collections::BTreeMap<u8, f64> = std::collections::BTreeMap::new();
    for &(l, w) in &items {
        *d.entry(l).or_default() += if total > 0.0 { w as f64 / total } else { 1.0 / items.len() as f64 };
    }
    d.into_iter().collect()
}

/// Comment `remove_outcomes` choisit l'affixe retiré.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Removal {
    /// au hasard, uniformément parmi les affixes non fracturés
    Random,
    /// Omen of Light : seulement le mod Désécré
    DesecratedOnly,
    /// Omen of Whittling : l'affixe du niveau requis le plus bas (égalité : au hasard parmi les ex aequo)
    LowestLevel,
}

impl Removal {
    fn of(c: &Currency) -> Self {
        if c.remove_desecrated_only {
            Removal::DesecratedOnly
        } else if c.remove_lowest_level {
            Removal::LowestLevel
        } else {
            Removal::Random
        }
    }
}

/// P(chaque mod est celui du niveau le plus bas), niveaux indépendants de lois `ds`, égalités départagées
/// au hasard. Pour chaque niveau l de i : produit sur les autres de (P(> l) + P(= l)·x), le coefficient
/// de x^m comptant les cas à m ex aequo (chance 1/(m+1)).
fn lowest_probs(ds: &[&Dist]) -> Vec<f64> {
    let above = |d: &Dist, l: u8| d.iter().filter(|e| e.0 > l).map(|e| e.1).sum::<f64>();
    let at = |d: &Dist, l: u8| d.iter().filter(|e| e.0 == l).map(|e| e.1).sum::<f64>();
    (0..ds.len())
        .map(|i| {
            ds[i]
                .iter()
                .map(|&(l, p)| {
                    let mut poly = vec![1.0f64];
                    for (j, d) in ds.iter().enumerate() {
                        if j == i {
                            continue;
                        }
                        let (g, q) = (above(d, l), at(d, l));
                        let mut next = vec![0.0; poly.len() + 1];
                        for (m, c) in poly.iter().enumerate() {
                            next[m] += c * g;
                            next[m + 1] += c * q;
                        }
                        poly = next;
                    }
                    p * poly.iter().enumerate().map(|(m, c)| c / (m + 1) as f64).sum::<f64>()
                })
                .sum()
        })
        .collect()
}

/// Remplisseurs gardés par slot (voir `Model::new`).
const MAX_FILLERS: usize = 1;

fn merge(v: &mut Vec<(MacroState, f64)>) {
    let mut out: Vec<(MacroState, f64)> = Vec::with_capacity(v.len());
    for &(s, p) in v.iter() {
        match out.iter_mut().find(|(t, _)| *t == s) {
            Some(e) => e.1 += p,
            None => out.push((s, p)),
        }
    }
    *v = out;
}

impl Model {
    pub fn new(pool: Arc<AffixPool>, goal: Arc<Goal>, ilvl: u8, actions: Vec<Action>, base_cost: f64, salvage: f64) -> Self {
        let mut pmask = 0u8;
        for (k, s) in goal.slots.iter().enumerate() {
            if *s == Slot::Prefix {
                pmask |= 1 << k;
            }
        }
        let goal_mask = ((1u16 << goal.len()) - 1) as u8;

        // groupes « inutiles » et leur poids plein (tous tiers éligibles à l'ilvl) : loi des mauvais affixes déjà posés.
        // Calculé SÉPARÉMENT pour le pool normal et le pool de Désécration : ce sont deux réservoirs de
        // tirage disjoints (une monnaie normale ne peut jamais piocher un mod `desecrated`, et
        // réciproquement), donc leurs tables de poids ne doivent jamais se mélanger sous peine de fausser
        // l'approximation de champ moyen des DEUX côtés.
        fn other_tables(pool: &AffixPool, goal: &Goal, ilvl: u8, desecrated: bool, extra: (i8, i8)) -> (std::collections::HashMap<(GroupId, Slot), usize>, Vec<Vec<f64>>, Vec<Vec<f64>>) {
            let mut other_pos = std::collections::HashMap::new();
            let (mut gp, mut gs): (Vec<f64>, Vec<f64>) = (vec![], vec![]);
            for a in pool.affixes.iter().filter(|a| a.weight > 0 && a.req_ilvl <= ilvl && a.desecrated == desecrated) {
                if goal.group_index(a.group).is_some() {
                    continue;
                }
                let v = if a.slot == Slot::Prefix { &mut gp } else { &mut gs };
                let pos = *other_pos.entry((a.group, a.slot)).or_insert_with(|| {
                    v.push(0.0);
                    v.len() - 1
                });
                v[pos] += a.weight as f64;
            }
            // un mod décaleur garanti (Potent Liquid Contempt) peut ouvrir une place de plus dans un slot
            let (cap_p, cap_s) = pool.cap(Rarity::Rare);
            let (cap_p, cap_s) = ((cap_p as i16 + extra.0 as i16).clamp(0, 6) as usize, (cap_s as i16 + extra.1 as i16).clamp(0, 6) as usize);
            let (surv_p, surv_s) = (survival(&gp, cap_p), survival(&gs, cap_s));
            (other_pos, surv_p, surv_s)
        }
        let shift_targets = actions.iter().filter_map(|a| match &a.kind {
            ActionKind::Currency(c) if c.kind == CurrencyKind::Essence => Some([c.target, c.alt_target]),
            _ => None,
        });
        let extra = shift_targets.flatten().flatten().map(|t| pool.affixes[t as usize].cap_shift).fold((0i8, 0i8), |(a, b), (p, q)| (a.max(p), b.max(q)));
        let (other_pos, surv_p, surv_s) = other_tables(&pool, &goal, ilvl, false, extra);
        let (desec_other_pos, desec_surv_p, desec_surv_s) = other_tables(&pool, &goal, ilvl, true, extra);

        let mut m = Self {
            pool,
            goal,
            ilvl,
            actions,
            weights: vec![],
            essence_class: vec![],
            restart: MacroState::empty(Rarity::Normal),
            abandon_extra: (base_cost - salvage).max(0.0),
            goal_mask,
            pmask,
            other_pos,
            surv_p,
            surv_s,
            desec_other_pos,
            desec_surv_p,
            desec_surv_s,
            tracked: Vec::new(),
            pruned: Vec::new(),
            levels: Levels::default(),
        };
        {
            let (pool, goal) = (&m.pool, &m.goal);
            // Un mod tiré l'a été par une monnaie Greater/Perfect (niveau minimum) ou non : loi du niveau =
            // moyenne des lois de tirage sur les niveaux minimum des monnaies d'ajout disponibles.
            let mins = |desecrate: bool| -> Vec<u8> {
                use CurrencyKind::*;
                let mut v: Vec<u8> = m
                    .actions
                    .iter()
                    .filter_map(|a| match &a.kind {
                        ActionKind::Currency(c) if desecrate == (c.kind == Desecrate) && matches!(c.kind, Transmute | Augment | Regal | Alchemy | Exalt | Chaos | Desecrate) => Some(c.min_mod_level),
                        _ => None,
                    })
                    .collect();
                v.sort_unstable();
                v.dedup();
                if v.is_empty() {
                    v.push(0);
                }
                v
            };
            let mixed = |desecrate: bool, keep: &dyn Fn(usize, &Affix) -> bool| -> Dist {
                let parts: Vec<Dist> = mins(desecrate)
                    .into_iter()
                    .map(|min| dist(pool.affixes.iter().enumerate().filter(|(i, a)| a.req_ilvl <= ilvl && a.req_ilvl >= min && keep(*i, a)).map(|(_, a)| (a.req_ilvl, a.weight))))
                    .filter(|d| !d.is_empty())
                    .collect();
                if parts.is_empty() {
                    // aucun tier assez haut (mod obtenu seulement par Essence, ou de bas niveau) : tous les tiers
                    return dist(pool.affixes.iter().enumerate().filter(|(i, a)| a.req_ilvl <= ilvl && keep(*i, a)).map(|(_, a)| (a.req_ilvl, a.weight)));
                }
                let mut d: std::collections::BTreeMap<u8, f64> = std::collections::BTreeMap::new();
                for part in &parts {
                    for &(l, p) in part {
                        *d.entry(l).or_default() += p / parts.len() as f64;
                    }
                }
                d.into_iter().collect()
            };
            let of_class = |want: &dyn Fn(Class) -> bool| mixed(false, &|i, _| want(goal.classify(pool, i as AffixIdx)));
            let bad_of = |slot: Slot, desecrated: bool| mixed(desecrated, &|i, a| a.weight > 0 && a.slot == slot && a.desecrated == desecrated && goal.classify(pool, i as AffixIdx) == Class::Other);
            m.levels = Levels {
                held: (0..goal.len()).map(|k| of_class(&|c| c == Class::Wanted(k))).collect(),
                blocked: (0..goal.len()).map(|k| of_class(&|c| c == Class::Blocked(k))).collect(),
                bad: [bad_of(Slot::Prefix, false), bad_of(Slot::Suffix, false)],
                desec_bad: [bad_of(Slot::Prefix, true), bad_of(Slot::Suffix, true)],
            };
        }
        m.weights = m
            .actions
            .iter()
            .map(|a| match &a.kind {
                ActionKind::Currency(c) => Some(m.add_weights(c)),
                ActionKind::Abandon => None,
            })
            .collect();
        m.essence_class = m
            .actions
            .iter()
            .map(|a| match &a.kind {
                ActionKind::Currency(c) if c.kind == CurrencyKind::Essence => {
                    [c.target, c.alt_target].into_iter().flatten().map(|t| (t, m.pool.affixes[t as usize].slot, m.goal.classify(&m.pool, t))).collect()
                }
                _ => Vec::new(),
            })
            .collect();
        // « Remplisseurs » : Essence/Alloy sur objet Rare dont le mod garanti est inutile (retire un mod au
        // hasard, pose ce mod). Pour l'état abstrait, deux remplisseurs du même slot ne diffèrent que par le
        // prix et par l'identité du mod posé, qui compte seulement pour ne pas le reposer (groupe déjà pris).
        // On garde les deux moins chers de chaque slot, suivis dans `MacroState::ess`, et on écarte les
        // autres : les suivre tous multiplie les états (×20 à ×70 sur des gants) sans changer le plan.
        let mut fillers: Vec<(f64, usize, GroupId, Slot, Option<Slot>)> = Vec::new();
        for (ai, act) in m.actions.iter().enumerate() {
            let ActionKind::Currency(c) = &act.kind else { continue };
            if let [(t, slot, Class::Other)] = m.essence_class[ai].as_slice() {
                let a = &m.pool.affixes[*t as usize];
                if c.requires_rare && a.cap_shift == (0, 0) {
                    fillers.push((act.cost, ai, a.group, *slot, c.remove_slot));
                }
            }
        }
        fillers.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        m.pruned = vec![false; m.actions.len()];
        let mut kept: Vec<(GroupId, Slot, Option<Slot>)> = Vec::new();
        for &(_, ai, group, slot, rm) in &fillers {
            // même mod qu'un remplisseur moins cher, ou déjà deux remplisseurs gardés pour ce slot
            if kept.contains(&(group, slot, rm)) || kept.iter().filter(|k| k.1 == slot && k.2 == rm).count() >= MAX_FILLERS {
                m.pruned[ai] = true;
                continue;
            }
            kept.push((group, slot, rm));
            if !m.tracked.contains(&(group, slot)) {
                m.tracked.push((group, slot));
            }
        }
        m
    }

    /// Empreinte de tout ce qui détermine les états et les transitions (pool, objectif, niveau d'objet,
    /// actions et leurs effets, remplisseurs écartés, états de départ), sans les prix : deux modèles de même
    /// empreinte ont le même graphe, au coût d'abandon près (voir `SolveCache`).
    pub fn structure_key(&self, starts: &[MacroState]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for a in &self.pool.affixes {
            (a.group, a.family_id, a.slot as u8, a.tier, a.req_ilvl, a.weight, a.tags, a.desecrated, a.cap_shift).hash(&mut h);
        }
        (self.pool.cap_delta, self.pool.rare_cap, self.ilvl).hash(&mut h);
        for w in &self.goal.wanted {
            (w.group, w.family, w.max_tier).hash(&mut h);
        }
        for (k, s) in self.goal.slots.iter().enumerate() {
            (k, *s as u8).hash(&mut h);
        }
        for (a, pruned) in self.actions.iter().zip(&self.pruned) {
            a.id.hash(&mut h);
            pruned.hash(&mut h);
            match &a.kind {
                ActionKind::Abandon => 0u8.hash(&mut h),
                // tous les champs de la monnaie sauf son prix
                ActionKind::Currency(c) => format!("{:?}", Currency { unit_cost: 0.0, ..c.clone() }).hash(&mut h),
            }
        }
        self.tracked.iter().map(|&(g, s)| (g, s as u8)).collect::<Vec<_>>().hash(&mut h);
        starts.hash(&mut h);
        h.finish()
    }

    /// Bit de `MacroState::ess` du mod garanti `t`, s'il est suivi.
    fn tracked_bit(&self, t: AffixIdx) -> Option<u32> {
        let a = &self.pool.affixes[t as usize];
        self.tracked.iter().position(|&g| g == (a.group, a.slot)).map(|i| 1 << i)
    }

    /// Projette un objet réel vers son état abstrait, mods garantis d'Essence/Alloy compris (à utiliser
    /// plutôt que `project` dès qu'on cherche l'état dans une solution de ce modèle).
    pub fn project(&self, item: &ItemState) -> Option<MacroState> {
        let mut s = crate::state::project(&self.goal, &self.pool, item)?;
        for m in item.mods() {
            let a = &self.pool.affixes[m.idx as usize];
            if let Some(i) = self.tracked.iter().position(|&g| g == (a.group, a.slot)) {
                s.ess |= 1 << i;
            }
        }
        Some(s)
    }

    fn add_weights(&self, c: &Currency) -> AddW {
        let desecrate = c.kind == CurrencyKind::Desecrate;
        let (other_pos, surv_p, surv_s) = if desecrate {
            (&self.desec_other_pos, &self.desec_surv_p, &self.desec_surv_s)
        } else {
            (&self.other_pos, &self.surv_p, &self.surv_s)
        };
        let mut w = AddW::default();
        let mut gp = vec![0.0f64; surv_p.first().map_or(0, |r| r.len())];
        let mut gs = vec![0.0f64; surv_s.first().map_or(0, |r| r.len())];
        for (i, a) in self.pool.affixes.iter().enumerate() {
            if a.weight == 0
                || a.req_ilvl > self.ilvl
                || a.req_ilvl < c.min_mod_level
                || c.add_slot.map_or(false, |s| s != a.slot)
                || a.desecrated != desecrate
                || c.require_tag.is_some_and(|t| a.tags & t == 0)
            {
                continue;
            }
            let wt = a.weight as f64;
            match self.goal.classify(&self.pool, i as AffixIdx) {
                Class::Wanted(k) => w.good[k] += wt,
                Class::Blocked(k) => w.blocked[k] += wt,
                Class::Other => {
                    let pos = other_pos[&(a.group, a.slot)];
                    match a.slot {
                        Slot::Prefix => gp[pos] += wt,
                        Slot::Suffix => gs[pos] += wt,
                    }
                }
            }
        }
        for (k, row) in surv_p.iter().enumerate() {
            w.other_p[k] = gp.iter().enumerate().map(|(j, g)| g * row[j]).sum();
        }
        for (k, row) in surv_s.iter().enumerate() {
            w.other_s[k] = gs.iter().enumerate().map(|(j, g)| g * row[j]).sum();
        }
        w
    }

    pub fn is_goal(&self, s: &MacroState) -> bool {
        s.held & self.goal_mask == self.goal_mask
    }

    /// Plafond effectif dans l'état `s` : celui de la base, décalé par le mod décaleur éventuel.
    fn caps(&self, s: &MacroState, rarity: Rarity) -> (u8, u8) {
        let (p, q) = self.pool.cap(rarity);
        match s.shifter {
            Some(i) if rarity != Rarity::Normal => {
                let (dp, dq) = self.pool.affixes[i as usize].cap_shift;
                ((p as i16 + dp as i16).clamp(0, 6) as u8, (q as i16 + dq as i16).clamp(0, 6) as u8)
            }
            _ => (p, q),
        }
    }

    /// Reste-t-il une place de préfixe ou de suffixe (6 affixes au plus) ?
    fn has_room(&self, s: &MacroState) -> bool {
        let (cap_p, cap_s) = self.caps(s, s.rarity);
        let (np, ns) = self.counts(s);
        np + ns < 6 && (np < cap_p || ns < cap_s)
    }

    fn counts(&self, s: &MacroState) -> (u8, u8) {
        let occ = s.held | s.blocked;
        let np = (occ & self.pmask).count_ones() as u8 + s.bad_p;
        let ns = (occ & !self.pmask).count_ones() as u8 + s.bad_s;
        (np, ns)
    }

    /// Ajout d'un affixe. `false` si aucun affixe n'est tirable.
    fn add_outcomes(&self, s: MacroState, w: &AddW, out: &mut Vec<(MacroState, f64)>) -> bool {
        let (cap_p, cap_s) = self.caps(&s, s.rarity);
        let (np, ns) = self.counts(&s);
        let (open_p, open_s) = (np < cap_p, ns < cap_s);
        let occ = s.held | s.blocked;
        let n = self.goal.len();
        let mut total = 0.0;
        for k in 0..n {
            if occ >> k & 1 == 1 {
                continue;
            }
            let open = if self.pmask >> k & 1 == 1 { open_p } else { open_s };
            if open {
                total += w.good[k] + w.blocked[k];
            }
        }
        // le mod décaleur ne vient jamais d'un tirage : il n'entre pas dans l'exclusion des groupes tirés
        let shifted = |slot: Slot| s.shifter.is_some_and(|i| self.pool.affixes[i as usize].slot == slot) as u8;
        let (kp, ks) = (s.bad_p - shifted(Slot::Prefix), s.bad_s - shifted(Slot::Suffix));
        let (op, os) = (w.other_p[(kp as usize).min(cap_p as usize)], w.other_s[(ks as usize).min(cap_s as usize)]);
        if open_p {
            total += op;
        }
        if open_s {
            total += os;
        }
        if total <= 0.0 {
            return false;
        }
        for k in 0..n {
            if occ >> k & 1 == 1 {
                continue;
            }
            let open = if self.pmask >> k & 1 == 1 { open_p } else { open_s };
            if !open {
                continue;
            }
            if w.good[k] > 0.0 {
                out.push((MacroState { held: s.held | 1 << k, ..s }, w.good[k] / total));
            }
            if w.blocked[k] > 0.0 {
                out.push((MacroState { blocked: s.blocked | 1 << k, ..s }, w.blocked[k] / total));
            }
        }
        if open_p && op > 0.0 {
            out.push((MacroState { bad_p: s.bad_p + 1, ..s }, op / total));
        }
        if open_s && os > 0.0 {
            out.push((MacroState { bad_s: s.bad_s + 1, ..s }, os / total));
        }
        true
    }

    /// Retrait d'un affixe non fracturé (filtré par slot si Omen), choisi selon `how`. `false` si aucun
    /// candidat.
    fn remove_outcomes(&self, s: MacroState, slot: Option<Slot>, how: Removal, out: &mut Vec<(MacroState, f64)>) -> bool {
        let ok = |is_prefix: bool| slot.map_or(true, |x| (x == Slot::Prefix) == is_prefix);
        // retirer le mod Désécré libère l'objet pour une nouvelle Désécration
        let undesec = |t: MacroState, which: u8| if s.desecrated && s.desec == which { MacroState { desecrated: false, desec: DESEC_NONE, ..t } } else { t };
        // (état après retrait, nombre de mods de ce genre, loi de leur niveau, est-ce le mod Désécré)
        // lois des niveaux : utiles seulement à l'Omen of Whittling, empruntées (ou calculées) à la demande
        use std::borrow::Cow;
        let lowest = how == Removal::LowestLevel;
        let point = |i: AffixIdx| -> Cow<Dist> { Cow::Owned(if lowest { vec![(self.pool.affixes[i as usize].req_ilvl, 1.0)] } else { Vec::new() }) };
        let mut cands: Vec<(MacroState, u8, Cow<Dist>, bool)> = Vec::new();
        for k in 0..self.goal.len() {
            let is_p = self.pmask >> k & 1 == 1;
            if !ok(is_p) {
                continue;
            }
            if s.held >> k & 1 == 1 && s.frac != k as u8 + 1 {
                let which = DESEC_HELD + k as u8;
                cands.push((undesec(MacroState { held: s.held & !(1 << k), ..s }, which), 1, Cow::Borrowed(&self.levels.held[k]), s.desecrated && s.desec == which));
            }
            if s.blocked >> k & 1 == 1 {
                let which = DESEC_BLOCKED + k as u8;
                cands.push((undesec(MacroState { blocked: s.blocked & !(1 << k), ..s }, which), 1, Cow::Borrowed(&self.levels.blocked[k]), s.desecrated && s.desec == which));
            }
        }
        // le mod décaleur fait partie des mauvais affixes de son slot : le retirer referme la place qu'il ouvrait
        let shift_slot = s.shifter.map(|i| self.pool.affixes[i as usize].slot);
        for (slot, bad) in [(Slot::Prefix, s.bad_p), (Slot::Suffix, s.bad_s)] {
            if bad == 0 || !ok(slot == Slot::Prefix) {
                continue;
            }
            let si = (slot == Slot::Suffix) as usize;
            let less = match slot {
                Slot::Prefix => MacroState { bad_p: s.bad_p - 1, ..s },
                Slot::Suffix => MacroState { bad_s: s.bad_s - 1, ..s },
            };
            // mods inutiles identifiés (décaleur, mods garantis suivis, mod Désécré) : chacun retiré avec le
            // même poids qu'un autre mauvais affixe ; le reste, anonyme, se partage le poids restant
            let mut anon = bad as i32;
            if let (Some(i), true) = (s.shifter, shift_slot == Some(slot)) {
                cands.push((MacroState { shifter: None, ..less }, 1, point(i), false));
                anon -= 1;
            }
            for (i, &(g, sl)) in self.tracked.iter().enumerate() {
                if sl == slot && s.ess >> i & 1 == 1 {
                    // niveau du mod garanti suivi : celui d'un affixe de ce groupe et de ce slot
                    let lv = if lowest { dist(self.pool.affixes.iter().filter(|a| a.group == g && a.slot == sl).map(|a| (a.req_ilvl, 1))) } else { Vec::new() };
                    cands.push((MacroState { ess: s.ess & !(1 << i), ..less }, 1, Cow::Owned(lv), false));
                    anon -= 1;
                }
            }
            let which = if slot == Slot::Prefix { DESEC_BAD_P } else { DESEC_BAD_S };
            if s.desecrated && s.desec == which && anon > 0 {
                cands.push((undesec(less, which), 1, Cow::Borrowed(&self.levels.desec_bad[si]), true));
                anon -= 1;
            }
            if anon > 0 {
                cands.push((less, anon as u8, Cow::Borrowed(&self.levels.bad[si]), false));
            }
        }
        let weights: Vec<f64> = match how {
            Removal::Random => cands.iter().map(|c| c.1 as f64).collect(),
            Removal::DesecratedOnly => cands.iter().map(|c| if c.3 { 1.0 } else { 0.0 }).collect(),
            Removal::LowestLevel => {
                // un exemplaire par mod, puis on regroupe les exemplaires d'une même catégorie
                let copies: Vec<(usize, &Dist)> = cands.iter().enumerate().flat_map(|(ci, c)| std::iter::repeat_n((ci, c.2.as_ref()), c.1 as usize)).collect();
                let probs = lowest_probs(&copies.iter().map(|c| c.1).collect::<Vec<_>>());
                let mut w = vec![0.0; cands.len()];
                for ((ci, _), p) in copies.iter().zip(probs) {
                    w[*ci] += p;
                }
                w
            }
        };
        let total: f64 = weights.iter().sum();
        if total <= 0.0 {
            return false;
        }
        let mut v: Vec<(MacroState, f64)> = cands.into_iter().zip(weights).filter(|(_, w)| *w > 0.0).map(|(c, w)| (c.0, w / total)).collect();
        merge(&mut v);
        out.extend(v);
        true
    }

    /// Catégorie du mod qu'un ajout a posé entre `s` et `t` (pour savoir où est le mod Désécré).
    fn added_desec(s: &MacroState, t: &MacroState) -> u8 {
        let diff = |a: u8, b: u8| (b & !a).trailing_zeros() as u8;
        if t.held != s.held {
            DESEC_HELD + diff(s.held, t.held)
        } else if t.blocked != s.blocked {
            DESEC_BLOCKED + diff(s.blocked, t.blocked)
        } else if t.bad_p > s.bad_p {
            DESEC_BAD_P
        } else {
            DESEC_BAD_S
        }
    }

    /// Remplit `out` avec les transitions de l'action `ai` depuis `s`. Vide = inapplicable.
    pub fn outcomes(&self, s: MacroState, ai: usize, out: &mut Vec<Tr>) {
        let act = &self.actions[ai];
        let cur = match &act.kind {
            ActionKind::Abandon => {
                if s != self.restart {
                    out.push(Tr { to: self.restart, p: 1.0, extra: self.abandon_extra, abandon: true });
                }
                return;
            }
            ActionKind::Currency(_) if self.pruned[ai] => return,
            ActionKind::Currency(c) => c,
        };
        let w = self.weights[ai].as_ref().unwrap();
        let mut v: Vec<(MacroState, f64)> = Vec::new();
        use CurrencyKind::*;
        let n = s.total();
        match cur.kind {
            Transmute if s.rarity == Rarity::Normal => {
                self.add_outcomes(MacroState { rarity: Rarity::Magic, ..s }, w, &mut v);
            }
            Augment if s.rarity == Rarity::Magic && self.has_room(&s) => {
                self.add_outcomes(s, w, &mut v);
            }
            Regal if s.rarity == Rarity::Magic => {
                self.add_outcomes(MacroState { rarity: Rarity::Rare, ..s }, w, &mut v);
            }
            Alchemy if s.rarity == Rarity::Normal => {
                let mut dist = vec![(MacroState { rarity: Rarity::Rare, ..s }, 1.0)];
                for _ in 0..4 {
                    let mut next = Vec::new();
                    for (st, p) in dist {
                        let mut tmp = Vec::new();
                        if self.add_outcomes(st, w, &mut tmp) {
                            next.extend(tmp.into_iter().map(|(t, q)| (t, p * q)));
                        } else {
                            next.push((st, p));
                        }
                    }
                    merge(&mut next);
                    dist = next;
                }
                v = dist;
            }
            Exalt if s.rarity == Rarity::Rare && self.has_room(&s) => {
                self.add_outcomes(s, w, &mut v);
            }
            Chaos if s.rarity == Rarity::Rare => {
                let mut rm = Vec::new();
                if self.remove_outcomes(s, cur.remove_slot, Removal::of(cur), &mut rm) {
                    for (s1, p1) in rm {
                        let mut ad = Vec::new();
                        if self.add_outcomes(s1, w, &mut ad) {
                            v.extend(ad.into_iter().map(|(t, q)| (t, p1 * q)));
                        } else {
                            v.push((s1, p1));
                        }
                    }
                }
            }
            Annul if s.rarity != Rarity::Normal => {
                self.remove_outcomes(s, cur.remove_slot, Removal::of(cur), &mut v);
            }
            Fracture if s.rarity == Rarity::Rare && n >= 4 && s.frac == 0 => {
                let nf = n as f64;
                for k in 0..self.goal.len() {
                    if s.held >> k & 1 == 1 {
                        v.push((MacroState { frac: k as u8 + 1, ..s }, 1.0 / nf));
                    }
                }
                let dead = (s.blocked.count_ones() + s.bad_p as u32 + s.bad_s as u32) as f64 / nf;
                if dead > 0.0 {
                    out.push(Tr { to: self.restart, p: dead, extra: self.abandon_extra, abandon: true });
                }
            }
            Essence if (s.rarity == Rarity::Magic && !cur.requires_rare) || (s.rarity == Rarity::Rare && cur.requires_rare) => {
                let targets = &self.essence_class[ai];
                // état après l'ajout de l'affixe garanti `t` dans `s1`, `None` sans place ou en conflit de groupe
                let add = |s1: MacroState, (t, slot, class): (AffixIdx, Slot, Class)| -> Option<MacroState> {
                    let (cap_p, cap_s) = self.caps(&s1, Rarity::Rare);
                    let (np, ns) = self.counts(&s1);
                    let room = match slot {
                        Slot::Prefix => np < cap_p,
                        Slot::Suffix => ns < cap_s,
                    };
                    let shift = self.pool.affixes[t as usize].cap_shift != (0, 0);
                    if !room || (shift && s1.shifter.is_some()) {
                        return None;
                    }
                    match class {
                        Class::Wanted(k) if s1.held >> k & 1 == 0 && s1.blocked >> k & 1 == 0 => Some(MacroState { rarity: Rarity::Rare, held: s1.held | 1 << k, ..s1 }),
                        Class::Blocked(k) if s1.held >> k & 1 == 0 && s1.blocked >> k & 1 == 0 => Some(MacroState { rarity: Rarity::Rare, blocked: s1.blocked | 1 << k, ..s1 }),
                        Class::Other => {
                            let bit = self.tracked_bit(t).unwrap_or(0);
                            if s1.ess & bit != 0 {
                                return None; // ce mod garanti est déjà sur l'objet : groupe déjà pris
                            }
                            let st = match slot {
                                Slot::Prefix => MacroState { rarity: Rarity::Rare, bad_p: s1.bad_p + 1, ess: s1.ess | bit, ..s1 },
                                Slot::Suffix => MacroState { rarity: Rarity::Rare, bad_s: s1.bad_s + 1, ess: s1.ess | bit, ..s1 },
                            };
                            Some(if shift { MacroState { shifter: Some(t), ..st } } else { st })
                        }
                        _ => None, // groupe voulu déjà occupé (held ou blocked)
                    }
                };
                // (affixe ajouté, état après retrait, probabilité)
                let mut branches: Vec<((AffixIdx, Slot, Class), MacroState, f64)> = Vec::new();
                match targets.as_slice() {
                    [one] => {
                        if cur.requires_rare {
                            let mut rm = Vec::new();
                            if self.remove_outcomes(s, None, Removal::Random, &mut rm) {
                                branches.extend(rm.into_iter().map(|(s1, p1)| (*one, s1, p1)));
                            }
                        } else {
                            branches.push((*one, s, 1.0));
                        }
                    }
                    // préfixe OU suffixe (Potent Liquid Ferocity/Contempt) : slot tiré à 50/50, puis le mod remplace un
                    // affixe non fracturé de CE slot (aucun retrait si ce slot n'en a pas)
                    [a, b] => {
                        for t in [*a, *b] {
                            let mut rm = Vec::new();
                            if self.remove_outcomes(s, Some(t.1), Removal::Random, &mut rm) {
                                branches.extend(rm.into_iter().map(|(s1, p1)| (t, s1, 0.5 * p1)));
                            } else {
                                branches.push((t, s, 0.5));
                            }
                        }
                    }
                    _ => {}
                }
                for (t, s1, p1) in branches {
                    // sans place ou en conflit : l'objet reste tel quel (même règle que le moteur exact), plutôt que de
                    // perdre cette probabilité (qui passerait pour un succès gratuit)
                    v.push((add(s1, t).unwrap_or(s), p1));
                }
            }
            Desecrate if s.rarity == Rarity::Rare && !s.desecrated => {
                // ajoute un mod « Désécré » non révélé, tiré dans le pool restreint (Ulaman/Amanamu/
                // Kurgal, éventuellement un seul via l'Omen the Sovereign/Liege/Blackblooded) ; retire
                // un mod au hasard d'abord SEULEMENT si l'objet est déjà plein à 6.
                let mut base: Vec<(MacroState, f64)> = vec![(s, 1.0)];
                if !self.has_room(&s) {
                    let mut rm = Vec::new();
                    base = if self.remove_outcomes(s, None, Removal::Random, &mut rm) { rm } else { Vec::new() };
                }
                for (s1, p1) in base {
                    let mut ad = Vec::new();
                    if self.add_outcomes(s1, w, &mut ad) {
                        v.extend(ad.into_iter().map(|(t, q)| (MacroState { desecrated: true, desec: Self::added_desec(&s1, &t), ..t }, p1 * q)));
                    } else if s1 != s {
                        // mod retiré mais aucun mod Désécré possible dans la place libérée : l'objet reste tel
                        // quel (même règle que le moteur exact). Perdre cette probabilité la ferait passer pour
                        // un succès gratuit.
                        v.push((s, p1));
                    }
                }
            }
            _ => {}
        }
        merge(&mut v);
        out.extend(v.into_iter().map(|(to, p)| Tr { to, p, extra: 0.0, abandon: false }));
    }
}

// ───────────────────────── Résumés lisibles (plan, overlay) ─────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    pub rarity: Rarity,
    pub held_wanted: Vec<u8>,
    pub blocked_wanted: Vec<u8>,
    pub fractured_wanted: Option<u8>,
    pub bad_prefixes: u8,
    pub bad_suffixes: u8,
}

pub fn summarize(s: &MacroState) -> ItemSummary {
    let bits = |m: u8| (0..6u8).filter(|k| m >> k & 1 == 1).collect::<Vec<_>>();
    ItemSummary {
        rarity: s.rarity,
        held_wanted: bits(s.held),
        blocked_wanted: bits(s.blocked),
        fractured_wanted: (s.frac > 0).then(|| s.frac - 1),
        bad_prefixes: s.bad_p,
        bad_suffixes: s.bad_s,
    }
}

/// Libellé français d'une transition (diff entre deux états abstraits).
pub fn describe_transition(s: &MacroState, t: &MacroState, labels: &[String], abandon: bool) -> String {
    if abandon {
        return "Objet perdu : on repart d'une base neuve".into();
    }
    let name = |k: u8| labels.get(k as usize).cloned().unwrap_or_else(|| format!("#{k}"));
    let mut parts: Vec<String> = Vec::new();
    if s.rarity != t.rarity {
        parts.push(match t.rarity {
            Rarity::Magic => "Devient magique".into(),
            Rarity::Rare => "Devient rare".into(),
            Rarity::Normal => "Devient normal".into(),
        });
    }
    for k in 0..6u8 {
        let (sh, th) = (s.held >> k & 1, t.held >> k & 1);
        let (sb, tb) = (s.blocked >> k & 1, t.blocked >> k & 1);
        if th > sh {
            parts.push(format!("Touche « {} »", name(k)));
        }
        if th < sh {
            parts.push(format!("Perd « {} » (voulu)", name(k)));
        }
        if tb > sb {
            parts.push(format!("Tier trop bas de « {} » (bloque le groupe)", name(k)));
        }
        if tb < sb {
            parts.push(format!("Retire le tier trop bas de « {} »", name(k)));
        }
    }
    if t.frac != s.frac && t.frac > 0 {
        parts.push(format!("Fracture « {} »", name(t.frac - 1)));
    }
    let d = |a: u8, b: u8, what: &str| match b.cmp(&a) {
        std::cmp::Ordering::Greater => Some(format!("Mauvais {what} ajouté")),
        std::cmp::Ordering::Less => Some(format!("Mauvais {what} retiré")),
        _ => None,
    };
    parts.extend(d(s.bad_p, t.bad_p, "préfixe"));
    parts.extend(d(s.bad_s, t.bad_s, "suffixe"));
    if parts.is_empty() {
        "Aucun changement utile".into()
    } else {
        parts.join(" · ")
    }
}

#[cfg(test)]
mod removal_tests {
    use super::*;

    #[test]
    fn lowest_level_probabilities_split_ties_evenly() {
        let (a, b, c): (Dist, Dist, Dist) = (vec![(10, 1.0)], vec![(20, 1.0)], vec![(10, 0.5), (30, 0.5)]);
        assert_eq!(lowest_probs(&[&a, &b]), vec![1.0, 0.0]);
        // c vaut 10 une fois sur deux : ex aequo avec a, départagé à pile ou face
        let p = lowest_probs(&[&a, &c]);
        assert!((p[0] - 0.75).abs() < 1e-12 && (p[1] - 0.25).abs() < 1e-12, "{p:?}");
        let p = lowest_probs(&[&a, &a, &a]);
        assert!(p.iter().all(|x| (x - 1.0 / 3.0).abs() < 1e-12), "{p:?}");
    }
}
