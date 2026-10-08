use craft_api::craft_data::Dataset;
use craft_api::PlanContext;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tauri_plugin_global_shortcut::Shortcut;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub hotkey_toggle: String,
    pub hotkey_interactive: String,
    pub watch_clipboard: bool,
    pub check_updates_on_start: bool,
    pub auto_show_on_copy: bool,
    pub cpu_threads: usize,
    pub default_ilvl: u8,
    pub game_window_title: String,
    pub overlay_width: u32,
    pub overlay_height: u32,
    pub overlay_margin_x: i32,
    pub overlay_margin_y: i32,
    /// masquage automatique de l'overlay affiché par une copie d'objet (secondes ; 0 = jamais)
    pub overlay_auto_hide_secs: u32,
    /// ligue poe.ninja pour les prix ; vide = ligue temporaire courante (détectée automatiquement)
    pub price_league: String,
    /// actualisation des prix poe.ninja en arrière-plan (au démarrage puis à intervalle régulier)
    pub auto_refresh_prices: bool,
    /// intervalle de cette actualisation, en minutes (15 au minimum)
    pub price_refresh_minutes: u32,
    /// unité d'affichage des coûts : « ex » (Exalted) ou « div » (Divine, au prix courant de la Divine).
    /// Affichage seulement : le solveur et les prix restent en Exalted.
    pub cost_unit: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey_toggle: "Ctrl+D".into(),
            hotkey_interactive: "Ctrl+Shift+D".into(),
            watch_clipboard: true,
            check_updates_on_start: true,
            auto_show_on_copy: true,
            cpu_threads: 0,
            default_ilvl: 80,
            game_window_title: "Path of Exile 2".into(),
            overlay_width: 400,
            overlay_height: 640,
            overlay_margin_x: 24,
            overlay_margin_y: 96,
            overlay_auto_hide_secs: 10,
            price_league: String::new(),
            auto_refresh_prices: true,
            price_refresh_minutes: 60,
            cost_unit: "ex".into(),
        }
    }
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub ds: RwLock<Arc<Dataset>>,
    /// surcharges de prix saisies par l'utilisateur (fusionnées sur `dataset.prices`)
    pub price_overrides: Mutex<BTreeMap<String, f64>>,
    /// derniers prix du marché (poe.ninja), persistés dans `market_prices.json`
    pub market: Mutex<Option<crate::prices::MarketPrices>>,
    /// dernier échec d'actualisation des prix (date unix, message), effacé au premier succès
    pub price_error: Mutex<Option<(u64, String)>>,
    /// sérialise les actualisations réseau (bouton « Actualiser » et arrière-plan)
    pub price_fetch: Mutex<()>,
    pub settings: Mutex<Settings>,
    pub active: Mutex<Option<Arc<PlanContext>>>,
    /// contexte du dernier plan calculé (actif ou non) : base du comparateur de chemins
    pub last_plan: Mutex<Option<Arc<PlanContext>>>,
    /// dernier objet capturé compatible avec le plan actif (base, objet) : sert de point de départ quand
    /// le plan est recalculé (prix rafraîchis) plutôt que de reconsidérer une base neuve.
    pub last_item: Mutex<Option<(String, craft_api::ItemView)>>,
    /// suivi de craft en direct : objet saisi coup par coup dans l'overlay (base du plan actif)
    pub live: Mutex<Option<craft_api::LiveSession>>,
    /// historique des crafts terminés (coût prévu contre coût réel), persisté dans `craft-history.json`
    pub history: Mutex<craft_api::History>,
    pub cpu: Mutex<Arc<rayon::ThreadPool>>,
    pub cancel: Mutex<Arc<AtomicBool>>,
    pub overlay_wanted: AtomicBool,
    pub overlay_interactive: AtomicBool,
    /// l'overlay a été ouvert par une copie (et non par le raccourci) : il peut se masquer seul
    pub auto_shown: AtomicBool,
    pub last_capture: Mutex<std::time::Instant>,
    /// dernière erreur d'enregistrement des raccourcis globaux (raccourci invalide ou déjà pris)
    pub hotkey_error: Mutex<Option<String>>,
    pub hotkeys: Mutex<Option<(Shortcut, Shortcut)>>,
    pub last_clipboard: Mutex<String>,
}

fn read_json<T: for<'de> Deserialize<'de>>(p: &PathBuf) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

pub fn make_pool(threads: usize) -> Arc<rayon::ThreadPool> {
    let n = if threads > 0 { threads } else { std::thread::available_parallelism().map(|n| n.get().saturating_sub(2).max(1)).unwrap_or(2) };
    Arc::new(rayon::ThreadPoolBuilder::new().num_threads(n).thread_name(|i| format!("craft-cpu-{i}")).build().expect("pool rayon"))
}

impl AppState {
    pub fn load(data_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        let settings: Settings = read_json(&data_dir.join("settings.json")).unwrap_or_default();
        // le dataset embarqué (`data/sample/dataset.json`) est désormais la seule source : plus
        // d'import/réinitialisation depuis l'application (voir update-dataset.bat pour le rafraîchir).
        let ds = Dataset::embedded();
        let prices = read_json(&data_dir.join("prices.json")).unwrap_or_default();
        let market: Option<crate::prices::MarketPrices> = read_json(&data_dir.join("market_prices.json"));
        let history: craft_api::History = read_json(&data_dir.join("craft-history.json")).unwrap_or_default();
        Self {
            cpu: Mutex::new(make_pool(settings.cpu_threads)),
            data_dir,
            ds: RwLock::new(Arc::new(ds)),
            price_overrides: Mutex::new(prices),
            market: Mutex::new(market),
            price_error: Mutex::new(None),
            price_fetch: Mutex::new(()),
            settings: Mutex::new(settings),
            active: Mutex::new(None),
            last_plan: Mutex::new(None),
            last_item: Mutex::new(None),
            live: Mutex::new(None),
            history: Mutex::new(history),
            cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
            overlay_wanted: AtomicBool::new(false),
            overlay_interactive: AtomicBool::new(false),
            auto_shown: AtomicBool::new(false),
            last_capture: Mutex::new(std::time::Instant::now()),
            hotkey_error: Mutex::new(None),
            hotkeys: Mutex::new(None),
            last_clipboard: Mutex::new(String::new()),
        }
    }

    pub fn dataset(&self) -> Arc<Dataset> {
        self.ds.read().unwrap().clone()
    }

    pub fn prices(&self) -> BTreeMap<String, f64> {
        // priorité : saisie manuelle > marché (poe.ninja) > prix par défaut du dataset
        let mut p = self.dataset().prices.clone();
        if let Some(m) = self.market.lock().unwrap().as_ref() {
            p.extend(m.prices.clone());
        }
        p.extend(self.price_overrides.lock().unwrap().clone());
        p
    }

    pub fn save_settings(&self) {
        let s = self.settings.lock().unwrap().clone();
        let _ = std::fs::write(self.data_dir.join("settings.json"), serde_json::to_string_pretty(&s).unwrap());
    }
    pub fn save_prices(&self) {
        let p = self.price_overrides.lock().unwrap().clone();
        let _ = std::fs::write(self.data_dir.join("prices.json"), serde_json::to_string_pretty(&p).unwrap());
    }

    pub fn save_history(&self) {
        let h = self.history.lock().unwrap().clone();
        let _ = std::fs::write(self.data_dir.join("craft-history.json"), serde_json::to_string_pretty(&h).unwrap());
    }

    /// Nouveau jeton d'annulation pour un calcul long ; annule implicitement le précédent.
    pub fn new_job(&self) -> Arc<AtomicBool> {
        let mut g = self.cancel.lock().unwrap();
        g.store(true, Ordering::Relaxed);
        *g = Arc::new(AtomicBool::new(false));
        g.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn cost_unit_defaults_to_exalted_and_is_kept() {
        // réglages enregistrés par une version précédente, sans le champ : coûts en Exalted comme avant
        let old: Settings = serde_json::from_str(r#"{"priceLeague":"Standard","defaultIlvl":82}"#).unwrap();
        assert_eq!(old.cost_unit, "ex");
        assert_eq!(old.default_ilvl, 82);
        let mut s = Settings::default();
        s.cost_unit = "div".into();
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.cost_unit, "div");
        assert!(serde_json::to_string(&s).unwrap().contains(r#""costUnit":"div""#), "nom du champ attendu par l'interface");
    }
}
