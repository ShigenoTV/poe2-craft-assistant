//! Prix du marché depuis poe.ninja (API économique publique de PoE2, sans clé).
//!
//! Respect des règles de l'API : identifiant d'application dans le User-Agent, 2 à 3 requêtes par actualisation,
//! jamais plus d'une actualisation réseau toutes les 5 minutes (poe.ninja ne met à jour PoE2 qu'environ toutes les heures).

use crate::state::AppState;
use craft_api::craft_data::{extract, first_league, Dataset};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Adresse de base, surchargeable pour les tests (`POE2_NINJA_BASE`).
fn base_url() -> String {
    std::env::var("POE2_NINJA_BASE").unwrap_or_else(|_| "https://poe.ninja/poe2/api/economy".to_string())
}

pub const MIN_REFETCH_SECS: u64 = 300;
/// Intervalle minimal accepté pour l'actualisation en arrière-plan (poe.ninja ne bouge pas plus vite).
pub const MIN_INTERVAL_MINUTES: u32 = 15;
/// Après un échec réseau, nouvel essai au plus tôt 10 minutes plus tard (les anciens prix restent en place).
pub const RETRY_AFTER_ERROR_SECS: u64 = 600;

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPrices {
    pub league: String,
    pub fetched_at: u64,
    /// price_id -> prix en Exalted
    pub prices: BTreeMap<String, f64>,
    /// price_id sans prix exploitable dans la réponse (objet absent ou jamais échangé)
    pub missing: Vec<String>,
    /// price_id -> date (unix) du relevé de ce prix. Un prix absent d'une actualisation garde sa valeur et
    /// sa date précédentes ; un ancien fichier sans ce champ prend `fetched_at` pour tous ses prix.
    #[serde(default)]
    pub updated_at: BTreeMap<String, u64>,
}

impl MarketPrices {
    pub fn updated_at_of(&self, id: &str) -> Option<u64> {
        self.prices.contains_key(id).then(|| self.updated_at.get(id).copied().unwrap_or(self.fetched_at))
    }
}

/// Fusionne un nouveau relevé avec le précédent : sur la même ligue, un prix que poe.ninja ne renvoie plus
/// (objet absent cette heure-ci) garde sa dernière valeur connue et sa date ; changer de ligue repart de zéro.
pub fn merge(prev: Option<&MarketPrices>, mut new: MarketPrices) -> MarketPrices {
    new.updated_at = new.prices.keys().map(|k| (k.clone(), new.fetched_at)).collect();
    if let Some(p) = prev.filter(|p| p.league == new.league) {
        for (k, v) in &p.prices {
            if !new.prices.contains_key(k) {
                new.prices.insert(k.clone(), *v);
                new.updated_at.insert(k.clone(), p.updated_at_of(k).unwrap_or(p.fetched_at));
            }
        }
        new.missing.retain(|k| !new.prices.contains_key(k));
    }
    new
}

/// Une actualisation en arrière-plan est-elle due ? `last_error_at` : date du dernier échec réseau, s'il y en a un
/// plus récent que le dernier relevé réussi.
pub fn refresh_due(now: u64, fetched_at: Option<u64>, last_error_at: Option<u64>, interval_minutes: u32) -> bool {
    let interval = u64::from(interval_minutes.max(MIN_INTERVAL_MINUTES)) * 60;
    if let Some(e) = last_error_at {
        if now.saturating_sub(e) < RETRY_AFTER_ERROR_SECS {
            return false;
        }
    }
    fetched_at.map_or(true, |f| now.saturating_sub(f) >= interval)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(&format!("poe2-craft-assistant/{} (outil personnel de craft ; prix en cache, actualisation espacée)", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
}

fn get_json(agent: &ureq::Agent, url: &str, query: &[(&str, &str)]) -> Result<Value, String> {
    let mut req = agent.get(url);
    for (k, v) in query {
        req = req.query(k, v);
    }
    let body = req
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(429, _) => "poe.ninja limite les requêtes : réessaie dans quelques minutes.".to_string(),
            ureq::Error::Status(c, _) => format!("poe.ninja a répondu {c}."),
            other => format!("Connexion à poe.ninja impossible : {other}"),
        })?
        .into_string()
        .map_err(|e| format!("Réponse poe.ninja illisible : {e}"))?;
    serde_json::from_str(&body).map_err(|e| format!("Réponse poe.ninja inattendue : {e}"))
}

/// Interroge poe.ninja (ligue courante si `league_pref` est vide) et convertit tous les prix connus du dataset en Exalted.
pub fn fetch(ds: &Dataset, league_pref: &str) -> Result<MarketPrices, String> {
    let base = base_url();
    let agent = agent();
    let league = match league_pref.trim() {
        "" => first_league(&get_json(&agent, &format!("{base}/leagues"), &[])?)?,
        l => l.to_string(),
    };
    let types: BTreeSet<&str> = ds.price_sources.values().map(|s| s.ninja_type.as_str()).collect();
    let (mut prices, mut missing) = (BTreeMap::new(), Vec::new());
    for ty in types {
        let wanted: Vec<(String, String)> = ds.price_sources.iter().filter(|(_, s)| s.ninja_type == ty).map(|(k, s)| (k.clone(), s.ninja_id.clone())).collect();
        let doc = get_json(&agent, &format!("{base}/exchange/current/overview"), &[("league", league.as_str()), ("type", ty)])?;
        let e = extract(&doc, &wanted).map_err(|e| format!("{e} (ligue « {league} », catégorie {ty})"))?;
        prices.extend(e.prices);
        missing.extend(e.missing);
    }
    if prices.is_empty() {
        return Err(format!("Aucun prix trouvé pour la ligue « {league} »."));
    }
    Ok(MarketPrices { league, fetched_at: now_unix(), prices, missing, updated_at: BTreeMap::new() })
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PriceState {
    pub effective: BTreeMap<String, f64>,
    pub overrides: BTreeMap<String, f64>,
    pub market_keys: Vec<String>,
    pub league: Option<String>,
    pub fetched_at: Option<u64>,
    /// price_id -> date (unix) du relevé poe.ninja de ce prix
    pub updated_at: BTreeMap<String, u64>,
    pub missing: Vec<String>,
    pub now: u64,
    /// dernière erreur de l'actualisation en arrière-plan (les prix précédents restent utilisés)
    pub last_error: Option<String>,
    /// date (unix) prévue de la prochaine actualisation en arrière-plan, si elle est activée
    pub next_refresh_at: Option<u64>,
    /// message informatif (ex. « déjà actualisés il y a 2 min »)
    pub note: Option<String>,
}

pub fn state_of(st: &AppState, note: Option<String>) -> PriceState {
    let market = st.market.lock().unwrap().clone();
    let settings = st.settings.lock().unwrap().clone();
    let last_error = st.price_error.lock().unwrap().clone();
    let next_refresh_at = settings.auto_refresh_prices.then(|| {
        let interval = u64::from(settings.price_refresh_minutes.max(MIN_INTERVAL_MINUTES)) * 60;
        let after_success = market.as_ref().map_or(0, |m| m.fetched_at + interval);
        let after_error = last_error.as_ref().map_or(0, |(at, _)| at + RETRY_AFTER_ERROR_SECS);
        after_success.max(after_error)
    });
    PriceState {
        effective: st.prices(),
        overrides: st.price_overrides.lock().unwrap().clone(),
        market_keys: market.as_ref().map(|m| m.prices.keys().cloned().collect()).unwrap_or_default(),
        league: market.as_ref().map(|m| m.league.clone()),
        fetched_at: market.as_ref().map(|m| m.fetched_at),
        updated_at: market.as_ref().map(|m| m.prices.keys().filter_map(|k| Some((k.clone(), m.updated_at_of(k)?))).collect()).unwrap_or_default(),
        missing: market.map(|m| m.missing).unwrap_or_default(),
        now: now_unix(),
        last_error: last_error.map(|(_, e)| e),
        next_refresh_at,
        note,
    }
}

/// Actualise les prix (réseau) sauf si la dernière actualisation date de moins de `MIN_REFETCH_SECS`.
pub fn refresh(st: &AppState) -> Result<PriceState, String> {
    // une seule actualisation à la fois (bouton et arrière-plan) : la seconde voit le relevé tout frais et s'arrête
    let _one_at_a_time = st.price_fetch.lock().unwrap();
    // Le verrou `market` est relâché avant d'appeler `state_of`, qui le reprend : un `if let` sur
    // `st.market.lock()` le garderait jusqu'à la fin du bloc et bloquerait l'appli pour de bon (Mutex non réentrant)
    // dès qu'on clique « Actualiser » moins de 5 min après l'actualisation de démarrage.
    let last = st.market.lock().unwrap().as_ref().map(|m| (m.fetched_at, m.league.clone()));
    if let Some((fetched_at, league)) = last {
        let age = now_unix().saturating_sub(fetched_at);
        let same_league = {
            let pref = st.settings.lock().unwrap().price_league.trim().to_string();
            pref.is_empty() || pref == league
        };
        if age < MIN_REFETCH_SECS && same_league {
            return Ok(state_of(st, Some(format!("Prix déjà actualisés il y a {} min : poe.ninja ne les met à jour qu'environ toutes les heures.", age / 60))));
        }
    }
    let (ds, pref) = (st.dataset(), st.settings.lock().unwrap().price_league.clone());
    let m = match fetch(&ds, &pref) {
        Ok(m) => m,
        Err(e) => {
            // les prix précédents restent en place ; l'erreur est affichée dans les réglages
            *st.price_error.lock().unwrap() = Some((now_unix(), e.clone()));
            return Err(e);
        }
    };
    let m = merge(st.market.lock().unwrap().as_ref(), m);
    *st.price_error.lock().unwrap() = None;
    let _ = std::fs::write(st.data_dir.join("market_prices.json"), serde_json::to_string_pretty(&m).unwrap_or_default());
    *st.market.lock().unwrap() = Some(m);
    Ok(state_of(st, None))
}

/// Recalcule le plan actif avec les prix les plus récents, en repartant du dernier objet capturé
/// compatible (ou de l'objet de départ d'origine, ou d'une base neuve si aucun des deux). Échoue vite et
/// sans rien changer si l'objectif est devenu impossible depuis cet objet (`build_context` renvoie une
/// erreur avant même de lancer le calcul coûteux) — c'est le comportement voulu, pas une panne.
pub(crate) fn refresh_active_plan(app: &tauri::AppHandle, st: &AppState) {
    use tauri::Emitter;
    let Some(ctx) = st.active.lock().unwrap().clone() else { return };
    let mut req = ctx.req.clone();
    if let Some((base, view)) = st.last_item.lock().unwrap().clone() {
        if base == req.base_id {
            req.starting_item = Some(view);
        }
    }
    let (ds, prices) = (st.dataset(), st.prices());
    match craft_api::build_context(&ds, &req, &prices, &std::sync::atomic::AtomicBool::new(false)) {
        Ok(new_ctx) => {
            *st.active.lock().unwrap() = Some(std::sync::Arc::new(new_ctx));
            let _ = app.emit("plan-refreshed", ());
        }
        // objectif devenu impossible depuis le dernier objet connu, ou autre souci : le plan actif
        // reste tel quel plutôt que d'être remplacé par une erreur.
        Err(e) => eprintln!("recalcul du plan actif (prix rafraîchis) : {e}"),
    }
}

/// Actualisation en arrière-plan : au démarrage puis toutes les `price_refresh_minutes` (réglage relu à chaque
/// tour, donc un changement s'applique sans redémarrer). Un échec réseau garde les anciens prix et réessaie
/// 10 minutes plus tard ; rien ne bloque l'application (thread dédié).
pub fn spawn_background_refresh(app: tauri::AppHandle, st: std::sync::Arc<AppState>) {
    use tauri::Emitter;
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(4));
        loop {
            let (enabled, interval) = {
                let s = st.settings.lock().unwrap();
                (s.auto_refresh_prices, s.price_refresh_minutes)
            };
            let fetched_at = st.market.lock().unwrap().as_ref().map(|m| m.fetched_at);
            let last_error_at = st.price_error.lock().unwrap().as_ref().map(|(at, _)| *at);
            if enabled && refresh_due(now_unix(), fetched_at, last_error_at, interval) {
                match refresh(&st) {
                    Ok(s) => {
                        let _ = app.emit("prices-updated", s);
                        refresh_active_plan(&app, &st);
                    }
                    Err(e) => {
                        eprintln!("prix poe.ninja : {e}");
                        let _ = app.emit("prices-updated", state_of(&st, None));
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

#[cfg(test)]
mod schedule_tests {
    use super::*;

    fn market(league: &str, at: u64, prices: &[(&str, f64)], missing: &[&str]) -> MarketPrices {
        MarketPrices {
            league: league.into(),
            fetched_at: at,
            prices: prices.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            missing: missing.iter().map(|s| s.to_string()).collect(),
            updated_at: BTreeMap::new(),
        }
    }

    #[test]
    fn each_price_carries_its_own_date_and_a_price_missing_from_a_refresh_keeps_its_last_value() {
        let first = merge(None, market("Rise", 1_000, &[("chaos", 50.0), ("omen_x", 30.0)], &[]));
        assert_eq!(first.updated_at_of("chaos"), Some(1_000));
        assert_eq!(first.updated_at_of("omen_x"), Some(1_000));

        // une heure plus tard, l'Omen n'est plus coté : il garde 30 Ex et sa date d'origine
        let second = merge(Some(&first), market("Rise", 4_600, &[("chaos", 55.0)], &["omen_x"]));
        assert_eq!(second.prices["chaos"], 55.0);
        assert_eq!(second.updated_at_of("chaos"), Some(4_600));
        assert_eq!(second.prices["omen_x"], 30.0);
        assert_eq!(second.updated_at_of("omen_x"), Some(1_000));
        assert!(second.missing.is_empty(), "un prix conservé n'est plus signalé manquant : {:?}", second.missing);

        // autre ligue : aucun prix de l'ancienne n'est repris
        let other = merge(Some(&second), market("Standard", 5_000, &[("chaos", 9.0)], &["omen_x"]));
        assert!(!other.prices.contains_key("omen_x"));
        assert_eq!(other.missing, vec!["omen_x".to_string()]);
    }

    #[test]
    fn old_market_files_without_dates_fall_back_to_the_fetch_date() {
        let m: MarketPrices = serde_json::from_str(r#"{"league":"Rise","fetchedAt":42,"prices":{"chaos":50.0},"missing":[]}"#).unwrap();
        assert_eq!(m.updated_at_of("chaos"), Some(42));
        assert_eq!(m.updated_at_of("absent"), None);
    }

    #[test]
    fn background_refresh_waits_for_the_interval_and_backs_off_after_an_error() {
        let h = 3_600;
        assert!(refresh_due(10_000, None, None, 60), "jamais relevé : actualisation immédiate");
        assert!(!refresh_due(10_000, Some(10_000 - h + 1), None, 60));
        assert!(refresh_due(10_000, Some(10_000 - h), None, 60));
        // un intervalle trop court est ramené à 15 min
        assert!(!refresh_due(10_000, Some(10_000 - 14 * 60), None, 1));
        assert!(refresh_due(10_000, Some(10_000 - 15 * 60), None, 1));
        // échec réseau il y a 5 min : on attend, les anciens prix restent ; 10 min après : nouvel essai
        assert!(!refresh_due(10_000, Some(0), Some(10_000 - 300), 60));
        assert!(refresh_due(10_000, Some(0), Some(10_000 - RETRY_AFTER_ERROR_SECS), 60));
    }
}

#[cfg(test)]
mod live_tests {
    //! Test de bout en bout du client réseau, sans dépendre de Tauri/WebKit : un vrai serveur HTTP local
    //! (thread std) sert les mêmes réponses que celles récupérées sur poe.ninja (fixtures de craft-data),
    //! et `fetch()` est appelé pour de vrai contre `POE2_NINJA_BASE`.
    use super::*;
    use craft_api::craft_data::Dataset;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // `std::env::set_var` n'est pas thread-safe : un seul test à la fois touche POE2_NINJA_BASE.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/../crates/craft-data/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// Serveur minimal : répond `/leagues` et `/exchange/current/overview?type=…`.
    /// `hits` compte les requêtes reçues, pour vérifier qu'une deuxième actualisation immédiate ne retape pas le réseau.
    fn spawn_server(status_override: Option<u16>) -> (String, std::sync::Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = std::sync::Arc::new(AtomicUsize::new(0));
        let hits2 = hits.clone();
        std::thread::spawn(move || {
            let cur = fixture("ninja_currency.json");
            let rit = fixture("ninja_ritual.json");
            let leagues = r#"[{"id":"Forbidden Rites","name":"Forbidden Rites"},{"id":"Standard","name":"Standard"}]"#;
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                hits2.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("");
                let body = if let Some(code) = status_override {
                    s.write_all(format!("HTTP/1.1 {code} Error
Content-Length: 0
Connection: close

").as_bytes()).ok();
                    continue;
                } else if path.starts_with("/leagues") {
                    leagues
                } else if path.contains("type=Currency") {
                    &cur
                } else if path.contains("type=Ritual") || path.contains("type=") {
                    // pas de relevé réel enregistré pour les autres catégories (Delirium, Essences) : on renvoie
                    // celui des Omens, dont aucune ligne ne correspond, donc leurs prix sont « manquants » et
                    // aucun prix n'est inventé.
                    &rit
                } else {
                    "{}"
                };
                let resp = format!("HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: {}
Connection: close

{}", body.len(), body);
                let _ = s.write_all(resp.as_bytes());
            }
        });
        (format!("http://127.0.0.1:{port}"), hits)
    }

    #[test]
    fn real_http_round_trip_against_real_shaped_responses() {
        let _g = ENV_LOCK.lock().unwrap();
        let (base, hits) = spawn_server(None);
        std::env::set_var("POE2_NINJA_BASE", &base);
        let ds = Dataset::embedded();

        let m = fetch(&ds, "").expect("la requête doit réussir");
        assert_eq!(m.league, "Forbidden Rites", "doit prendre la 1ère ligue quand aucune n'est demandée");
        assert!((m.prices["exalt"] - 1.0).abs() < 1e-9, "{:?}", m.prices.get("exalt"));
        assert!((m.prices["chaos"] - 56.38).abs() < 0.1, "{:?}", m.prices.get("chaos"));
        assert!((m.prices["omen_sinistral_exaltation"] - 37.6).abs() < 0.5, "{:?}", m.prices.get("omen_sinistral_exaltation"));
        assert!(m.prices.len() >= 15, "seuls {} prix trouvés", m.prices.len());
        let types: BTreeSet<&str> = ds.price_sources.values().map(|s| s.ninja_type.as_str()).collect();
        assert_eq!(hits.load(Ordering::SeqCst), 1 + types.len(), "leagues + une requête par catégorie, pas plus");

        // ligue explicitement demandée : /leagues n'est plus interrogé
        hits.store(0, Ordering::SeqCst);
        let m2 = fetch(&ds, "Standard").expect("doit réussir avec une ligue explicite");
        assert_eq!(m2.league, "Standard");
        assert_eq!(hits.load(Ordering::SeqCst), types.len(), "sans /leagues : une requête par catégorie");

        std::env::remove_var("POE2_NINJA_BASE");
    }

    #[test]
    fn refresh_keeps_manual_prices_and_survives_a_network_error_with_the_old_prices() {
        let _g = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("poe2-prices-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let st = AppState::load(dir.clone());
        st.price_overrides.lock().unwrap().insert("chaos".into(), 1.5);

        let (base, _) = spawn_server(None);
        std::env::set_var("POE2_NINJA_BASE", &base);
        let s1 = refresh(&st).expect("première actualisation");
        assert!((st.prices()["chaos"] - 1.5).abs() < 1e-9, "le prix saisi à la main prime toujours");
        assert!(s1.updated_at.contains_key("chaos") && s1.updated_at.contains_key("exalt"), "chaque prix poe.ninja a sa date");
        assert!(s1.last_error.is_none());
        let market_before = st.market.lock().unwrap().clone().unwrap();

        // relevé vieilli artificiellement, puis poe.ninja injoignable : anciens prix conservés, erreur visible
        st.market.lock().unwrap().as_mut().unwrap().fetched_at -= 2 * MIN_REFETCH_SECS;
        std::env::set_var("POE2_NINJA_BASE", "http://127.0.0.1:1");
        assert!(refresh(&st).is_err());
        let after = st.market.lock().unwrap().clone().unwrap();
        assert_eq!(after.prices, market_before.prices, "un échec réseau ne touche pas aux prix");
        assert!((st.prices()["chaos"] - 1.5).abs() < 1e-9);
        let s2 = state_of(&st, None);
        assert!(s2.last_error.as_deref().unwrap_or("").contains("impossible"), "{:?}", s2.last_error);
        assert!(s2.next_refresh_at.unwrap() >= s2.now + RETRY_AFTER_ERROR_SECS - 5, "nouvel essai espacé après l'échec");
        // le relevé reste persisté pour le prochain démarrage
        assert!(dir.join("market_prices.json").exists());

        std::env::remove_var("POE2_NINJA_BASE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clicking_refresh_right_after_a_refresh_answers_instead_of_freezing_the_app() {
        let _g = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("poe2-prices-twice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let st = std::sync::Arc::new(AppState::load(dir.clone()));
        let (base, hits) = spawn_server(None);
        std::env::set_var("POE2_NINJA_BASE", &base);
        refresh(&st).expect("première actualisation (comme celle du démarrage)");
        let after_first = hits.load(Ordering::SeqCst);

        // second clic dans les 5 minutes, dans un thread : s'il reste bloqué, le test échoue au lieu de pendre
        let (tx, rx) = std::sync::mpsc::channel();
        let st2 = st.clone();
        std::thread::spawn(move || {
            let _ = tx.send(refresh(&st2));
        });
        let s = rx.recv_timeout(Duration::from_secs(10)).expect("l'actualisation rapprochée ne doit pas bloquer l'appli").expect("doit réussir");
        assert!(s.note.as_deref().unwrap_or("").contains("déjà actualisés"), "{:?}", s.note);
        assert_eq!(hits.load(Ordering::SeqCst), after_first, "pas de nouvelle requête réseau");
        // l'état reste utilisable ensuite (verrous libérés)
        assert!(st.market.try_lock().is_ok() && st.price_fetch.try_lock().is_ok());
        assert!(!st.prices().is_empty());

        std::env::remove_var("POE2_NINJA_BASE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_errors_and_unreachable_servers_are_reported_in_french_not_panicking() {
        let _g = ENV_LOCK.lock().unwrap();
        let (base, _) = spawn_server(Some(429));
        std::env::set_var("POE2_NINJA_BASE", &base);
        let err = fetch(&Dataset::embedded(), "Standard").unwrap_err();
        assert!(err.contains("limite"), "{err}");

        std::env::set_var("POE2_NINJA_BASE", "http://127.0.0.1:1"); // rien n'écoute
        let err2 = fetch(&Dataset::embedded(), "Standard").unwrap_err();
        assert!(err2.contains("impossible"), "{err2}");
        std::env::remove_var("POE2_NINJA_BASE");
    }
}
