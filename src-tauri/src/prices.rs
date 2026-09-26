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
pub const AUTO_REFRESH_AFTER_SECS: u64 = 3600;

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
    Ok(MarketPrices { league, fetched_at: now_unix(), prices, missing })
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PriceState {
    pub effective: BTreeMap<String, f64>,
    pub overrides: BTreeMap<String, f64>,
    pub market_keys: Vec<String>,
    pub league: Option<String>,
    pub fetched_at: Option<u64>,
    pub missing: Vec<String>,
    pub now: u64,
    /// message informatif (ex. « déjà actualisés il y a 2 min »)
    pub note: Option<String>,
}

pub fn state_of(st: &AppState, note: Option<String>) -> PriceState {
    let market = st.market.lock().unwrap().clone();
    PriceState {
        effective: st.prices(),
        overrides: st.price_overrides.lock().unwrap().clone(),
        market_keys: market.as_ref().map(|m| m.prices.keys().cloned().collect()).unwrap_or_default(),
        league: market.as_ref().map(|m| m.league.clone()),
        fetched_at: market.as_ref().map(|m| m.fetched_at),
        missing: market.map(|m| m.missing).unwrap_or_default(),
        now: now_unix(),
        note,
    }
}

/// Actualise les prix (réseau) sauf si la dernière actualisation date de moins de `MIN_REFETCH_SECS`.
pub fn refresh(st: &AppState) -> Result<PriceState, String> {
    if let Some(m) = st.market.lock().unwrap().as_ref() {
        let age = now_unix().saturating_sub(m.fetched_at);
        let same_league = {
            let pref = st.settings.lock().unwrap().price_league.trim().to_string();
            pref.is_empty() || pref == m.league
        };
        if age < MIN_REFETCH_SECS && same_league {
            return Ok(state_of(st, Some(format!("Prix déjà actualisés il y a {} min : poe.ninja ne les met à jour qu'environ toutes les heures.", age / 60))));
        }
    }
    let (ds, pref) = (st.dataset(), st.settings.lock().unwrap().price_league.clone());
    let m = fetch(&ds, &pref)?;
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

/// Au démarrage : actualisation silencieuse si les prix ont plus d'une heure (ou n'existent pas encore).
pub fn spawn_startup_refresh(app: tauri::AppHandle, st: std::sync::Arc<AppState>) {
    use tauri::Emitter;
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(4));
        if !st.settings.lock().unwrap().auto_refresh_prices {
            return;
        }
        let fresh = st.market.lock().unwrap().as_ref().map_or(false, |m| now_unix().saturating_sub(m.fetched_at) < AUTO_REFRESH_AFTER_SECS);
        if fresh {
            return;
        }
        match refresh(&st) {
            Ok(s) => {
                let _ = app.emit("prices-updated", s);
                refresh_active_plan(&app, &st);
            }
            Err(e) => eprintln!("prix poe.ninja : {e}"),
        }
    });
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

    /// Serveur minimal : répond `/leagues` et `/exchange/current/overview?type=Currency|Ritual`.
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
                } else if path.contains("type=Ritual") {
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
        assert_eq!(hits.load(Ordering::SeqCst), 3, "leagues + Currency + Ritual = 3 requêtes, pas plus");

        // ligue explicitement demandée : /leagues n'est plus interrogé
        hits.store(0, Ordering::SeqCst);
        let m2 = fetch(&ds, "Standard").expect("doit réussir avec une ligue explicite");
        assert_eq!(m2.league, "Standard");
        assert_eq!(hits.load(Ordering::SeqCst), 2, "sans /leagues : 2 requêtes");

        std::env::remove_var("POE2_NINJA_BASE");
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
