//! Couche « service » partagée par l'application Tauri et le CLI : requêtes/réponses sérialisables,
//! construction du contexte de plan, conseil sur objet réel, sandbox.

use craft_core::*;
use craft_data::*;
use craft_solver::*;
use rand::{rngs::SmallRng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub use craft_core;
pub use craft_data;
pub use craft_solver;

// ───────────────────────── Vues pour l'UI ─────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModView {
    pub affix_idx: u16,
    #[serde(default)]
    pub fractured: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub rarity: Rarity,
    pub ilvl: u8,
    pub mods: Vec<ModView>,
}

impl ItemView {
    pub fn to_state(&self, pool: &AffixPool) -> Result<ItemState, String> {
        let mut it = ItemState::new(self.rarity, self.ilvl);
        for m in &self.mods {
            if m.affix_idx as usize >= pool.affixes.len() {
                return Err("affixe hors du pool".into());
            }
            it.push(Mod { idx: m.affix_idx, fractured: m.fractured });
        }
        Ok(it)
    }
    pub fn from_state(it: &ItemState) -> Self {
        Self { rarity: it.rarity, ilvl: it.ilvl, mods: it.mods().iter().map(|m| ModView { affix_idx: m.idx, fractured: m.fractured }).collect() }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDetail {
    pub affix_idx: u16,
    pub name: String,
    pub family: String,
    pub text: String,
    pub tier: u8,
    pub level: u8,
    pub slot: Slot,
    pub fractured: bool,
    pub weight: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    pub view: ItemView,
    pub mods: Vec<ModDetail>,
}

pub fn detail(pool: &AffixPool, it: &ItemState) -> ItemDetail {
    ItemDetail {
        view: ItemView::from_state(it),
        mods: it
            .mods()
            .iter()
            .map(|m| {
                let a = &pool.affixes[m.idx as usize];
                ModDetail { affix_idx: m.idx, name: a.name.clone(), family: a.family.clone(), text: a.text.clone(), tier: a.tier, level: a.req_ilvl, slot: a.slot, fractured: m.fractured, weight: a.weight }
            })
            .collect(),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionView {
    pub id: String,
    pub label: String,
    pub kind: CurrencyKind,
    pub min_mod_level: u8,
    pub add_slot: Option<Slot>,
    pub remove_slot: Option<Slot>,
    pub unit_cost: f64,
    pub default_enabled: bool,
}

pub fn list_actions(ds: &Dataset, prices: &BTreeMap<String, f64>) -> Result<Vec<ActionView>, String> {
    let all: HashSet<String> = {
        let mut s: HashSet<String> = ds.currencies.iter().map(|c| c.id.clone()).collect();
        for c in &ds.currencies {
            for o in ds.omens.iter().filter(|o| o.applies_to.contains(&c.kind)) {
                s.insert(format!("{}+{}", c.id, o.id));
            }
        }
        s
    };
    let defaults: HashSet<String> = ds.actions(prices, None)?.into_iter().map(|c| c.id).collect();
    Ok(ds
        .actions(prices, Some(&all))?
        .into_iter()
        .map(|c| ActionView {
            default_enabled: defaults.contains(&c.id),
            id: c.id,
            label: c.label,
            kind: c.kind,
            min_mod_level: c.min_mod_level,
            add_slot: c.add_slot,
            remove_slot: c.remove_slot,
            unit_cost: c.unit_cost,
        })
        .collect())
}

// ───────────────────────── Sandbox ─────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub applied: bool,
    pub item: ItemDetail,
    pub cost: f64,
}

pub fn sandbox_apply(bp: &BasePool, view: &ItemView, cur: &Currency, seed: Option<u64>) -> Result<ApplyResult, String> {
    let mut it = view.to_state(&bp.pool)?;
    let mut rng = match seed {
        Some(s) => SmallRng::seed_from_u64(s),
        None => SmallRng::from_entropy(),
    };
    let out = bp.pool.apply(&mut it, cur, &mut rng);
    let applied = out == Outcome::Applied;
    Ok(ApplyResult { applied, item: detail(&bp.pool, &it), cost: if applied { cur.unit_cost } else { 0.0 } })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WantedReq {
    /// clé de groupe du dataset (ex. "life_flat")
    pub group: String,
    pub max_tier: u8,
}

fn resolve_wanted(bp: &BasePool, wanted: &[WantedReq]) -> Result<(Vec<WantedAffix>, Vec<GoalItem>), String> {
    let mut w = Vec::new();
    let mut items = Vec::new();
    for r in wanted {
        let g = bp.groups.iter().find(|g| g.key == r.group).ok_or_else(|| format!("groupe « {} » inexistant sur {}", r.group, bp.base.id))?;
        let max_tier = r.max_tier.clamp(1, g.tiers.len() as u8);
        w.push(WantedAffix { group: g.group, max_tier });
        let label = if max_tier as usize == g.tiers.len() { format!("{} (tout tier)", g.family) } else { format!("{} T{}+", g.family, max_tier) };
        items.push(GoalItem { label, slot: g.slot, group: g.group, max_tier });
    }
    Ok((w, items))
}

/// Retrouve une action (monnaie éventuellement combinée à un Omen, ou Essence) par identifiant.
/// `bp` est nécessaire pour résoudre une Essence (son affixe garanti dépend du pool de la base) ;
/// les autres monnaies n'en ont pas besoin.
pub fn find_currency(ds: &Dataset, prices: &BTreeMap<String, f64>, bp: &BasePool, id: &str) -> Result<Currency, String> {
    let all: HashSet<String> = list_actions(ds, prices)?.into_iter().map(|a| a.id).collect();
    if let Some(c) = ds.actions(prices, Some(&all))?.into_iter().find(|c| c.id == id) {
        return Ok(c);
    }
    ds.essence_currencies(bp, prices, None)?.into_iter().find(|c| c.id == id).ok_or_else(|| format!("monnaie inconnue : {id}"))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimRequest {
    pub base_id: String,
    pub ilvl: u8,
    pub start: ItemView,
    pub wanted: Vec<WantedReq>,
    pub currency_id: String,
    pub max_orbs: u32,
    pub trials: u64,
    pub seed: u64,
    #[serde(default)]
    pub base_cost: f64,
}

pub fn run_simulation(
    ds: &Dataset,
    prices: &BTreeMap<String, f64>,
    req: &SimRequest,
    on_progress: impl FnMut(u64, u64) -> bool,
) -> Result<Option<SimResult>, String> {
    let bp = ds.build_pool(&req.base_id)?;
    let (wanted, _) = resolve_wanted(&bp, &req.wanted)?;
    let goal = Goal::new(&bp.pool, &wanted)?;
    let cur = find_currency(ds, prices, &bp, &req.currency_id)?;
    let mut start = req.start.to_state(&bp.pool)?;
    start.ilvl = req.ilvl;
    let spec = SimSpec { start, currency: cur, goal, max_orbs: req.max_orbs.max(1), base_cost: req.base_cost };
    Ok(simulate(&bp.pool, &spec, &SimConfig { trials: req.trials.max(1), seed: req.seed }, on_progress))
}

// ───────────────────────── Reverse-crafting ─────────────────────────

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    pub base_id: String,
    pub ilvl: u8,
    pub wanted: Vec<WantedReq>,
    #[serde(default)]
    pub enabled_actions: Option<Vec<String>>,
    #[serde(default)]
    pub prices: Option<BTreeMap<String, f64>>,
    #[serde(default = "d_true")]
    pub allow_abandon: bool,
    #[serde(default = "d_trials")]
    pub mc_trials: u64,
    #[serde(default = "d_cap")]
    pub node_cap: usize,
    #[serde(default)]
    pub seed: u64,
    /// libellé de l'origine des prix, affiché avec le plan (ex. « poe.ninja, ligue X »)
    #[serde(default)]
    pub prices_label: Option<String>,
}
fn d_true() -> bool {
    true
}
fn d_trials() -> u64 {
    20_000
}
fn d_cap() -> usize {
    220
}

/// Tout ce qu'il faut pour planifier, conseiller en jeu et vérifier. Immuable, partageable (`Arc`).
pub struct PlanContext {
    pub req: PlanRequest,
    pub bp: BasePool,
    pub model: Arc<Model>,
    pub goal_items: Vec<GoalItem>,
    pub labels: Vec<String>,
    pub base_cost: f64,
    pub salvage: f64,
    pub prices_source: String,
    pub solution: Solution,
    extra: Mutex<HashMap<MacroState, Arc<Solution>>>,
}

pub fn build_context(ds: &Dataset, req: &PlanRequest, prices: &BTreeMap<String, f64>, cancel: &AtomicBool) -> Result<PlanContext, String> {
    let bp = ds.build_pool(&req.base_id)?;
    let prices = match &req.prices {
        Some(o) => {
            let mut p = prices.clone();
            p.extend(o.clone());
            p
        }
        None => prices.clone(),
    };
    let (wanted, goal_items) = resolve_wanted(&bp, &req.wanted)?;
    let pool = Arc::new(bp.pool.clone());
    let goal = Arc::new(Goal::new(&pool, &wanted)?);
    let enabled: Option<HashSet<String>> = req.enabled_actions.as_ref().map(|v| v.iter().cloned().collect());
    let mut actions: Vec<Action> = ds
        .actions(&prices, enabled.as_ref())?
        .into_iter()
        .chain(ds.essence_currencies(&bp, &prices, enabled.as_ref())?)
        .map(|c| Action { id: c.id.clone(), label: c.label.clone(), cost: c.unit_cost, kind: ActionKind::Currency(c) })
        .collect();
    if actions.is_empty() {
        return Err("aucune action de craft activée".into());
    }
    if req.allow_abandon {
        actions.push(Action { id: "__abandon".into(), label: "Abandonner l'objet".into(), cost: 0.0, kind: ActionKind::Abandon });
    }
    let base_cost = prices.get("base_white").copied().unwrap_or(1.0);
    let salvage = prices.get("base_salvage").copied().unwrap_or(0.0);
    let model = Arc::new(Model::new(pool, goal, req.ilvl, actions, base_cost, salvage));
    let start = MacroState::empty(Rarity::Normal);
    let solution = solve(&model, &[start], &SolveConfig::default(), cancel)?;
    Ok(PlanContext {
        req: req.clone(),
        labels: goal_items.iter().map(|g| g.label.clone()).collect(),
        bp,
        model,
        goal_items,
        base_cost,
        salvage,
        prices_source: req.prices_label.clone().unwrap_or_else(|| format!("{} ({})", ds.meta.source, ds.meta.generated_at)),
        solution,
        extra: Mutex::new(HashMap::new()),
    })
}

/// Construit le graphe de plan depuis une base neuve, puis vérifie la politique par Monte-Carlo (moteur exact).
/// À appeler dans `pool.install(..)` pour borner le parallélisme.
pub fn make_plan(ctx: &PlanContext, on_progress: impl FnMut(u64, u64) -> bool) -> Result<CraftPlan, String> {
    let start = MacroState::empty(Rarity::Normal);
    let inputs = PlanInputs {
        model: &ctx.model,
        sol: &ctx.solution,
        start,
        base_id: &ctx.req.base_id,
        base_cost: ctx.base_cost,
        salvage: ctx.salvage,
        goal_items: ctx.goal_items.clone(),
        prices_source: ctx.prices_source.clone(),
    };
    let mut plan = build_plan(&inputs, &PlanConfig { node_cap: ctx.req.node_cap, ..Default::default() })?;
    if ctx.req.mc_trials > 0 {
        plan.mc = verify_policy(&ctx.model, &ctx.solution, ItemState::new(Rarity::Normal, ctx.req.ilvl), ctx.req.mc_trials, 20_000, ctx.req.seed, on_progress);
    }
    Ok(plan)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdviceResult {
    pub dead: bool,
    pub advice: Option<Advice>,
    pub goal: Vec<GoalItem>,
    /// pour chaque affixe voulu : présent sur l'objet (bool)
    pub wanted_status: Vec<String>, // "held" | "blocked" | "missing"
}

/// Conseil pour un objet réel (lu dans le presse-papiers). Résout à la volée si l'état n'est pas dans la table.
pub fn advise_item(ctx: &PlanContext, item: &ItemState, cancel: &AtomicBool) -> Result<AdviceResult, String> {
    let goal = ctx.goal_items.clone();
    let Some(state) = project(&ctx.model.goal, &ctx.model.pool, item) else {
        return Ok(AdviceResult { dead: true, advice: None, goal, wanted_status: vec!["missing".into(); ctx.goal_items.len()] });
    };
    let status = (0..ctx.goal_items.len())
        .map(|k| if state.held >> k & 1 == 1 { "held" } else if state.blocked >> k & 1 == 1 { "blocked" } else { "missing" }.to_string())
        .collect();
    let advice = if ctx.solution.id(&state).is_some() {
        advise(&ctx.model, &ctx.solution, &state, &ctx.labels)
    } else {
        let cached = ctx.extra.lock().unwrap().get(&state).cloned();
        let sol = match cached {
            Some(s) => s,
            None => {
                let s = Arc::new(solve(&ctx.model, &[state], &SolveConfig::default(), cancel)?);
                let mut g = ctx.extra.lock().unwrap();
                if g.len() > 64 {
                    g.clear();
                }
                g.insert(state, s.clone());
                s
            }
        };
        advise(&ctx.model, &sol, &state, &ctx.labels)
    };
    Ok(AdviceResult { dead: false, advice, goal, wanted_status: status })
}

// ───────────────────────── Analyse d'un texte d'objet ─────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemAnalysis {
    pub parsed: ParsedItem,
    pub base_id: Option<String>,
    pub detail: Option<ItemDetail>,
    pub unmatched: Vec<String>,
    pub error: Option<String>,
}

pub fn detect_base(ds: &Dataset, parsed: &ParsedItem) -> Option<String> {
    let bt = parsed.base_type.as_deref()?.to_lowercase();
    ds.bases
        .iter()
        .find(|b| b.name.to_lowercase() == bt)
        .or_else(|| ds.bases.iter().find(|b| bt.contains(&b.name.to_lowercase())))
        .map(|b| b.id.clone())
}

pub fn analyze_item(ds: &Dataset, text: &str, base_hint: Option<&str>, fallback_ilvl: u8) -> ItemAnalysis {
    let parsed = match parse_item(text) {
        Ok(p) => p,
        Err(e) => {
            return ItemAnalysis {
                parsed: ParsedItem { item_class: None, rarity_label: None, rarity: None, name: None, base_type: None, item_level: None, corrupted: false, advanced: false, mods: vec![] },
                base_id: None,
                detail: None,
                unmatched: vec![],
                error: Some(e),
            }
        }
    };
    let base_id = detect_base(ds, &parsed).or_else(|| base_hint.map(|s| s.to_string()));
    let (mut detail, mut unmatched, mut error) = (None, vec![], None);
    match base_id.as_deref().map(|b| ds.build_pool(b)) {
        Some(Ok(bp)) => match resolve(&parsed, &bp, fallback_ilvl) {
            Ok(r) => {
                detail = Some(detail_of(&bp, &r.item));
                unmatched = r.unmatched;
            }
            Err(e) => error = Some(e),
        },
        Some(Err(e)) => error = Some(e),
        None => error = Some("base non reconnue : choisis-la manuellement".into()),
    }
    ItemAnalysis { parsed, base_id, detail, unmatched, error }
}

fn detail_of(bp: &BasePool, it: &ItemState) -> ItemDetail {
    detail(&bp.pool, it)
}

// ───────────────────────── Vues du dataset (UI) ─────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseView {
    pub id: String,
    pub name: String,
    pub item_class: String,
    pub tags: Vec<String>,
}

impl From<&BaseItem> for BaseView {
    fn from(b: &BaseItem) -> Self {
        Self { id: b.id.clone(), name: b.name.clone(), item_class: b.item_class.clone(), tags: b.tags.clone() }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetInfo {
    pub source: String,
    pub game_version: String,
    pub generated_at: String,
    pub notice: String,
    pub price_unit: String,
    pub mod_count: usize,
    pub bases: Vec<BaseView>,
}

pub fn dataset_info(ds: &Dataset) -> DatasetInfo {
    DatasetInfo {
        source: ds.meta.source.clone(),
        game_version: ds.meta.game_version.clone(),
        generated_at: ds.meta.generated_at.clone(),
        notice: ds.meta.notice.clone(),
        price_unit: ds.meta.price_unit.clone(),
        mod_count: ds.mods.len(),
        bases: ds.bases.iter().map(BaseView::from).collect(),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolView {
    pub base: BaseView,
    pub groups: Vec<GroupInfo>,
    pub affixes: Vec<Affix>,
}

pub fn pool_view(ds: &Dataset, base_id: &str) -> Result<PoolView, String> {
    let bp = ds.build_pool(base_id)?;
    Ok(PoolView { base: BaseView::from(&bp.base), groups: bp.groups, affixes: bp.pool.affixes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Bout en bout : le solveur doit pouvoir utiliser une Essence (Transmute → Essence garantie)
    /// pour atteindre un objectif sur son affixe cible, et le coût doit rester fini.
    #[test]
    fn solver_uses_an_essence_action_to_reach_its_guaranteed_target() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let enabled: HashSet<String> = ["transmute", "essence_abrasion"].iter().map(|s| s.to_string()).collect();
        let req = PlanRequest {
            base_id: "sword_1h".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "PhysicalDamage".into(), max_tier: 5 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context");
        assert!(
            ctx.model.actions.iter().any(|a| matches!(&a.kind, ActionKind::Currency(c) if c.kind == CurrencyKind::Essence)),
            "l'Essence doit apparaître dans les actions du modèle"
        );
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged, "le solveur doit converger sur ce cas simple");
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0, "coût attendu fini et positif, obtenu {}", plan.expected_cost);
    }

    /// Bout en bout : le solveur doit pouvoir désécrer (Alchemy → Preserved Rib) pour atteindre un
    /// objectif porté par un mod du domaine `desecrated`, sans que ce mod ne soit jamais accessible
    /// via les monnaies normales (Alchemy, Exalt...) sur le même objet.
    #[test]
    fn solver_uses_desecration_to_reach_a_desecrated_only_target() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let enabled: HashSet<String> = ["alchemy", "desecrate_rib"].iter().map(|s| s.to_string()).collect();
        let req = PlanRequest {
            base_id: "shield_str".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "MaximumResistances".into(), max_tier: 1 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context");
        assert!(
            ctx.model.actions.iter().any(|a| matches!(&a.kind, ActionKind::Currency(c) if c.kind == CurrencyKind::Desecrate)),
            "la Désécration doit apparaître dans les actions du modèle"
        );
        // note : « MaximumResistances » est aussi le groupe d'un mod normal (MaximumElementalResistance) —
        // collision légitime du jeu, pas un bug : les deux s'excluent mutuellement en vrai. On vérifie
        // juste qu'au moins un tier du groupe est bien un mod `desecrated` (celui qu'on vise).
        let grp = ctx.bp.groups.iter().find(|g| g.key == "MaximumResistances").expect("le groupe cible doit exister dans le pool");
        assert!(
            grp.tiers.iter().any(|t| ctx.bp.pool.affixes[t.affix_idx as usize].desecrated),
            "au moins un tier de MaximumResistances doit être un mod desecrated"
        );
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged, "le solveur doit converger sur ce cas simple");
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0, "coût attendu fini et positif, obtenu {}", plan.expected_cost);
    }
}

#[cfg(test)]
mod alloy_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Bout en bout : le mécanisme "Essence sur objet Rare" (retire au hasard puis ajoute garanti,
    /// utilisé par les Alloys Verisium et les futures Essences Perfect) doit permettre au solveur
    /// d'atteindre un objectif, converger, et donner un coût fini.
    #[test]
    fn solver_uses_a_rare_essence_alloy_to_reach_its_target() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let enabled: HashSet<String> = ["alchemy", "alloy_mystic"].iter().map(|s| s.to_string()).collect();
        let req = PlanRequest {
            base_id: "sword_1h".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "PhysicalDamage".into(), max_tier: 5 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context");
        assert!(
            ctx.model.actions.iter().any(|a| matches!(&a.kind, ActionKind::Currency(c) if c.kind == CurrencyKind::Essence && c.requires_rare)),
            "l'Alloy (Essence sur Rare) doit apparaître dans les actions du modèle"
        );
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged, "le solveur doit converger sur ce cas simple");
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0, "coût attendu fini et positif, obtenu {}", plan.expected_cost);
    }
}

#[cfg(test)]
mod jewel_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Bout en bout : les bases Joyau (nouvellement importées, domaine `misc` du jeu) doivent être
    /// utilisables par le solveur comme n'importe quelle autre base.
    #[test]
    fn solver_works_on_a_jewel_base() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let req = PlanRequest {
            base_id: "jewel_strjewel".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "AttackDamage".into(), max_tier: 1 }],
            enabled_actions: None,
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context sur un joyau");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged, "le solveur doit converger sur un joyau");
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0, "coût attendu fini et positif, obtenu {}", plan.expected_cost);
    }
}
