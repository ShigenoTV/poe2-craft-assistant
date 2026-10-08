use crate::state::{make_pool, AppState, Settings};
use craft_api::craft_core::*;
use craft_api::craft_data::*;
use craft_api::craft_solver::{Advice, CraftPlan, GoalItem};
use craft_api::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

type St<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub fn dataset_info(st: St) -> DatasetInfo {
    craft_api::dataset_info(&st.dataset())
}

#[tauri::command]
pub fn base_pool(st: St, base_id: String) -> Result<PoolView, String> {
    pool_view(&st.dataset(), &base_id)
}

#[tauri::command]
pub fn list_actions_cmd(st: St) -> Result<Vec<ActionView>, String> {
    list_actions(&st.dataset(), &st.prices())
}

#[tauri::command]
pub fn get_prices(st: St) -> BTreeMap<String, f64> {
    st.prices()
}

#[tauri::command]
pub fn set_prices(st: St, overrides: BTreeMap<String, f64>) -> Result<(), String> {
    if overrides.values().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("les prix doivent être des nombres positifs".into());
    }
    *st.price_overrides.lock().unwrap() = overrides;
    st.save_prices();
    Ok(())
}

#[tauri::command]
pub fn sandbox_apply(st: St, base_id: String, item: ItemView, currency_id: String) -> Result<ApplyResult, String> {
    let ds = st.dataset();
    let bp = ds.build_pool(&base_id)?;
    let cur = find_currency(&ds, &st.prices(), &bp, &currency_id)?;
    sandbox_apply_impl(&bp, &item, &cur)
}

fn sandbox_apply_impl(bp: &BasePool, item: &ItemView, cur: &Currency) -> Result<ApplyResult, String> {
    craft_api::sandbox_apply(bp, item, cur, None)
}

#[tauri::command]
pub fn item_detail(st: St, base_id: String, item: ItemView) -> Result<ItemDetail, String> {
    let bp = st.dataset().build_pool(&base_id)?;
    let it = item.to_state(&bp.pool)?;
    Ok(craft_api::detail(&bp.pool, &it))
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    stage: String,
    done: u64,
    total: u64,
}

#[tauri::command]
pub async fn run_simulation(st: St<'_>, req: SimRequest, on_event: Channel<Progress>) -> Result<Option<SimResult>, String> {
    let st = st.inner().clone();
    let cancel = st.new_job();
    tauri::async_runtime::spawn_blocking(move || {
        let pool = st.cpu.lock().unwrap().clone();
        let (ds, prices) = (st.dataset(), st.prices());
        pool.install(|| {
            craft_api::run_simulation(&ds, &prices, &req, |done, total| {
                let _ = on_event.send(Progress { stage: "simulating".into(), done, total });
                !cancel.load(std::sync::atomic::Ordering::Relaxed)
            })
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Compare le dernier plan calculé à ses meilleures alternatives (sans l'une de ses familles de monnaies).
#[tauri::command]
pub async fn compare_paths(st: St<'_>, trials: u64, on_event: Channel<Progress>) -> Result<Vec<ComparedPath>, String> {
    let st = st.inner().clone();
    let ctx = st.last_plan.lock().unwrap().clone().ok_or("calcule d'abord un plan")?;
    let cancel = st.new_job();
    tauri::async_runtime::spawn_blocking(move || {
        let pool = st.cpu.lock().unwrap().clone();
        let _ = on_event.send(Progress { stage: "solving".into(), done: 0, total: 0 });
        pool.install(|| {
            craft_api::compare_paths(&ctx, 2, trials.max(1_000), &cancel, |done, total| {
                let _ = on_event.send(Progress { stage: "verifying".into(), done, total });
                !cancel.load(std::sync::atomic::Ordering::Relaxed)
            })
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveInfo {
    base_id: String,
    ilvl: u8,
    goal: Vec<GoalItem>,
    expected_cost: f64,
}

#[tauri::command]
pub async fn solve_plan(app: AppHandle, st: St<'_>, mut req: PlanRequest, activate: bool, on_event: Channel<Progress>) -> Result<CraftPlan, String> {
    let st = st.inner().clone();
    let cancel = st.new_job();
    tauri::async_runtime::spawn_blocking(move || {
        let pool = st.cpu.lock().unwrap().clone();
        let (ds, prices) = (st.dataset(), st.prices());
        req.prices_label = Some(price_label(&st));
        let _ = on_event.send(Progress { stage: "solving".into(), done: 0, total: 0 });
        let ctx = Arc::new(build_context(&ds, &req, &prices, &cancel)?);
        let _ = on_event.send(Progress { stage: "verifying".into(), done: 0, total: req.mc_trials });
        let plan = pool.install(|| {
            make_plan(&ctx, |done, total| {
                let _ = on_event.send(Progress { stage: "verifying".into(), done, total });
                !cancel.load(std::sync::atomic::Ordering::Relaxed)
            })
        })?;
        *st.last_plan.lock().unwrap() = Some(ctx.clone());
        if activate {
            *st.active.lock().unwrap() = Some(ctx);
            *st.live.lock().unwrap() = None; // nouveau plan : le suivi repart de son objet de départ
            let _ = tauri::Emitter::emit(&app, "plan-refreshed", ());
        }
        Ok(plan)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn cancel_job(st: St) {
    st.cancel.lock().unwrap().store(true, std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
pub fn active_plan(st: St) -> Option<ActiveInfo> {
    st.active.lock().unwrap().as_ref().map(|c| ActiveInfo {
        base_id: c.req.base_id.clone(),
        ilvl: c.req.ilvl,
        goal: c.goal_items.clone(),
        expected_cost: c.solution.value[c.solution.id(&craft_api::craft_solver::MacroState::empty(Rarity::Normal)).unwrap_or(0)],
    })
}

#[tauri::command]
pub fn clear_active_plan(app: AppHandle, st: St) {
    *st.active.lock().unwrap() = None;
    *st.live.lock().unwrap() = None;
    let _ = tauri::Emitter::emit(&app, "plan-refreshed", ());
}

/// Analyse un texte d'objet collé à la main ; même chemin que le presse-papiers (met à jour l'overlay).
#[tauri::command]
pub async fn submit_item_text(app: AppHandle, st: St<'_>, text: String) -> Result<crate::capture::ItemCaptured, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::capture::process_text(&app, &st, &text)).await.map_err(|e| e.to_string())
}

/// Analyse pure d'un texte d'objet, sans toucher à l'overlay ni au plan actif — utilisé pour repartir
/// d'un objet déjà existant dans le Reverse-crafting (au lieu d'une base neuve).
#[tauri::command]
pub fn analyze_item_text(st: St, text: String, base_hint: Option<String>, fallback_ilvl: u8) -> craft_api::ItemAnalysis {
    craft_api::analyze_item(&st.dataset(), &text, base_hint.as_deref(), fallback_ilvl)
}

#[tauri::command]
pub fn last_clipboard(st: St) -> String {
    st.last_clipboard.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_settings(st: St) -> Settings {
    st.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, st: St, settings: Settings) -> Result<(), String> {
    let old_threads = st.settings.lock().unwrap().cpu_threads;
    let old = st.settings.lock().unwrap().clone();
    *st.settings.lock().unwrap() = settings.clone();
    if let Err(e) = crate::hotkeys::apply(&app, &st) {
        *st.settings.lock().unwrap() = old; // on garde les anciens raccourcis fonctionnels
        let _ = crate::hotkeys::apply(&app, &st);
        return Err(e);
    }
    if settings.cpu_threads != old_threads {
        *st.cpu.lock().unwrap() = make_pool(settings.cpu_threads);
    }
    st.save_settings();
    crate::overlay::refresh(&app, &st);
    // l'overlay (autre fenêtre) suit le changement d'unité des coûts sans attendre
    let _ = tauri::Emitter::emit(&app, "settings-changed", &settings);
    Ok(())
}

#[tauri::command]
pub fn overlay_toggle(app: AppHandle, st: St) {
    crate::overlay::toggle(&app, &st)
}

#[tauri::command]
pub fn overlay_set_interactive(app: AppHandle, st: St, value: bool) {
    crate::overlay::set_interactive(&app, &st, value)
}

#[tauri::command]
pub fn overlay_state(st: St) -> (bool, bool) {
    use std::sync::atomic::Ordering::Relaxed;
    (st.overlay_wanted.load(Relaxed), st.overlay_interactive.load(Relaxed))
}

#[allow(dead_code)]
type _Unused = Advice;

// ───────────────────────── Suivi de craft en direct ─────────────────────────

/// Applique `f` à l'objet suivi du plan actif (session créée depuis l'objet de départ du plan si besoin),
/// puis recalcule le conseil. `None` sans plan actif.
pub(crate) fn live_run(
    app: &AppHandle,
    st: &AppState,
    f: impl FnOnce(&craft_api::PlanContext, &mut craft_api::LiveSession) -> Result<(), String>,
) -> Result<Option<craft_api::LiveView>, String> {
    use tauri::Emitter;
    let Some(ctx) = st.active.lock().unwrap().clone() else { return Ok(None) };
    let session = {
        let mut g = st.live.lock().unwrap();
        let mut s = match g.take() {
            Some(s) if s.base_id == ctx.req.base_id => s,
            _ => craft_api::live::live_start(&ctx),
        };
        let r = f(&ctx, &mut s);
        let snapshot = s.clone();
        *g = Some(s);
        r?;
        snapshot
    };
    *st.last_item.lock().unwrap() = Some((session.base_id.clone(), session.current().clone()));
    let view = craft_api::live::live_view(&ctx, &session, &std::sync::atomic::AtomicBool::new(false))?;
    let _ = app.emit("live-updated", &view);
    Ok(Some(view))
}

async fn live_blocking(
    app: AppHandle,
    st: Arc<AppState>,
    f: impl FnOnce(&craft_api::PlanContext, &mut craft_api::LiveSession) -> Result<(), String> + Send + 'static,
) -> Result<Option<craft_api::LiveView>, String> {
    tauri::async_runtime::spawn_blocking(move || live_run(&app, &st, f)).await.map_err(|e| e.to_string())?
}

/// Objet suivi et meilleur coup suivant (sans rien modifier).
#[tauri::command]
pub async fn live_state(app: AppHandle, st: St<'_>) -> Result<Option<craft_api::LiveView>, String> {
    live_blocking(app, st.inner().clone(), |_, _| Ok(())).await
}

/// Saisie de ce qui vient d'être obtenu (une seule étape d'historique pour toute la liste).
#[tauri::command]
pub async fn live_edit(app: AppHandle, st: St<'_>, edits: Vec<craft_api::LiveEdit>) -> Result<Option<craft_api::LiveView>, String> {
    // la saisie compte la monnaie conseillée juste avant (corrigeable ensuite avec `live_set_spend`)
    live_blocking(app, st.inner().clone(), move |ctx, s| {
        let spend = craft_api::live::advised_spend(ctx, s);
        s.apply_spending(&ctx.bp.pool, &edits, spend)
    })
    .await
}

/// Change la monnaie comptée pour la dernière saisie (`None` : correction de saisie, rien n'est compté).
#[tauri::command]
pub async fn live_set_spend(app: AppHandle, st: St<'_>, action_id: Option<String>) -> Result<Option<craft_api::LiveView>, String> {
    live_blocking(app, st.inner().clone(), move |ctx, s| {
        let spend = action_id.map(|id| craft_api::live::spend_of(ctx, &id)).transpose()?;
        s.set_last_spend(spend)
    })
    .await
}

/// Annule la dernière saisie (erreur de clic, mauvais tier...).
#[tauri::command]
pub async fn live_undo(app: AppHandle, st: St<'_>) -> Result<Option<craft_api::LiveView>, String> {
    live_blocking(app, st.inner().clone(), |_, s| {
        s.undo();
        Ok(())
    })
    .await
}

/// Repart d'une base neuve (objet abandonné, nouvelle base achetée) ; annulable.
#[tauri::command]
pub async fn live_reset(app: AppHandle, st: St<'_>) -> Result<Option<craft_api::LiveView>, String> {
    live_blocking(app, st.inner().clone(), |ctx, s| {
        s.set_spending(ItemView { rarity: Rarity::Normal, ilvl: ctx.req.ilvl, mods: vec![] }, Some(craft_api::live::new_base_spend(ctx)));
        Ok(())
    })
    .await
}

// ───────────────────────── Historique des crafts ─────────────────────────

/// Clôt le craft suivi (réussi si l'objectif est atteint, sinon abandonné), l'ajoute à l'historique et
/// repart d'une base neuve pour le craft suivant.
#[tauri::command]
pub async fn live_finish(app: AppHandle, st: St<'_>) -> Result<craft_api::CraftRecord, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        let ctx = st.active.lock().unwrap().clone().ok_or("aucun plan actif")?;
        let rec = {
            let live = st.live.lock().unwrap();
            let s = live.as_ref().filter(|s| s.base_id == ctx.req.base_id).ok_or("rien à enregistrer : aucune saisie depuis le début du suivi")?;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            craft_api::history::finish_record(&ctx, s, st.history.lock().unwrap().next_id(), now)?
        };
        st.history.lock().unwrap().add(rec.clone());
        st.save_history();
        let fresh = ItemView { rarity: Rarity::Normal, ilvl: ctx.req.ilvl, mods: vec![] };
        *st.live.lock().unwrap() = Some(craft_api::live::start_from(&ctx, fresh));
        let _ = app.emit("history-updated", ());
        live_run(&app, &st, |_, _| Ok(()))?;
        Ok(rec)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn history_list(st: St) -> Vec<craft_api::CraftRecord> {
    st.history.lock().unwrap().records.clone()
}

/// Supprime une fiche (`id`) ou tout l'historique (`None`).
#[tauri::command]
pub fn history_delete(app: AppHandle, st: St, id: Option<u64>) -> Result<(), String> {
    match id {
        Some(id) => {
            if !st.history.lock().unwrap().remove(id) {
                return Err("fiche introuvable".into());
            }
        }
        None => st.history.lock().unwrap().records.clear(),
    }
    st.save_history();
    let _ = tauri::Emitter::emit(&app, "history-updated", ());
    Ok(())
}

// ───────────────────────── Mises à jour ─────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    current: String,
    notes: Option<String>,
    date: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    /// "downloading" | "installing"
    stage: String,
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Vrai tant que `tools/setup-updates.mjs` n'a pas renseigné la clé publique et le dépôt GitHub.
fn updater_unconfigured(app: &AppHandle) -> bool {
    let cfg = app.config().plugins.0.get("updater").cloned().unwrap_or_default();
    let key_empty = cfg.get("pubkey").and_then(|v| v.as_str()).map_or(true, |k| k.trim().is_empty());
    let placeholder = cfg.get("endpoints").map_or(true, |e| e.to_string().contains("OWNER/REPO"));
    key_empty || placeholder
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    use tauri_plugin_updater::UpdaterExt;
    if updater_unconfigured(&app) {
        return Err("not_configured".into());
    }
    let updater = app.updater().map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;
    Ok(found.map(|u| UpdateInfo { version: u.version.clone(), current: u.current_version.clone(), notes: u.body.clone(), date: u.date.map(|d| d.to_string()) }))
}

/// Télécharge, vérifie la signature, installe puis relance. Sous Windows l'installeur NSIS ferme l'app lui-même.
#[tauri::command]
pub async fn install_update(app: AppHandle, on_event: Channel<UpdateProgress>) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    if updater_unconfigured(&app) {
        return Err("not_configured".into());
    }
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?.ok_or("aucune mise à jour disponible")?;
    let mut downloaded = 0u64;
    let (ch1, ch2) = (on_event.clone(), on_event);
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = ch1.send(UpdateProgress { stage: "downloading".into(), downloaded, total });
            },
            move || {
                let _ = ch2.send(UpdateProgress { stage: "installing".into(), downloaded: 0, total: None });
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}

/// Erreur d'enregistrement des raccourcis globaux, ou `None` s'ils fonctionnent.
#[tauri::command]
pub fn hotkey_status(st: St) -> Option<String> {
    st.hotkey_error.lock().unwrap().clone()
}

#[tauri::command]
pub fn price_state(st: St) -> crate::prices::PriceState {
    crate::prices::state_of(&st, None)
}

/// Actualise les prix depuis poe.ninja (espacé d'au moins 5 minutes), et relance le plan actif avec.
#[tauri::command]
pub async fn refresh_prices(app: AppHandle, st: St<'_>) -> Result<crate::prices::PriceState, String> {
    let st = st.inner().clone();
    let out = tauri::async_runtime::spawn_blocking({
        let st = st.clone();
        move || crate::prices::refresh(&st)
    })
    .await
    .map_err(|e| e.to_string())?;
    if out.is_ok() {
        crate::prices::refresh_active_plan(&app, &st);
    }
    out
}

/// Origine des prix affichée avec le plan : « poe.ninja, ligue X (il y a N min) » + nombre de prix saisis à la main.
fn price_label(st: &AppState) -> String {
    let mut label = match st.market.lock().unwrap().as_ref() {
        Some(m) => format!("poe.ninja, ligue {} (relevé il y a {} min)", m.league, crate::prices::now_unix().saturating_sub(m.fetched_at) / 60),
        None => "prix d'exemple du jeu de données (non actualisés)".to_string(),
    };
    let n = st.price_overrides.lock().unwrap().len();
    if n > 0 {
        label.push_str(&format!(" + {n} prix saisis à la main"));
    }
    label
}
