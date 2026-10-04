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
        // clé exacte (« Groupe » ou « Groupe::Famille »), sinon ancienne clé de groupe seule (objectifs
        // enregistrés avant le découpage en familles) : on prend alors la famille qui a le plus de tiers
        let g = bp
            .groups
            .iter()
            .find(|g| g.key == r.group)
            .or_else(|| bp.groups.iter().filter(|g| g.key.split("::").next() == Some(r.group.as_str())).max_by_key(|g| g.tiers.len()))
            .ok_or_else(|| format!("groupe « {} » inexistant sur {}", r.group, bp.base.id))?;
        let max_tier = r.max_tier.clamp(1, g.tiers.len() as u8);
        w.push(WantedAffix { group: g.group, family: g.family_id, max_tier });
        let label = if max_tier as usize == g.tiers.len() { format!("{} (tout tier)", g.family) } else { format!("{} T{}+", g.family, max_tier) };
        items.push(GoalItem { label, slot: g.slot, group: g.group, family_id: g.family_id, max_tier });
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
    /// Objet déjà existant dont on veut repartir (au lieu d'une base neuve). Doit être compatible avec
    /// `base_id` (mêmes affixes résolubles) ; `None` = objet neuf (Normal, aucun mod), comme avant.
    #[serde(default)]
    pub starting_item: Option<ItemView>,
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
    /// État de départ réellement résolu (objet neuf, ou projection de `req.starting_item`) — celui sur
    /// lequel `ctx.solution` a été calculée. `make_plan`/`verify_policy` doivent repartir d'ici, jamais
    /// d'un `MacroState::empty` recalculé indépendamment, sous peine d'ignorer un objet déjà existant.
    pub start: MacroState,
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
    let model = Arc::new(Model::new(pool.clone(), goal.clone(), req.ilvl, actions, base_cost, salvage));
    let start = match &req.starting_item {
        Some(view) => {
            let item = view.to_state(&pool).map_err(|e| format!("objet de départ invalide : {e}"))?;
            craft_solver::project(&goal, &pool, &item).ok_or_else(|| "objet de départ incompatible avec cet objectif (affixe fracturé non voulu, ou hors de ce que le solveur sait représenter)".to_string())?
        }
        None => MacroState::empty(Rarity::Normal),
    };
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
        start,
        extra: Mutex::new(HashMap::new()),
    })
}

/// Construit le graphe de plan depuis l'état de départ déjà résolu par `build_context` (objet neuf, ou
/// objet existant fourni), puis vérifie la politique par Monte-Carlo (moteur exact).
/// À appeler dans `pool.install(..)` pour borner le parallélisme.
pub fn make_plan(ctx: &PlanContext, on_progress: impl FnMut(u64, u64) -> bool) -> Result<CraftPlan, String> {
    let start = ctx.start;
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
        let start_item = match &ctx.req.starting_item {
            Some(view) => view.to_state(&ctx.model.pool)?,
            None => ItemState::new(Rarity::Normal, ctx.req.ilvl),
        };
        plan.mc = verify_policy(&ctx.model, &ctx.solution, start_item, ctx.req.mc_trials, 20_000, ctx.req.seed, on_progress);
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

/// Base d'un objet collé : nom exact, sinon le nom de base le PLUS LONG contenu dans la ligne de type
/// (un objet Magique s'appelle « Glowing Sapphire Ring of the Fox » : « Sapphire Ring » doit l'emporter
/// sur « Ring »). Plusieurs bases portent le même nom (Two-Stone Ring, Runemastered ...) : on garde
/// `hint` s'il en fait partie, sinon celle dont les implicites correspondent à ceux de l'objet.
pub fn detect_base(ds: &Dataset, parsed: &ParsedItem, hint: Option<&str>) -> Option<String> {
    let bt = parsed.base_type.as_deref()?.to_lowercase();
    let name = ds
        .bases
        .iter()
        .find(|b| b.name.to_lowercase() == bt)
        .or_else(|| ds.bases.iter().filter(|b| bt.contains(&b.name.to_lowercase())).max_by_key(|b| b.name.len()))?
        .name
        .clone();
    let same: Vec<&BaseItem> = ds.bases.iter().filter(|b| b.name == name).collect();
    if let Some(h) = hint.and_then(|h| same.iter().find(|b| b.id == h)) {
        return Some(h.id.clone());
    }
    let shape = |t: &str| implicit_shape().replace_all(t, "#").to_lowercase();
    let item_implicits: Vec<String> = parsed.mods.iter().filter(|m| m.kind == ModKind::Implicit).flat_map(|m| m.lines.iter().map(|l| shape(l))).collect();
    same.iter()
        .find(|b| !b.implicits.is_empty() && b.implicits.iter().flat_map(|t| t.split(" / ")).all(|t| item_implicits.contains(&shape(t))))
        .or(same.first())
        .map(|b| b.id.clone())
}

/// Nombres et plages (« 14 », « (12-16) », « 14(12-16) » du format avancé, « 0.5 ») remplacés par « # » pour comparer un implicite
/// réel (valeur tirée) au gabarit de la base (plage).
fn implicit_shape() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"(?:\d+(?:\.\d+)?)?\(\d+(?:\.\d+)?[-–—]\d+(?:\.\d+)?\)|\d+(?:\.\d+)?").unwrap())
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
    let base_id = detect_base(ds, &parsed, base_hint).or_else(|| base_hint.map(|s| s.to_string()));
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
    pub implicits: Vec<String>,
    /// plafond d'un objet Rare de cette base (3/3 sauf implicite qui le décale)
    pub max_prefixes: u8,
    pub max_suffixes: u8,
}

impl From<&BaseItem> for BaseView {
    fn from(b: &BaseItem) -> Self {
        let (max_prefixes, max_suffixes) = AffixPool { affixes: vec![], cap_delta: (b.prefix_cap_delta, b.suffix_cap_delta) }.cap(Rarity::Rare);
        Self {
            id: b.id.clone(),
            name: b.name.clone(),
            item_class: b.item_class.clone(),
            tags: b.tags.clone(),
            implicits: b.implicits.clone(),
            max_prefixes,
            max_suffixes,
        }
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
                starting_item: None,
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
            wanted: vec![WantedReq { group: "MaximumResistances::Maximum Resistances (Amanamu)".into(), max_tier: 1 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
                starting_item: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context");
        assert!(
            ctx.model.actions.iter().any(|a| matches!(&a.kind, ActionKind::Currency(c) if c.kind == CurrencyKind::Desecrate)),
            "la Désécration doit apparaître dans les actions du modèle"
        );
        // note : « MaximumResistances » est aussi le groupe d'un mod normal (MaximumElementalResistance) —
        // collision légitime du jeu, pas un bug : les deux s'excluent mutuellement en vrai. On vérifie
        // juste qu'au moins un tier du groupe est bien un mod `desecrated` (celui qu'on vise).
        let grp = ctx.bp.groups.iter().find(|g| g.key == "MaximumResistances::Maximum Resistances (Amanamu)").expect("le groupe cible doit exister dans le pool");
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
            base_id: "helmet_str".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "AlloySpellAoE".into(), max_tier: 1 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
                starting_item: None,
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
                starting_item: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context sur un joyau");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged, "le solveur doit converger sur un joyau");
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0, "coût attendu fini et positif, obtenu {}", plan.expected_cost);
    }
}

#[cfg(test)]
mod liquid_emotion_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Bout en bout : Diluted Liquid Ire (Alchemy -> Rare, puis Liquid Ire garanti "increased Armour")
    /// doit permettre au solveur d'atteindre l'objectif sur un joyau Rubis.
    #[test]
    fn solver_uses_a_liquid_emotion_on_a_ruby_jewel() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let enabled: HashSet<String> = ["alchemy", "liquid_ire"].iter().map(|s| s.to_string()).collect();
        let req = PlanRequest {
            base_id: "jewel_strjewel".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "IncreasedPhysicalDamageReductionRatingPercent".into(), max_tier: 1 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
                starting_item: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context sur Rubis");
        assert!(
            ctx.model.actions.iter().any(|a| matches!(&a.kind, ActionKind::Currency(c) if c.id == "liquid_ire")),
            "liquid_ire doit apparaître dans les actions du modèle pour un Rubis"
        );
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged, "le solveur doit converger");
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0, "coût attendu fini et positif, obtenu {}", plan.expected_cost);
    }
}

#[cfg(test)]
mod weapon_class_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Bout en bout : les bases Talisman et Trap (nouvellement importées) doivent être utilisables.
    #[test]
    fn solver_works_on_talisman_and_trap_bases() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        for base_id in ["talisman", "trap"] {
            let group = if base_id == "trap" { "Dexterity" } else { "Strength" };
            let req = PlanRequest {
                base_id: base_id.into(),
                ilvl: 82,
                wanted: vec![WantedReq { group: group.into(), max_tier: 1 }],
                enabled_actions: None,
                prices: None,
                allow_abandon: true,
                mc_trials: 0,
                node_cap: 50,
                seed: 1,
                prices_label: None,
                starting_item: None,
            };
            let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).unwrap_or_else(|e| panic!("build_context sur {base_id} : {e}"));
            let plan = make_plan(&ctx, |_, _| true).unwrap_or_else(|e| panic!("make_plan sur {base_id} : {e}"));
            assert!(plan.solver.converged, "le solveur doit converger sur {base_id}");
            assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0, "coût fini et positif sur {base_id}");
        }
    }
}

#[cfg(test)]
mod starting_item_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Bout en bout : partir d'un objet déjà Magique avec un bon mod garanti doit coûter, en moyenne,
    /// moins cher que partir d'un objet neuf pour atteindre le même objectif.
    #[test]
    fn solver_starts_from_an_existing_item_instead_of_a_fresh_base() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let bp = ds.build_pool("sword_1h").unwrap();
        let target_idx = bp.pool.affixes.iter().position(|a| a.id == "LocalAddedPhysicalDamage5").expect("cible de test introuvable dans le pool");

        let base_req = PlanRequest {
            base_id: "sword_1h".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "PhysicalDamage".into(), max_tier: 5 }],
            enabled_actions: None,
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 100,
            seed: 1,
            prices_label: None,
            starting_item: None,
        };
        let fresh = build_context(&ds, &base_req, &prices, &AtomicBool::new(false)).expect("build_context (neuf)");
        let fresh_plan = make_plan(&fresh, |_, _| true).expect("make_plan (neuf)");

        let mut with_start = base_req.clone();
        with_start.starting_item = Some(ItemView { rarity: Rarity::Magic, ilvl: 82, mods: vec![ModView { affix_idx: target_idx as u16, fractured: false }] });
        let ctx2 = build_context(&ds, &with_start, &prices, &AtomicBool::new(false)).expect("build_context (objet existant)");
        let plan2 = make_plan(&ctx2, |_, _| true).expect("make_plan (objet existant)");

        assert!(plan2.solver.converged);
        assert!(
            plan2.expected_cost < fresh_plan.expected_cost,
            "partir d'un objet qui a déjà le bon mod (Magique, T5 garanti) devrait coûter moins cher ({} vs objet neuf {})",
            plan2.expected_cost,
            fresh_plan.expected_cost
        );
    }
}

#[cfg(test)]
mod remaining_alloy_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Bout en bout : Sovereign Alloy sur un Focus doit résoudre vers la cible "armure" (Ward), jamais
    /// vers la cible "arme" — vérifie que le tag générique `weapon` n'empiète pas sur focus/staff/wand.
    #[test]
    fn sovereign_alloy_targets_armour_not_weapon_on_a_focus() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let enabled: HashSet<String> = ["alchemy", "alloy_sovereign"].iter().map(|s| s.to_string()).collect();
        let req = PlanRequest {
            base_id: "focus".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "AlloyWardPercent".into(), max_tier: 1 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context sur focus");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged, "doit converger : Sovereign Alloy doit bien cibler le Ward sur focus");
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0);
    }
}

#[cfg(test)]
mod jewellery_base_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Bout en bout : le solveur calcule un plan sur une base de bijou nouvellement importée (une des
    /// trois Two-Stone Ring), et un objet collé de cette base est reconnu comme CETTE variante — par son
    /// implicite, ou par le plan actif quand il s'agit d'une base au même nom.
    #[test]
    fn solver_and_item_detection_on_a_specific_jewellery_base() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let base_id = "two_stone_ring_fire_lightning_resistance";
        let req = PlanRequest {
            base_id: base_id.into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "IncreasedLife".into(), max_tier: 2 }],
            enabled_actions: None,
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged);
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0);

        let advanced = "Item Class: Rings\nRarity: Magic\nTwo-Stone Ring of the Whelpling\n--------\nItem Level: 82\n--------\n{ Implicit Modifier — Elemental, Fire, Lightning, Resistance }\n+14(12-16)% to Fire and Lightning Resistances (implicit)\n--------\n{ Suffix Modifier \"of the Whelpling\" (Tier: 8) — Life }\n+8(5-8) to maximum Life\n";
        let a = analyze_item(&ds, advanced, None, 82);
        assert_eq!(a.base_id.as_deref(), Some(base_id), "reconnue par son implicite (format avancé)");
        let basic = "Item Class: Rings\nRarity: Normal\nTwo-Stone Ring\n--------\nItem Level: 82\n--------\n+13% to Fire and Lightning Resistances (implicit)\n";
        assert_eq!(analyze_item(&ds, basic, None, 82).base_id.as_deref(), Some(base_id), "reconnue par son implicite (format simple)");
        // implicite absent du texte : la variante du plan actif l'emporte sur la première du même nom
        let bare = "Item Class: Rings\nRarity: Normal\nTwo-Stone Ring\n--------\nItem Level: 82\n";
        assert_eq!(analyze_item(&ds, bare, Some(base_id), 82).base_id.as_deref(), Some(base_id));
        // nom de base le plus long : « Sapphire Ring » et pas « Ring » dans un nom d'objet Magique
        let magic = "Item Class: Rings\nRarity: Magic\nGlowing Sapphire Ring of the Fox\n--------\nItem Level: 82\n";
        assert_eq!(analyze_item(&ds, magic, None, 82).base_id.as_deref(), Some("sapphire_ring"));
    }
}

#[cfg(test)]
mod base_cap_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Bout en bout sur les vraies données : 4 préfixes voulus sont refusés sur une Sapphire Ring (3/3)
    /// mais planifiés sur une Penumbra Ring (+2 préfixes / -2 suffixes, soit 5/1 en Rare).
    #[test]
    fn four_prefixes_are_planned_on_a_penumbra_ring_only() {
        let ds = Dataset::embedded();
        let prices = ds.prices.clone();
        let req = |base: &str| PlanRequest {
            base_id: base.into(),
            ilvl: 82,
            wanted: ["IncreasedLife", "IncreasedMana", "FireDamage", "ColdDamage"].iter().map(|g| WantedReq { group: g.to_string(), max_tier: 20 }).collect(),
            enabled_actions: None,
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: None,
        };
        assert!(build_context(&ds, &req("sapphire_ring"), &prices, &AtomicBool::new(false)).is_err(), "4 préfixes impossibles sur une base 3/3");
        let ctx = build_context(&ds, &req("penumbra_ring"), &prices, &AtomicBool::new(false)).expect("build_context sur Penumbra Ring");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged);
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0);
        let info = dataset_info(&ds);
        let b = info.bases.iter().find(|b| b.id == "penumbra_ring").unwrap();
        assert_eq!((b.max_prefixes, b.max_suffixes), (5, 1));
    }
}

#[cfg(test)]
mod family_split_tests {
    use super::*;
    use craft_core::goal::{Class, Goal};
    use std::sync::atomic::AtomicBool;

    /// Bout en bout sur les vraies données : sur une baguette, le groupe IncreaseSocketedGemLevel est
    /// découpé en familles (sorts, sorts de feu, de froid...) qui ont chacune leurs propres tiers, et viser
    /// « sorts de feu » n'accepte pas un « niveau de tous les sorts », qui occupe pourtant le même groupe.
    #[test]
    fn spell_level_families_have_their_own_tiers_on_a_wand() {
        let ds = Dataset::embedded();
        let bp = ds.build_pool("wand").expect("pool de la baguette");
        let fams: Vec<&GroupInfo> = bp.groups.iter().filter(|g| g.key.starts_with("IncreaseSocketedGemLevel::")).collect();
        let fire = fams.iter().find(|g| g.family == "+# to Level of all Fire Spell Skills").expect("famille sorts de feu");
        let spell = fams.iter().find(|g| g.family == "+# to Level of all Spell Skills").expect("famille tous les sorts");
        assert_eq!(fire.tiers.len(), 5, "5 tiers de sorts de feu sur baguette (RePoE 4.5.5.2)");
        assert_eq!(spell.tiers.len(), 4, "4 tiers de tous les sorts sur baguette (RePoE 4.5.5.2)");
        assert!(fire.tiers.iter().all(|t| t.text.contains("Fire Spell")));
        assert!(spell.tiers.iter().all(|t| t.text.contains("all Spell Skills")));
        assert_eq!(fire.group, spell.group, "même groupe d'exclusion");
        assert_ne!(fire.family_id, spell.family_id);

        let prices = ds.prices.clone();
        let req = PlanRequest {
            base_id: "wand".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: fire.key.clone(), max_tier: 2 }],
            enabled_actions: None,
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context");
        assert_eq!(ctx.goal_items[0].family_id, fire.family_id);
        assert!(ctx.goal_items[0].label.starts_with("+# to Level of all Fire Spell Skills"));
        let goal = Goal::new(&ctx.bp.pool, &[WantedAffix { group: fire.group, family: fire.family_id, max_tier: 2 }]).unwrap();
        assert_eq!(goal.classify(&ctx.bp.pool, fire.tiers[0].affix_idx), Class::Wanted(0));
        assert_eq!(goal.classify(&ctx.bp.pool, fire.tiers[4].affix_idx), Class::Blocked(0), "T5 de feu : tier insuffisant");
        assert_eq!(goal.classify(&ctx.bp.pool, spell.tiers[0].affix_idx), Class::Blocked(0), "T1 de tous les sorts : autre famille du groupe");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged);
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0);

        // ancienne clé de groupe seule (objectif enregistré avant le découpage) : toujours acceptée
        let legacy = PlanRequest { wanted: vec![WantedReq { group: "IncreaseSocketedGemLevel".into(), max_tier: 1 }], ..req };
        assert!(build_context(&ds, &legacy, &prices, &AtomicBool::new(false)).is_ok());
    }
}

#[cfg(test)]
mod greater_perfect_essence_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Mod garanti par une Essence sur une base, tel que le résout le solveur.
    fn essence_target(ds: &Dataset, base: &str, essence: &str) -> Option<String> {
        let bp = ds.build_pool(base).unwrap();
        let acts = ds.essence_currencies(&bp, &ds.prices, None).unwrap();
        acts.iter().find(|c| c.id == essence).map(|c| bp.pool.affixes[c.target.unwrap() as usize].id.clone())
    }

    /// Cibles vérifiées (craftofexile, onglet Essences ; poe2db pour l'arbalète, rangée « Two Handed
    /// Melee Weapon or Crossbow ») : le bon tier selon la classe d'objet.
    #[test]
    fn greater_and_perfect_essences_resolve_the_verified_mod_per_item_class() {
        let ds = Dataset::embedded();
        let t = |base: &str, e: &str| essence_target(&ds, base, e);
        assert_eq!(t("bow", "essence_abrasion_greater").as_deref(), Some("LocalAddedPhysicalDamage7"));
        assert_eq!(t("crossbow", "essence_abrasion_greater").as_deref(), Some("LocalAddedPhysicalDamageTwoHand7"));
        assert_eq!(t("helmet_str", "essence_body_greater").as_deref(), Some("IncreasedLife8"));
        assert_eq!(t("boots_str", "essence_body_greater").as_deref(), Some("IncreasedLife7"), "bottes : +(85-99), pas +(100-119)");
        assert_eq!(t("ring", "essence_body_greater"), None, "pas d'Essence of the Body sur un anneau");
        assert_eq!(t("focus", "essence_enhancement_greater").as_deref(), Some("LocalIncreasedEnergyShieldPercent5"));
        assert_eq!(t("bow", "essence_haste_greater").as_deref(), Some("LocalIncreasedAttackSpeed4"));
        assert_eq!(t("sword_2h", "essence_haste_greater").as_deref(), Some("LocalIncreasedAttackSpeed7"));
        assert_eq!(t("bow", "essence_flames_perfect").as_deref(), Some("EssenceDamageasExtraFire1"));
        assert_eq!(t("crossbow", "essence_flames_perfect").as_deref(), Some("EssenceDamageasExtraFire2H"));
        assert_eq!(t("wand", "essence_sorcery_perfect").as_deref(), Some("EssenceSpellSkillLevel1H1"));
        assert_eq!(t("focus", "essence_sorcery_perfect"), None, "Perfect Sorcery : baguette et bâton seulement");
        // Lesser et normales réalignées sur les mêmes sources (avant : mauvais tags ou mods sur mesure)
        assert_eq!(t("sword_1h", "essence_haste").as_deref(), Some("LocalIncreasedAttackSpeed5"));
        assert_eq!(t("crossbow", "essence_haste_lesser").as_deref(), Some("LocalIncreasedAttackSpeed2"));
        assert_eq!(t("bow", "essence_battle").as_deref(), Some("LocalIncreasedAccuracy5"));
        assert_eq!(t("mace_2h", "essence_seeking").as_deref(), Some("LocalCriticalStrikeChance3"));
        assert_eq!(t("staff", "essence_sorcery").as_deref(), Some("SpellDamageOnTwoHandWeapon4"));
        assert_eq!(t("boots_str", "essence_body").as_deref(), Some("IncreasedLife6"));
        assert_eq!(t("ring", "essence_mind").as_deref(), Some("IncreasedMana7"));
        assert_eq!(t("focus", "essence_enhancement_lesser").as_deref(), Some("LocalIncreasedEnergyShieldPercent2"));
        assert!(ds.essences.iter().all(|e| !e.id.starts_with("essence_infinite")), "the Infinite tire un attribut au hasard : non représentable");
        // un arc porte aussi le tag two_hand_weapon : seule la variante « une main » est réservée dans son pool
        let bow = ds.build_pool("bow").unwrap();
        assert!(bow.pool.affixes.iter().any(|a| a.id == "EssenceDamageasExtraFire1"));
        assert!(bow.pool.affixes.iter().all(|a| a.id != "EssenceDamageasExtraFire2H"));
        // jamais tiré au hasard
        assert!(bow.pool.affixes.iter().filter(|a| a.id.starts_with("EssenceDamageasExtra")).all(|a| a.weight == 0));
    }

    fn plan_with(ds: &Dataset, base: &str, enabled: &[&str], target_mod: &str) -> CraftPlan {
        let bp = ds.build_pool(base).unwrap();
        let g = bp.groups.iter().find(|g| g.tiers.iter().any(|t| bp.pool.affixes[t.affix_idx as usize].id == target_mod)).expect("groupe de la cible");
        let tier = g.tiers.iter().find(|t| bp.pool.affixes[t.affix_idx as usize].id == target_mod).unwrap().tier;
        let req = PlanRequest {
            base_id: base.into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: g.key.clone(), max_tier: tier }],
            enabled_actions: Some(enabled.iter().map(|s| s.to_string()).collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: None,
        };
        let ctx = build_context(ds, &req, &ds.prices, &AtomicBool::new(false)).expect("build_context");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged);
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0);
        plan
    }

    fn used(plan: &CraftPlan, id: &str) -> f64 {
        plan.shopping.iter().find(|l| l.id == id).map_or(0.0, |l| l.expected_count)
    }

    /// Bout en bout : Transmutation → Greater Essence of Flames sur une arbalète atteint « Adds (56-70)
    /// to (84-107) Fire Damage » ; le plan achète réellement l'Essence.
    #[test]
    fn solver_buys_a_greater_essence_to_reach_its_tier() {
        let ds = Dataset::embedded();
        let plan = plan_with(&ds, "crossbow", &["transmute", "essence_flames_greater"], "LocalAddedFireDamageTwoHand7");
        assert!(used(&plan, "essence_flames_greater") > 0.0, "le plan doit utiliser la Greater Essence : {:?}", plan.shopping.iter().map(|l| &l.id).collect::<Vec<_>>());
    }

    /// Bout en bout : l'Essence of Haste s'applique désormais à une épée une main (avant : tags inexistants)
    /// et le plan l'achète pour atteindre « (17-19)% increased Attack Speed ».
    #[test]
    fn solver_buys_essence_of_haste_on_a_one_hand_sword() {
        let ds = Dataset::embedded();
        let plan = plan_with(&ds, "sword_1h", &["transmute", "essence_haste"], "LocalIncreasedAttackSpeed5");
        assert!(used(&plan, "essence_haste") > 0.0);
    }

    /// Bout en bout : Alchimie → Perfect Essence of Flames (objet Rare : retire un mod puis ajoute le
    /// garanti) sur un arc atteint « Gain (15-20)% of Damage as Extra Fire Damage », mod que rien d'autre
    /// ne donne ; le plan achète réellement l'Essence.
    #[test]
    fn solver_buys_a_perfect_essence_on_a_rare_item() {
        let ds = Dataset::embedded();
        let plan = plan_with(&ds, "bow", &["alchemy", "essence_flames_perfect"], "EssenceDamageasExtraFire1");
        assert!(used(&plan, "essence_flames_perfect") > 0.0, "le plan doit utiliser la Perfect Essence : {:?}", plan.shopping.iter().map(|l| &l.id).collect::<Vec<_>>());
        assert!(used(&plan, "alchemy") > 0.0, "la Perfect Essence exige un objet Rare d'abord");
    }
}
