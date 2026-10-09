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

pub mod compare;
pub mod history;
pub mod live;
pub use compare::{compare_paths, ComparedPath};
pub use history::{CraftRecord, History};
pub use live::{LiveEdit, LiveSession, LiveView, Spend};

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
            for o in ds.omens.iter().filter(|o| o.combines_with(c)) {
                s.insert(format!("{}+{}", c.id, o.id));
            }
        }
        s
    };
    let defaults: HashSet<String> = ds.actions(prices, None)?.into_iter().map(|c| c.id).collect();
    Ok(ds
        .actions(prices, Some(&all))?
        .into_iter()
        .map(|c| {
            Ok(ActionView {
                default_enabled: defaults.contains(&c.id),
                id: c.id,
                label: c.label,
                kind: c.kind,
                min_mod_level: c.min_mod_level,
                add_slot: c.add_slot,
                remove_slot: c.remove_slot,
                unit_cost: c.unit_cost,
            })
        })
        .chain(ds.essences.iter().map(|e| {
            // Essences, Liquid Emotions et Alloys : leur mod garanti dépend de la base, mais l'interrupteur
            // est global (sans lui, `enabled_actions` envoyé par l'interface les désactivait toutes)
            Ok(ActionView {
                id: e.id.clone(),
                label: e.label.clone(),
                kind: CurrencyKind::Essence,
                min_mod_level: 0,
                add_slot: None,
                remove_slot: None,
                unit_cost: prices.get(&e.price_id).copied().ok_or_else(|| format!("prix manquant : {}", e.price_id))?,
                default_enabled: e.default_enabled,
            })
        }))
        .collect::<Result<Vec<_>, String>>()?)
}

/// Libellé lisible d'un `price_id` : monnaie, Essence, sinon nom poe.ninja (ex. « Potent Liquid Ferocity »).
pub fn price_label(ds: &Dataset, id: &str) -> String {
    if let Some(e) = ds.essences.iter().find(|e| e.price_id == id) {
        return e.label.clone();
    }
    if let Some(c) = ds.currencies.iter().find(|c| c.price_id == id) {
        return c.label.clone();
    }
    match ds.price_sources.get(id) {
        Some(s) => s.ninja_id.split('-').map(|w| w[..1].to_uppercase() + &w[1..]).collect::<Vec<_>>().join(" "),
        None => id.to_string(),
    }
}

/// Étape d'instillation d'une amulette, et ses lignes d'achat (une par émotion distincte).
fn instill_step(ds: &Dataset, skill: u32, prices: &BTreeMap<String, f64>) -> Result<(InstillStep, Vec<ShoppingLine>), String> {
    let (def, cost) = ds.instill_cost(skill, prices)?;
    let mut lines: Vec<ShoppingLine> = Vec::new();
    for e in &def.emotions {
        let unit = prices[e];
        match lines.iter_mut().find(|l| &l.id == e) {
            Some(l) => {
                l.expected_count += 1.0;
                l.expected_cost += unit;
            }
            None => lines.push(ShoppingLine { id: e.clone(), label: price_label(ds, e), expected_count: 1.0, unit_cost: unit, expected_cost: unit }),
        }
    }
    let step = InstillStep { skill, name: def.name.clone(), stats: def.stats.clone(), emotions: def.emotions.iter().map(|e| price_label(ds, e)).collect(), cost };
    Ok((step, lines))
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
    /// Passif à instiller sur l'amulette une fois l'objectif atteint (`InstillDef::skill`) ; amulettes seulement.
    #[serde(default)]
    pub instill: Option<u32>,
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
    /// instillation demandée (`req.instill`), résolue aux prix du plan
    pub instill: Option<(InstillStep, Vec<ShoppingLine>)>,
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
    let instill = match req.instill {
        Some(_) if bp.base.item_class != "Amulet" => return Err("l'instillation ne s'applique qu'aux amulettes".into()),
        Some(skill) => Some(instill_step(ds, skill, &prices)?),
        None => None,
    };
    let pool = Arc::new(bp.pool.clone());
    let enabled: Option<HashSet<String>> = req.enabled_actions.as_ref().map(|v| v.iter().cloned().collect());
    let mut actions: Vec<Action> = ds
        .actions(&prices, enabled.as_ref())?
        .into_iter()
        .filter(|c| ds.currency_applies(&c.id, &bp.base))
        .chain(ds.essence_currencies(&bp, &prices, enabled.as_ref())?)
        .map(|c| Action { id: c.id.clone(), label: c.label.clone(), cost: c.unit_cost, kind: ActionKind::Currency(c) })
        .collect();
    if actions.is_empty() {
        return Err("aucune action de craft activée".into());
    }
    // un mod décaleur (« +1 Suffix/Prefix Modifier allowed ») n'est suivi que comme mod inutile
    if wanted.iter().any(|w| pool.affixes.iter().any(|a| a.group == w.group && a.cap_shift != (0, 0))) {
        return Err("un mod « +1 Prefix/Suffix Modifier allowed » ne peut pas être un affixe voulu".into());
    }
    // places qu'un mod décaleur garanti activé peut ouvrir (Potent Liquid Contempt)
    let extra = actions
        .iter()
        .filter_map(|a| match &a.kind {
            ActionKind::Currency(c) if c.kind == CurrencyKind::Essence => Some([c.target, c.alt_target]),
            _ => None,
        })
        .flatten()
        .flatten()
        .map(|t| pool.affixes[t as usize].cap_shift)
        .fold((0u8, 0u8), |(a, b), (p, q)| (a.max(p.max(0) as u8), b.max(q.max(0) as u8)));
    let goal = Arc::new(Goal::with_extra(&pool, &wanted, extra)?);
    if req.allow_abandon {
        actions.push(Action { id: "__abandon".into(), label: "Abandonner l'objet".into(), cost: 0.0, kind: ActionKind::Abandon });
    }
    let base_cost = prices.get("base_white").copied().unwrap_or(1.0);
    let salvage = prices.get("base_salvage").copied().unwrap_or(0.0);
    let model = Arc::new(Model::new(pool.clone(), goal.clone(), req.ilvl, actions, base_cost, salvage));
    let start = match &req.starting_item {
        Some(view) => {
            let item = view.to_state(&pool).map_err(|e| format!("objet de départ invalide : {e}"))?;
            model.project(&item).ok_or_else(|| "objet de départ incompatible avec cet objectif (affixe fracturé non voulu, ou hors de ce que le solveur sait représenter)".to_string())?
        }
        None => MacroState::empty(Rarity::Normal),
    };
    let solution = solve_cached(SolveCache::global(), &model, &[start], &SolveConfig::default(), cancel)?;
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
        instill,
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
    if let Some((step, lines)) = &ctx.instill {
        plan.instill = Some(step.clone());
        plan.shopping.extend(lines.iter().cloned());
    }
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
    let Some(state) = ctx.model.project(item) else {
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
                let s = Arc::new(solve_cached(SolveCache::global(), &ctx.model, &[state], &SolveConfig::default(), cancel)?);
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
    /// plafond d'un objet Rare de cette base (3/3, 2/2 pour un joyau, décalé par certains implicites)
    pub max_prefixes: u8,
    pub max_suffixes: u8,
}

impl From<&BaseItem> for BaseView {
    fn from(b: &BaseItem) -> Self {
        let (max_prefixes, max_suffixes) = b.affix_pool(vec![]).cap(Rarity::Rare);
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
    /// recettes d'instillation d'amulette, émotions en libellés
    pub instills: Vec<InstillView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstillView {
    pub skill: u32,
    pub name: String,
    pub stats: Vec<String>,
    pub emotions: Vec<String>,
    /// `price_id` des émotions (même ordre), pour le coût aux prix courants
    pub emotion_ids: Vec<String>,
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
        instills: ds
            .instills
            .iter()
            .map(|i| InstillView { skill: i.skill, name: i.name.clone(), stats: i.stats.clone(), emotions: i.emotions.iter().map(|e| price_label(ds, e)).collect(), emotion_ids: i.emotions.clone() })
            .collect(),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolView {
    pub base: BaseView,
    /// groupes qu'on peut viser (sans les mods « +1 Prefix/Suffix Modifier allowed », suivis à part)
    pub groups: Vec<GroupInfo>,
    pub affixes: Vec<Affix>,
    /// places de préfixe / suffixe en plus du plafond qu'une Liquid Emotion peut ouvrir sur cette base
    /// (Potent Liquid Contempt), et le nom de ces émotions
    pub extra_prefixes: u8,
    pub extra_suffixes: u8,
    pub extra_via: Vec<String>,
}

pub fn pool_view(ds: &Dataset, base_id: &str) -> Result<PoolView, String> {
    let bp = ds.build_pool(base_id)?;
    let shift_of = |id: &str| bp.pool.affixes.iter().find(|a| a.id == id).map_or((0, 0), |a| a.cap_shift);
    let (mut extra_prefixes, mut extra_suffixes, mut extra_via) = (0u8, 0u8, Vec::new());
    for e in &ds.essences {
        let Some(t) = e.targets.iter().find(|t| t.matches(&bp.base.tags)) else { continue };
        let shifts = [shift_of(&t.mod_id), shift_of(&t.alt_mod_id)];
        if shifts.iter().any(|s| s.0 > 0 || s.1 > 0) {
            extra_prefixes = extra_prefixes.max(shifts.iter().map(|s| s.0.max(0) as u8).max().unwrap_or(0));
            extra_suffixes = extra_suffixes.max(shifts.iter().map(|s| s.1.max(0) as u8).max().unwrap_or(0));
            extra_via.push(e.label.clone());
        }
    }
    let groups = bp.groups.into_iter().filter(|g| g.tiers.iter().all(|t| bp.pool.affixes[t.affix_idx as usize].cap_shift == (0, 0))).collect();
    Ok(PoolView { base: BaseView::from(&bp.base), groups, affixes: bp.pool.affixes, extra_prefixes, extra_suffixes, extra_via })
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
                instill: None,
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
            wanted: vec![WantedReq { group: "MaximumResistances::+#% to all maximum Resistances (Amanamu)".into(), max_tier: 1 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
                starting_item: None,
                instill: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context");
        assert!(
            ctx.model.actions.iter().any(|a| matches!(&a.kind, ActionKind::Currency(c) if c.kind == CurrencyKind::Desecrate)),
            "la Désécration doit apparaître dans les actions du modèle"
        );
        // note : « MaximumResistances » est aussi le groupe d'un mod normal (MaximumElementalResistance) —
        // collision légitime du jeu, pas un bug : les deux s'excluent mutuellement en vrai. On vérifie
        // juste qu'au moins un tier du groupe est bien un mod `desecrated` (celui qu'on vise).
        let grp = ctx.bp.groups.iter().find(|g| g.key == "MaximumResistances::+#% to all maximum Resistances (Amanamu)").expect("le groupe cible doit exister dans le pool");
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
                instill: None,
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
                instill: None,
        };
        let ctx = build_context(&ds, &req, &prices, &AtomicBool::new(false)).expect("build_context sur un joyau");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged, "le solveur doit converger sur un joyau");
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0, "coût attendu fini et positif, obtenu {}", plan.expected_cost);
    }

    /// Un joyau Rare a 2 préfixes / 2 suffixes au plus, Time-Lost compris (Path of Building, Item.lua :
    /// `affixLimit` 4 pour un Rare de type Jewel). Bout en bout : l'interface affiche 2/2, un objectif à
    /// 3 préfixes est refusé, et sur un joyau Rare déjà plein (2 + 2) le solveur doit passer par une Annulation
    /// avant l'Exalt (avec le 3/3 d'avant, sa 1re action était l'Exalt, dans un 3e préfixe libre).
    #[test]
    fn solver_respects_the_two_two_cap_of_a_jewel() {
        let ds = Dataset::embedded();
        let jewels: Vec<_> = ds.bases.iter().filter(|b| b.item_class == "Jewel").collect();
        assert_eq!(jewels.len(), 8, "Ruby/Emerald/Sapphire/Diamond et leurs 4 versions Time-Lost");
        for b in &jewels {
            let view = BaseView::from(*b);
            assert_eq!((view.max_prefixes, view.max_suffixes), (2, 2), "{}", b.id);
            let pool = ds.build_pool(&b.id).unwrap().pool;
            assert_eq!(pool.cap(Rarity::Rare), (2, 2), "{}", b.id);
            assert_eq!(pool.cap(Rarity::Magic), (1, 1), "{}", b.id);
        }
        // les autres classes gardent 3/3
        assert_eq!(BaseView::from(ds.bases.iter().find(|b| b.id == "crossbow").unwrap()).max_prefixes, 3);

        let bp = ds.build_pool("jewel_strjewel").unwrap();
        let of = |slot: Slot| bp.groups.iter().filter(move |g| g.slot == slot);
        let (pre, suf): (Vec<_>, Vec<_>) = (of(Slot::Prefix).collect(), of(Slot::Suffix).collect());
        assert!(pre.len() >= 3 && suf.len() >= 2);
        let req = |wanted: Vec<WantedReq>, start: Option<ItemView>| PlanRequest {
            base_id: "jewel_strjewel".into(),
            ilvl: 82,
            wanted,
            enabled_actions: Some(vec!["alchemy".into(), "exalt".into(), "annul".into()]),
            prices: None,
            allow_abandon: false,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: start,
            instill: None,
        };
        let want = |g: &GroupInfo| WantedReq { group: g.key.clone(), max_tier: g.tiers.len() as u8 };

        let three = req(pre[..3].iter().map(|g| want(g)).collect(), None);
        let err = build_context(&ds, &three, &ds.prices, &AtomicBool::new(false)).err().expect("3 préfixes refusés sur un joyau");
        assert!(err.contains("2 préfixes"), "{err}");

        // joyau Rare plein : 2 préfixes + 2 suffixes non voulus, objectif = un 3e préfixe
        let lowest = |g: &GroupInfo| g.tiers.iter().max_by_key(|t| t.tier).unwrap().affix_idx;
        let mods = [pre[0], pre[1], suf[0], suf[1]].iter().map(|g| ModView { affix_idx: lowest(g), fractured: false }).collect();
        let full = ItemView { rarity: Rarity::Rare, ilvl: 82, mods };
        let ctx = build_context(&ds, &req(vec![want(pre[2])], Some(full)), &ds.prices, &AtomicBool::new(false)).expect("build_context");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged);
        let first = match &plan.nodes[&plan.root_id] {
            CraftNode::Action(n) => n.action.id.clone(),
            CraftNode::Terminal(_) => panic!("le joyau de départ n'atteint pas déjà l'objectif"),
        };
        assert_eq!(first, "annul", "joyau plein : l'Exalt n'a pas de place sans Annulation");
        assert!(plan.shopping.iter().any(|l| l.id == "exalt" && l.expected_count > 0.0));
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
                instill: None,
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
                instill: None,
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
            instill: None,
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
            instill: None,
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
            instill: None,
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
            instill: None,
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
            instill: None,
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
    pub(super) fn essence_target(ds: &Dataset, base: &str, essence: &str) -> Option<String> {
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

    pub(super) fn plan_with(ds: &Dataset, base: &str, enabled: &[&str], target_mod: &str) -> CraftPlan {
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
            instill: None,
        };
        let ctx = build_context(ds, &req, &ds.prices, &AtomicBool::new(false)).expect("build_context");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged);
        assert!(plan.expected_cost.is_finite() && plan.expected_cost > 0.0);
        plan
    }

    pub(super) fn used(plan: &CraftPlan, id: &str) -> f64 {
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

#[cfg(test)]
mod desecration_import_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Données RePoE 4.5.5.2 : les mods Désécrés des trois seigneurs sont importés (69 Amanamu, 64 Kurgal,
    /// 64 Ulaman) et chacun porte le tag de son seigneur, que filtrent les Omens Sovereign/Liege/Blackblooded.
    #[test]
    fn all_lord_desecrated_mods_are_imported() {
        let ds = Dataset::embedded();
        let des: Vec<&ModDef> = ds.mods.iter().filter(|m| m.desecrated).collect();
        let count = |t: &str| des.iter().filter(|m| m.tags.first().map(String::as_str) == Some(t)).count();
        assert_eq!((count("amanamu_mod"), count("kurgal_mod"), count("ulaman_mod")), (69, 64, 64));
        assert_eq!(des.len(), 197);
    }

    /// poe2db : Rib = « Desecrates a Rare Armour », Collarbone = « Amulet, Ring or Belt », Jawbone =
    /// « Weapon or Quiver ». Un os ne doit jamais apparaître sur une base qu'il ne peut pas désécrer.
    #[test]
    fn each_bone_only_applies_to_its_item_types() {
        let ds = Dataset::embedded();
        let ok = |bone: &str, base: &str| ds.currency_applies(bone, ds.base(base).unwrap());
        assert!(ok("desecrate_rib", "helmet_str") && !ok("desecrate_rib", "mace_1h") && !ok("desecrate_rib", "ring"));
        assert!(ok("desecrate_collarbone", "ring") && ok("desecrate_collarbone", "double_belt") && !ok("desecrate_collarbone", "helmet_str"));
        assert!(ok("desecrate_jawbone", "mace_1h") && ok("desecrate_jawbone", "wand") && ok("desecrate_jawbone_ancient+omen_sovereign", "blunt_quiver"));
        assert!(!ok("desecrate_jawbone", "body_armour_str"));
    }

    /// Bout en bout : Alchimie → Preserved Jawbone sur une masse une main atteint un mod Désécré
    /// d'Ulaman propre aux masses ; le plan achète la Jawbone, jamais la Rib (inapplicable à une arme).
    #[test]
    fn solver_desecrates_a_mace_with_a_jawbone() {
        let ds = Dataset::embedded();
        let bp = ds.build_pool("mace_1h").unwrap();
        let g = bp
            .groups
            .iter()
            .find(|g| g.tiers.iter().any(|t| bp.pool.affixes[t.affix_idx as usize].id == "AbyssMod1HMaceUlamanPrefixDamageWhileActiveTotem"))
            .expect("mod Désécré de masse dans le pool");
        let enabled: HashSet<String> = ["alchemy", "desecrate_jawbone", "desecrate_rib"].iter().map(|s| s.to_string()).collect();
        let req = PlanRequest {
            base_id: "mace_1h".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: g.key.clone(), max_tier: 1 }],
            enabled_actions: Some(enabled.into_iter().collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: None,
            instill: None,
        };
        let ctx = build_context(&ds, &req, &ds.prices, &AtomicBool::new(false)).expect("build_context");
        assert!(ctx.model.actions.iter().all(|a| a.id != "desecrate_rib"), "la Rib ne désécre que les armures");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged);
        assert!(plan.shopping.iter().any(|l| l.id == "desecrate_jawbone" && l.expected_count > 0.0));
    }
}

#[cfg(test)]
mod liquid_emotions_complete_tests {
    use super::greater_perfect_essence_tests::{essence_target, plan_with, used};
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Cibles du jeu (Path of Building, LiquidEmotions.lua ; recoupées sur poe2db) : joyau normal pour les
    /// émotions de base, Time-Lost pour les « Ancient », et le Diamond seulement là où le jeu le prévoit.
    #[test]
    fn liquid_emotions_resolve_the_game_mod_per_jewel() {
        let ds = Dataset::embedded();
        let t = |base: &str, e: &str| essence_target(&ds, base, e);
        let (ruby, sapphire, emerald, diamond) = ("jewel_strjewel", "jewel_intjewel", "jewel_dexjewel", "jewel_dexjewel_intjewel_strjewel");
        let (tl_ruby, tl_sapphire, tl_diamond) = ("jewel_str_radius_jewel", "jewel_int_radius_jewel", "jewel_dex_radius_jewel_int_radius_jewel_str_radius_jewel");
        assert_eq!(t(ruby, "liquid_despair").as_deref(), Some("JewelRageonHit"));
        assert_eq!(t(sapphire, "liquid_fear").as_deref(), Some("JewelSpellCriticalDamage"));
        assert_eq!(t(emerald, "liquid_suffering").as_deref(), Some("JewelMovementSpeed"));
        assert_eq!(t(emerald, "liquid_disgust").as_deref(), Some("JewelLifeonKill"), "Disgust sur Émeraude : Life on Kill (cible manquante avant)");
        assert_eq!(t(diamond, "liquid_isolation").as_deref(), Some("CraftedJewelMaximumChaosResistance"));
        assert_eq!(t(diamond, "liquid_ire"), None, "le Diamond ne reçoit pas Liquid Ire (il porte pourtant le tag strjewel)");
        assert_eq!(t(sapphire, "liquid_melancholy").as_deref(), Some("CraftedJewelExposureOnHitWhileRubyEmeraldSocketed"));
        assert_eq!(t(tl_ruby, "liquid_ire_ancient").as_deref(), Some("JewelRadiusArmour"));
        assert_eq!(t(tl_sapphire, "liquid_envy_ancient").as_deref(), Some("JewelRadiusSpellDamage"));
        assert_eq!(t(tl_diamond, "liquid_ferocity_ancient").as_deref(), Some("CraftedJewelRadiusChaosResistance"));
        assert_eq!(t(tl_diamond, "liquid_melancholy_ancient").as_deref(), Some("CraftedJewelRadiusExtraLargeSize"));
        assert_eq!(t(tl_diamond, "liquid_ire_ancient"), None);
        assert_eq!(t(ruby, "liquid_ire_ancient"), None, "une Ancient ne s'applique qu'à un joyau Time-Lost");
        assert_eq!(t(tl_ruby, "liquid_ire"), None, "une émotion de base ne s'applique pas à un Time-Lost");
        // préfixe OU suffixe (infobulle du jeu « Ruby Prefix: … / Ruby Suffix: … ») : les deux sont gardés
        let pair = |base: &str, e: &str| {
            let b = ds.bases.iter().find(|b| b.id == base).unwrap();
            let t = ds.essences.iter().find(|x| x.id == e).unwrap().targets.iter().find(|t| t.matches(&b.tags)).unwrap();
            (t.mod_id.clone(), t.alt_mod_id.clone())
        };
        assert_eq!(pair(ruby, "liquid_ferocity"), ("CraftedJewelSuffixEffect".into(), "CraftedJewelPrefixEffect".into()));
        assert_eq!(pair(diamond, "liquid_contempt"), ("CraftedJewelAdditionalSuffixAllowed".into(), "CraftedJewelAdditionalPrefixAllowed".into()));
        assert_eq!(pair(tl_sapphire, "liquid_contempt_ancient"), ("CraftedJewelAdditionalSuffixAllowed".into(), "CraftedJewelAdditionalPrefixAllowed".into()));
        let shift = |id: &str| ds.mods.iter().find(|m| m.id == id).map(|m| (m.prefix_cap_delta, m.suffix_cap_delta));
        assert_eq!(shift("CraftedJewelAdditionalSuffixAllowed"), Some((0, 1)), "préfixe « +1 Suffix Modifier allowed »");
        assert_eq!(shift("CraftedJewelAdditionalPrefixAllowed"), Some((1, 0)), "suffixe « +1 Prefix Modifier allowed »");
        let liquids: Vec<_> = ds.essences.iter().filter(|e| e.id.starts_with("liquid_")).collect();
        assert_eq!(liquids.len(), 26, "les 26 émotions du jeu");
        assert_eq!(liquids.iter().flat_map(|e| &e.targets).filter(|t| !t.mod_id.is_empty()).count(), 84);
        assert_eq!(liquids.iter().flat_map(|e| &e.targets).filter(|t| !t.alt_mod_id.is_empty()).count(), 12, "3 émotions × 4 joyaux");
        assert!(liquids.iter().all(|e| e.requires_rare && ds.price_sources.contains_key(&e.price_id)));
    }

    /// Cohérence : chaque cible d'Essence/Liquid Emotion/Alloy se résout sur au moins une base (avant,
    /// The Runefather's Alloy visait les tags « mace_1h »/« mace_2h », qui n'existent pas).
    #[test]
    fn every_essence_target_reaches_a_base() {
        let ds = Dataset::embedded();
        for e in &ds.essences {
            for t in e.targets.iter().filter(|t| !t.mod_id.is_empty()) {
                assert!(ds.bases.iter().any(|b| t.matches(&b.tags)), "{} : aucune base pour {:?}", e.id, t.item_tags);
            }
        }
        assert_eq!(essence_target(&ds, "mace_2h", "alloy_runefathers").as_deref(), Some("AlloyRunefathersMace"));
    }

    /// Bout en bout : « +1% to Maximum Chaos Resistance » n'existe sur un Diamond QUE par Concentrated
    /// Liquid Isolation (mod Crafted, poids nul) ; le plan l'achète réellement après une Alchimie.
    #[test]
    fn solver_buys_liquid_isolation_on_a_diamond() {
        let ds = Dataset::embedded();
        let plan = plan_with(&ds, "jewel_dexjewel_intjewel_strjewel", &["alchemy", "liquid_isolation"], "CraftedJewelMaximumChaosResistance");
        assert!(used(&plan, "liquid_isolation") > 0.0, "{:?}", plan.shopping.iter().map(|l| &l.id).collect::<Vec<_>>());
        assert!(used(&plan, "alchemy") > 0.0, "la Liquid Emotion exige un joyau Rare");
    }

    /// Bout en bout : une émotion « Ancient » sur un joyau Time-Lost (Ancient Potent Liquid Ferocity →
    /// résistance au froid, mod Crafted introuvable autrement).
    #[test]
    fn solver_buys_an_ancient_emotion_on_a_time_lost_jewel() {
        let ds = Dataset::embedded();
        let plan = plan_with(&ds, "jewel_int_radius_jewel", &["alchemy", "liquid_ferocity_ancient"], "CraftedJewelRadiusColdResistance");
        assert!(used(&plan, "liquid_ferocity_ancient") > 0.0, "{:?}", plan.shopping.iter().map(|l| &l.id).collect::<Vec<_>>());
    }

    /// Bout en bout : Potent Liquid Ferocity ajoute « increased Effect of Suffixes » (préfixe) OU « … of
    /// Prefixes » (suffixe) ; le plan l'achète pour viser le préfixe.
    #[test]
    fn solver_buys_potent_liquid_ferocity_for_its_prefix() {
        let ds = Dataset::embedded();
        let plan = plan_with(&ds, "jewel_strjewel", &["alchemy", "liquid_ferocity"], "CraftedJewelSuffixEffect");
        assert!(used(&plan, "liquid_ferocity") > 0.0, "{:?}", plan.shopping.iter().map(|l| &l.id).collect::<Vec<_>>());
    }

    /// Bout en bout : 3 suffixes voulus dépassent le plafond 2/2 d'un joyau. Refusé sans Potent Liquid
    /// Contempt ; avec elle, le solveur l'achète (préfixe « +1 Suffix Modifier allowed ») et le plan,
    /// rejoué sur le moteur exact (vrais tirages, vrai plafond objet par objet), atteint l'objectif au
    /// coût annoncé. Départ : un Rubis Rare portant déjà 2 des 3 suffixes et 1 préfixe inutile.
    #[test]
    fn potent_liquid_contempt_opens_a_third_suffix_on_a_jewel() {
        let ds = Dataset::embedded();
        let bp = ds.build_pool("jewel_strjewel").unwrap();
        // groupes tirables les plus fréquents de chaque slot, pour un plan rapide
        let top = |slot: Slot| {
            let mut g: Vec<&GroupInfo> = bp.groups.iter().filter(|g| g.slot == slot && g.total_weight > 0).collect();
            g.sort_by_key(|g| std::cmp::Reverse(g.total_weight));
            g
        };
        let (pre, suf) = (top(Slot::Prefix), top(Slot::Suffix));
        let wanted: Vec<WantedReq> = suf[..3].iter().map(|g| WantedReq { group: g.key.clone(), max_tier: g.tiers.len() as u8 }).collect();
        let lowest = |g: &GroupInfo| ModView { affix_idx: g.tiers.last().unwrap().affix_idx, fractured: false };
        let start = ItemView { rarity: Rarity::Rare, ilvl: 82, mods: vec![lowest(suf[0]), lowest(suf[1]), lowest(pre[0])] };
        let req = |actions: &[&str], mc_trials: u64| PlanRequest {
            base_id: "jewel_strjewel".into(),
            ilvl: 82,
            wanted: wanted.clone(),
            enabled_actions: Some(actions.iter().map(|s| s.to_string()).collect()),
            // prix bas : le plan reste court, ce qui permet de le rejouer sur le moteur exact
            prices: Some([("liquid_contempt".to_string(), 2.0), ("annul".to_string(), 1.0)].into()),
            allow_abandon: false,
            mc_trials,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: Some(start.clone()),
            instill: None,
        };
        let base = ["alchemy", "exalt", "annul"];
        let err = build_context(&ds, &req(&base, 0), &ds.prices, &AtomicBool::new(false)).err().expect("3 suffixes refusés sans Contempt");
        assert!(err.contains("2 suffixes"), "{err}");

        let ctx = build_context(&ds, &req(&["alchemy", "exalt", "annul", "liquid_contempt"], 4000), &ds.prices, &AtomicBool::new(false)).expect("build_context avec Contempt");
        let plan = make_plan(&ctx, |_, _| true).expect("make_plan");
        assert!(plan.solver.converged && plan.expected_cost.is_finite());
        assert!(used(&plan, "liquid_contempt") > 0.0, "{:?}", plan.shopping.iter().map(|l| &l.id).collect::<Vec<_>>());
        let mc = plan.mc.as_ref().expect("vérification Monte Carlo");
        assert!(mc.censored * 100 < mc.trials, "le moteur exact doit atteindre l'objectif : {mc:?}");
        let gap = (mc.mean_cost - plan.expected_cost).abs() / plan.expected_cost;
        assert!(gap < 0.1, "coût moteur exact {:.1} vs solveur {:.1} ({mc:?})", mc.mean_cost, plan.expected_cost);
    }

    /// Interface : sur un joyau, le choix d'objectif annonce 2/2 plus une place qu'ouvre Potent Liquid
    /// Contempt, et ne propose pas le mod « +1 … Modifier allowed » comme affixe voulu.
    #[test]
    fn pool_view_announces_the_place_opened_by_contempt() {
        let ds = Dataset::embedded();
        let v = pool_view(&ds, "jewel_strjewel").unwrap();
        assert_eq!((v.base.max_prefixes, v.base.max_suffixes, v.extra_prefixes, v.extra_suffixes), (2, 2, 1, 1));
        assert_eq!(v.extra_via, ["Potent Liquid Contempt"]);
        assert!(v.groups.iter().all(|g| !g.family.contains("Modifier allowed")));
        let tl = pool_view(&ds, "jewel_int_radius_jewel").unwrap();
        assert_eq!(tl.extra_via, ["Ancient Potent Liquid Contempt"]);
        let ring = pool_view(&ds, "ring").unwrap();
        assert_eq!((ring.extra_prefixes, ring.extra_suffixes), (0, 0));
    }

    /// L'interface envoie la liste des actions cochées : les Essences/Liquid Emotions doivent y figurer,
    /// sinon le planificateur de l'application ne les utilise jamais.
    #[test]
    fn listed_actions_include_essences_and_liquid_emotions() {
        let ds = Dataset::embedded();
        let acts = list_actions(&ds, &ds.prices).unwrap();
        let ire = acts.iter().find(|a| a.id == "liquid_ire").expect("liquid_ire listée");
        assert!(ire.default_enabled && ire.kind == CurrencyKind::Essence);
        assert!(acts.iter().any(|a| a.id == "essence_flames_perfect"));
        assert_eq!(acts.iter().filter(|a| a.kind == CurrencyKind::Essence).count(), ds.essences.len());
    }

    /// Recettes d'instillation : 875 passifs, trois émotions dans l'ordre (Fast Acting Toxins = Paranoia,
    /// Greed, Isolation, vérifié sur poe2db).
    #[test]
    fn instill_recipes_are_imported_in_game_order() {
        let ds = Dataset::embedded();
        assert_eq!(ds.instills.len(), 875);
        let fat = ds.instills.iter().find(|i| i.name == "Fast Acting Toxins").unwrap();
        assert_eq!(fat.emotions, ["liquid_paranoia", "liquid_greed", "liquid_isolation"]);
        let splinters = ds.instills.iter().find(|i| i.name == "Splinters").unwrap();
        assert_eq!(splinters.emotions, ["liquid_envy", "liquid_paranoia", "liquid_despair"]);
        let info = dataset_info(&ds);
        let v = info.instills.iter().find(|i| i.skill == fat.skill).unwrap();
        assert_eq!(v.emotions, ["Liquid Paranoia", "Diluted Liquid Greed", "Concentrated Liquid Isolation"]);
        assert!(info.instills.iter().flat_map(|i| &i.emotions).any(|l| l == "Potent Liquid Ferocity"), "libellé tiré de poe.ninja pour une émotion sans entrée Essence");
    }

    /// Bout en bout : objectif sur une amulette + instillation. Le plan ajoute l'étape finale et ses trois
    /// émotions à la liste de courses, sans toucher au coût du craft lui-même ; refusé hors amulette.
    #[test]
    fn plan_adds_the_instill_step_on_an_amulet() {
        let ds = Dataset::embedded();
        let bp = ds.build_pool("gold_amulet").unwrap();
        let g = bp.groups.iter().find(|g| g.key == "IncreasedLife").expect("vie sur amulette");
        let skill = ds.instills.iter().find(|i| i.name == "Fast Acting Toxins").unwrap().skill;
        let mut req = PlanRequest {
            base_id: "gold_amulet".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: g.key.clone(), max_tier: g.tiers.len() as u8 }],
            enabled_actions: Some(vec!["transmute".into(), "augment".into()]),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: None,
            instill: None,
        };
        let plain = make_plan(&build_context(&ds, &req, &ds.prices, &AtomicBool::new(false)).unwrap(), |_, _| true).unwrap();
        req.instill = Some(skill);
        let plan = make_plan(&build_context(&ds, &req, &ds.prices, &AtomicBool::new(false)).unwrap(), |_, _| true).unwrap();
        let step = plan.instill.as_ref().expect("étape d'instillation");
        assert_eq!(step.name, "Fast Acting Toxins");
        let expected = ds.prices["liquid_paranoia"] + ds.prices["liquid_greed"] + ds.prices["liquid_isolation"];
        assert!((step.cost - expected).abs() < 1e-9);
        for id in ["liquid_paranoia", "liquid_greed", "liquid_isolation"] {
            assert_eq!(used(&plan, id), 1.0, "{id} achetée une fois");
        }
        assert!((plan.expected_cost - plain.expected_cost).abs() < 1e-9, "l'instillation ne change pas le craft");
        req.base_id = "ring".into();
        let err = build_context(&ds, &req, &ds.prices, &AtomicBool::new(false)).err().expect("refusé sur un anneau");
        assert!(err.contains("amulettes"), "{err}");
    }

    /// Budget, bout en bout sur le vrai dataset : le plan vérifié sur le moteur exact renvoie la loi du
    /// coût (quantiles) que l'interface lit pour « j'ai X Exalted, quelle chance de réussir ? ».
    #[test]
    fn plan_reports_cost_distribution_for_budget_questions() {
        let ds = Dataset::embedded();
        let bp = ds.build_pool("gold_amulet").unwrap();
        let g = bp.groups.iter().find(|g| g.key == "IncreasedLife").expect("vie sur amulette");
        let req = PlanRequest {
            base_id: "gold_amulet".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: g.key.clone(), max_tier: 3 }],
            enabled_actions: Some(vec!["transmute".into(), "augment".into()]),
            prices: None,
            allow_abandon: true,
            mc_trials: 20_000,
            node_cap: 50,
            seed: 3,
            prices_label: None,
            starting_item: None,
            instill: None,
        };
        let plan = make_plan(&build_context(&ds, &req, &ds.prices, &AtomicBool::new(false)).unwrap(), |_, _| true).unwrap();
        let mc = plan.mc.as_ref().expect("vérification Monte Carlo");
        assert_eq!(mc.cost_quantiles.len(), craft_solver::QUANTILE_STEPS + 1);
        assert_eq!(mc.cost_quantiles[craft_solver::QUANTILE_STEPS / 10 * 9], mc.p90_cost, "le P90 est le quantile à 90 %");
        let p90 = mc.success_probability(mc.p90_cost);
        assert!((0.895..0.95).contains(&p90), "P(coût ≤ P90) = {p90}");
        let at_mean = mc.success_probability(plan.expected_cost);
        assert!(at_mean > 0.3 && at_mean < p90, "P(coût ≤ moyenne) = {at_mean}");
        assert!(mc.success_probability(mc.cost_quantiles[0] * 0.5) < 0.01);
        assert!(mc.p90_cost > plan.expected_cost, "la loi du coût a une queue : P90 au-dessus de la moyenne");
        let json = serde_json::to_value(mc).unwrap();
        assert_eq!(json["costQuantiles"].as_array().unwrap().len(), craft_solver::QUANTILE_STEPS + 1, "nom attendu par l'interface");
    }

    /// Bout en bout, Désécration sur des gants (mods Désécrés en suffixe seulement) : sur un objet plein,
    /// le mod retiré au hasard peut être un préfixe, et aucun mod Désécré ne peut alors prendre sa place.
    /// Ce cas ne doit ni disparaître des transitions du solveur (il passait pour un succès gratuit : plan
    /// annoncé ~13 ex, ~12 000 ex sur le moteur exact), ni amputer l'objet dans le moteur exact.
    /// Même test pour les Alloys, activées par défaut (voir plus bas).
    #[test]
    fn desecration_on_full_gloves_keeps_solver_and_exact_engine_aligned() {
        let ds = Dataset::embedded();
        let req = PlanRequest {
            base_id: "gloves_dex".into(),
            ilvl: 81,
            wanted: ["IncreasedLife", "FireResistance"].iter().map(|g| WantedReq { group: (*g).into(), max_tier: 3 }).collect(),
            enabled_actions: None,
            prices: None,
            allow_abandon: true,
            mc_trials: 6_000,
            node_cap: 50,
            seed: 5,
            prices_label: None,
            starting_item: None,
            instill: None,
        };
        let ctx = build_context(&ds, &req, &ds.prices, &AtomicBool::new(false)).unwrap();
        let (m, sol) = (&ctx.model, &ctx.solution);
        assert!(m.actions.iter().any(|a| a.id == "desecrate_rib"), "Preserved Rib proposée sur des gants");
        let mut out = Vec::new();
        for (i, s) in sol.states.iter().enumerate() {
            for a in 0..m.actions.len() {
                out.clear();
                m.outcomes(*s, a, &mut out);
                let sum: f64 = out.iter().map(|t| t.p).sum();
                assert!(out.is_empty() || (sum - 1.0).abs() < 1e-9, "état {i} {s:?}, {} : probabilités sommant à {sum}", m.actions[a].id);
            }
        }
        // Alloys (actions par défaut) : le mod garanti posé est suivi, sinon le solveur croit pouvoir le
        // reposer et la politique tourne en rond sur le moteur exact (20 % d'essais interrompus avant)
        assert!(!m.tracked.is_empty(), "mods garantis d'Alloy suivis");
        let plan = make_plan(&ctx, |_, _| true).unwrap();
        let mc = plan.mc.as_ref().expect("vérification Monte Carlo");
        assert!(mc.censored * 100 < mc.trials, "{mc:?}");
        let gap = (mc.mean_cost - plan.expected_cost).abs() / plan.expected_cost;
        assert!(gap < 0.1, "coût moteur exact {:.1} vs solveur {:.1}", mc.mean_cost, plan.expected_cost);
    }
}

/// Vitesse du solveur (policy iteration, cache par structure de modèle) sans changement de résultat : sur
/// de vrais objectifs que l'ancienne value iteration résolvait, mêmes valeurs, mêmes actions, même plan.
#[cfg(test)]
mod solver_speed_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn req(base: &str, ilvl: u8, wanted: &[(&str, u8)], enabled: Option<&[&str]>) -> PlanRequest {
        PlanRequest {
            base_id: base.into(),
            ilvl,
            wanted: wanted.iter().map(|(g, t)| WantedReq { group: (*g).into(), max_tier: *t }).collect(),
            enabled_actions: enabled.map(|e| e.iter().map(|s| s.to_string()).collect()),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: None,
            instill: None,
        }
    }

    fn q(m: &Model, sol: &Solution, s: usize, a: usize) -> f64 {
        let es = sol.edges_of(s, a);
        if es.is_empty() {
            return f64::INFINITY;
        }
        let (mut p_self, mut acc) = (0.0, 0.0);
        for e in es {
            acc += e.p * e.extra;
            if e.to as usize == s {
                p_self += e.p;
            } else {
                acc += e.p * sol.value[e.to as usize];
            }
        }
        if p_self > 1.0 - 1e-12 {
            return f64::INFINITY;
        }
        (m.actions[a].cost + acc) / (1.0 - p_self)
    }

    /// Plan complet (tous les états visités), pour comparer deux solutions nœud par nœud.
    fn full_plan(ctx: &PlanContext, sol: &Solution) -> CraftPlan {
        let inputs = PlanInputs { model: &ctx.model, sol, start: ctx.start, base_id: "", base_cost: 0.0, salvage: 0.0, goal_items: ctx.goal_items.clone(), prices_source: String::new() };
        build_plan(&inputs, &PlanConfig { node_cap: 1_000_000, ..Default::default() }).unwrap()
    }

    fn rel(a: f64, b: f64) -> f64 {
        if a == b {
            0.0
        } else {
            (a - b).abs() / a.abs().max(b.abs()).max(1.0)
        }
    }

    /// Compare la solution de référence (value iteration seule, l'ancienne méthode) à la nouvelle.
    fn assert_same(ctx: &PlanContext, old: &Solution, new: &Solution) {
        let m = &ctx.model;
        assert!(old.converged && new.converged);
        assert_eq!(old.states, new.states, "même graphe");
        let mut ties = 0;
        for s in 0..old.states.len() {
            assert_eq!(old.value[s].is_finite(), new.value[s].is_finite(), "état {s}");
            if old.value[s].is_finite() {
                assert!(rel(old.value[s], new.value[s]) < 1e-6, "état {s} : V {} (ancien) vs {} (nouveau)", old.value[s], new.value[s]);
            }
            if old.policy[s] != new.policy[s] {
                // seule différence admise : deux actions à égalité (écart sous la précision de l'ancienne méthode)
                let (qo, qn) = (q(m, new, s, old.policy[s] as usize), q(m, new, s, new.policy[s] as usize));
                assert!(rel(qo, qn) < 1e-6, "état {s} : {} (ancien) vs {} (nouveau), q {qo} vs {qn}", m.actions[old.policy[s] as usize].id, m.actions[new.policy[s] as usize].id);
                ties += 1;
            }
        }
        let (po, pn) = (full_plan(ctx, old), full_plan(ctx, new));
        assert!(rel(po.expected_cost, pn.expected_cost) < 1e-6, "coût {} vs {}", po.expected_cost, pn.expected_cost);
        if ties == 0 {
            assert_eq!(po.nodes.keys().collect::<Vec<_>>(), pn.nodes.keys().collect::<Vec<_>>(), "mêmes nœuds de plan");
            for (id, a) in &po.nodes {
                if let (CraftNode::Action(a), CraftNode::Action(b)) = (a, &pn.nodes[id]) {
                    assert_eq!(a.action.id, b.action.id, "nœud {id}");
                    assert_eq!(a.branches.iter().map(|x| &x.to).collect::<Vec<_>>(), b.branches.iter().map(|x| &x.to).collect::<Vec<_>>(), "nœud {id}");
                }
            }
            assert_eq!(po.shopping.iter().map(|l| &l.id).collect::<Vec<_>>(), pn.shopping.iter().map(|l| &l.id).collect::<Vec<_>>());
            for (a, b) in po.shopping.iter().zip(&pn.shopping) {
                assert!(rel(a.expected_count, b.expected_count) < 1e-6, "{} : {} vs {}", a.id, a.expected_count, b.expected_count);
            }
        }
    }

    #[test]
    fn policy_iteration_gives_the_same_plans_as_value_iteration() {
        let ds = Dataset::embedded();
        let no = AtomicBool::new(false);
        let cases = [
            req("gold_amulet", 82, &[("IncreasedLife", 3)], Some(&["transmute", "augment"])),
            req("gold_amulet", 82, &[("IncreasedLife", 2)], None),
            req("gloves_dex", 81, &[("IncreasedLife", 3), ("FireResistance", 3)], None),
            req("gloves_dex", 81, &[("IncreasedLife", 3), ("FireResistance", 3), ("ColdResistance", 3)], None),
            req("jewel_strjewel", 82, &[("IncreasedPhysicalDamageReductionRatingPercent", 1)], Some(&["alchemy", "liquid_ire"])),
            req("crossbow", 82, &[("FireDamage", 3)], None),
        ];
        for r in &cases {
            let ctx = build_context(&ds, r, &ds.prices, &no).unwrap_or_else(|e| panic!("{} : {e}", r.base_id));
            let old = solve(&ctx.model, &[ctx.start], &SolveConfig { policy_iteration: false, ..Default::default() }, &no).unwrap();
            let new = solve(&ctx.model, &[ctx.start], &SolveConfig::default(), &no).unwrap();
            assert!(new.pi_iters > 0, "{} : policy iteration utilisée", r.base_id);
            assert_same(&ctx, &old, &new);
            // et le contexte de l'application (cache) donne la même chose
            assert_same(&ctx, &old, &ctx.solution);
        }
    }

    /// Le cas qui ne convergeait pas en 45 s (gants Vie/Feu/Froid/Précision T3, actions par défaut) :
    /// résolu, et le coût annoncé est confirmé par le moteur exact.
    #[test]
    fn four_t3_mods_on_gloves_converge_and_match_the_exact_engine() {
        let ds = Dataset::embedded();
        let mut r = req("gloves_dex", 81, &[("IncreasedLife", 3), ("FireResistance", 3), ("ColdResistance", 3), ("IncreasedAccuracy", 3)], None);
        r.mc_trials = 3_000;
        r.seed = 7;
        let t0 = std::time::Instant::now();
        let ctx = build_context(&ds, &r, &ds.prices, &AtomicBool::new(false)).unwrap();
        assert!(ctx.solution.converged, "convergé");
        assert!(t0.elapsed().as_secs() < 20, "résolu en {:?}", t0.elapsed());
        let plan = make_plan(&ctx, |_, _| true).unwrap();
        let mc = plan.mc.as_ref().unwrap();
        assert!(rel(plan.solver.cost_from_visits, plan.expected_cost) < 1e-6, "contrôle par visites {} vs {}", plan.solver.cost_from_visits, plan.expected_cost);
        assert!(mc.censored * 100 < mc.trials, "{mc:?}");
        let gap = (mc.mean_cost - plan.expected_cost).abs() / plan.expected_cost;
        assert!(gap < 0.08, "moteur exact {:.1} vs solveur {:.1}", mc.mean_cost, plan.expected_cost);
        // Omen of Whittling (le Chaos retire le mod du niveau le plus bas) : utilisé, et le moteur exact
        // confirme le gain (sans lui : ~1 453 ex au moteur exact, 1 439 annoncés)
        let whittling = plan.shopping.iter().find(|l| l.id == "chaos+omen_whittling").map_or(0.0, |l| l.expected_count);
        assert!(whittling > 1.0, "Whittling utilisé : {whittling}");
        assert!(mc.ci95_mean.1 < 1_400.0, "gain confirmé par le moteur exact : {:?}", mc.ci95_mean);
    }

    /// Omen of Light (l'Annulation ne retire que le mod Désécré) : le solveur s'en sert pour retenter une
    /// Désécration ratée au lieu de jeter l'objet, et le moteur exact confirme le gain. Annulation à 0,5 ex
    /// pour que ce soit rentable (au prix du dataset, 304 ex, jeter l'objet reste moins cher).
    #[test]
    fn omen_of_light_lets_the_solver_retry_a_desecration() {
        let ds = Dataset::embedded();
        let g = "MaximumResistances::+#% to all maximum Resistances (Amanamu)";
        let run = |acts: &[&str]| {
            let mut r = req("shield_str", 82, &[(g, 1)], Some(acts));
            r.mc_trials = 6_000;
            r.seed = 3;
            r.prices = Some([("annul".to_string(), 0.5)].into_iter().collect());
            let ctx = build_context(&ds, &r, &ds.prices, &AtomicBool::new(false)).unwrap();
            make_plan(&ctx, |_, _| true).unwrap()
        };
        let without = run(&["alchemy", "desecrate_rib"]);
        let with = run(&["alchemy", "desecrate_rib", "annul+omen_light"]);
        let light = with.shopping.iter().find(|l| l.id == "annul+omen_light").map_or(0.0, |l| l.expected_count);
        assert!(light > 5.0, "Omen of Light utilisé : {light}");
        let (mw, mo) = (with.mc.as_ref().unwrap(), without.mc.as_ref().unwrap());
        assert!(mw.ci95_mean.1 * 3.0 < mo.ci95_mean.0, "moteur exact : {:.1} avec Light contre {:.1} sans", mw.mean_cost, mo.mean_cost);
        // écart solveur / moteur exact du même ordre que sans Light (~10 % sur ce cas de Désécration)
        assert!((mw.mean_cost - with.expected_cost).abs() / with.expected_cost < 0.15, "moteur exact {:.1} vs solveur {:.1}", mw.mean_cost, with.expected_cost);
    }

    /// Cache : un changement de prix seul reprend le graphe et l'ancienne politique, et donne exactement
    /// le résultat d'un calcul complet aux nouveaux prix.
    #[test]
    fn a_price_change_reuses_the_cached_graph_with_the_same_result() {
        let ds = Dataset::embedded();
        let no = AtomicBool::new(false);
        let mut r = req("gloves_dex", 81, &[("IncreasedLife", 3), ("FireResistance", 3), ("ColdResistance", 3)], None);
        let first = build_context(&ds, &r, &ds.prices, &no).unwrap();
        let mut prices = ds.prices.clone();
        let used = full_plan(&first, &first.solution).shopping.iter().find(|l| prices.contains_key(&l.id) && l.expected_count > 0.5).unwrap().id.clone();
        *prices.get_mut(&used).unwrap() *= 3.0;
        r.prices = Some([("base_white".to_string(), 2.5)].into_iter().collect());
        let cache = SolveCache::new(4);
        let cfg = SolveConfig::default();
        // remplit un cache privé aux anciens prix, puis recalcule aux nouveaux
        solve_cached(&cache, &first.model, &[first.start], &cfg, &no).unwrap();
        let second = build_context(&ds, &r, &prices, &no).unwrap();
        assert_eq!(first.model.structure_key(&[first.start]), second.model.structure_key(&[second.start]), "même structure");
        let warm = solve_cached(&cache, &second.model, &[second.start], &cfg, &no).unwrap();
        let cold = solve(&second.model, &[second.start], &cfg, &no).unwrap();
        assert_eq!(cache.len(), 1);
        assert!(warm.pi_iters < cold.pi_iters, "reprise de l'ancienne politique : {} évaluations contre {}", warm.pi_iters, cold.pi_iters);
        assert_same(&second, &cold, &warm);
        assert!(rel(warm.value[warm.id(&second.start).unwrap()], first.solution.value[first.solution.id(&first.start).unwrap()]) > 1e-3, "les prix ont bien changé le coût");
        // un autre objectif ne réutilise pas ce graphe
        let other = build_context(&ds, &req("gloves_dex", 81, &[("IncreasedLife", 3)], None), &ds.prices, &no).unwrap();
        assert_ne!(other.model.structure_key(&[other.start]), second.model.structure_key(&[second.start]));
    }
}
