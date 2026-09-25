//! Prix du marché : lecture d'une réponse `exchange/current/overview` de poe.ninja (PoE2).
//!
//! Format (vérifié sur une réponse réelle) : `{ core: { primary, secondary, rates, items }, lines: [{ id, primaryValue, … }], items: [{ id, name, … }] }`.
//! `primaryValue` est exprimé dans `core.primary` (aujourd'hui « divine », PAS l'Exalted) ; `core.rates[x]` = quantité de `x`
//! pour 1 unité de la référence. Tous les prix sont convertis en Exalted Orb, l'unité du dataset.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Où trouver un prix : catégorie de l'API (« Currency », « Ritual » = Omens…) et identifiant poe.ninja.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceSource {
    pub ninja_type: String,
    pub ninja_id: String,
}

/// Facteur « unité de référence de la réponse → Exalted ».
/// Si la réponse contient la ligne « exalted », on s'en sert comme ancre (l'Exalted vaut alors exactement 1) ;
/// sinon on retombe sur `core.rates.exalted`, arrondi par poe.ninja.
pub fn exalted_factor(doc: &Value) -> Result<f64, String> {
    let primary = doc["core"]["primary"].as_str().ok_or("réponse poe.ninja inattendue : core.primary absent")?;
    if primary == "exalted" {
        return Ok(1.0);
    }
    let anchor = doc["lines"].as_array().and_then(|l| l.iter().find(|l| l["id"].as_str() == Some("exalted"))).and_then(|l| l["primaryValue"].as_f64()).filter(|v| v.is_finite() && *v > 0.0);
    if let Some(v) = anchor {
        return Ok(1.0 / v);
    }
    doc["core"]["rates"]["exalted"]
        .as_f64()
        .filter(|r| r.is_finite() && *r > 0.0)
        .ok_or_else(|| format!("aucun taux de change vers l'Exalted (référence de la réponse : « {primary} »)"))
}

#[derive(Debug)]
pub struct Extracted {
    /// price_id -> prix en Exalted
    pub prices: BTreeMap<String, f64>,
    /// price_id dont l'objet est introuvable ou sans prix exploitable dans cette réponse
    pub missing: Vec<String>,
}

/// `wanted` : paires (price_id, ninja_id) à chercher dans cette réponse.
pub fn extract(doc: &Value, wanted: &[(String, String)]) -> Result<Extracted, String> {
    let lines = doc["lines"].as_array().ok_or("réponse poe.ninja inattendue : lines absent")?;
    if lines.is_empty() {
        return Err("réponse vide : cette ligue n'a pas encore de données de prix".into());
    }
    let factor = exalted_factor(doc)?;
    let (mut prices, mut missing) = (BTreeMap::new(), Vec::new());
    for (price_id, ninja_id) in wanted {
        let v = lines.iter().find(|l| l["id"].as_str() == Some(ninja_id)).and_then(|l| l["primaryValue"].as_f64()).filter(|v| v.is_finite() && *v > 0.0);
        match v {
            Some(v) => {
                prices.insert(price_id.clone(), v * factor);
            }
            None => missing.push(price_id.clone()),
        }
    }
    Ok(Extracted { prices, missing })
}

/// Premier élément de la liste des ligues (« la première est la ligue temporaire courante »).
pub fn first_league(doc: &Value) -> Result<String, String> {
    doc.as_array().and_then(|a| a.first()).and_then(|l| l["id"].as_str()).map(str::to_string).ok_or_else(|| "liste de ligues poe.ninja vide ou inattendue".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(name: &str) -> Value {
        serde_json::from_str(&std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
    }
    fn w(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn real_currency_response_is_converted_from_divine_to_exalted() {
        let doc = fixture("ninja_currency.json");
        assert_eq!(doc["core"]["primary"], "divine"); // la référence n'est PAS l'Exalted
        let r = extract(&doc, &w(&[("exalt", "exalted"), ("chaos", "chaos"), ("annul", "annul"), ("fracture", "fracturing-orb"), ("alchemy", "alch")])).unwrap();
        assert!((r.prices["exalt"] - 1.0).abs() < 1e-9, "l'Exalted vaut 1 Exalted : {}", r.prices["exalt"]);
        assert!((r.prices["chaos"] - 56.38).abs() < 0.1, "{}", r.prices["chaos"]);
        assert!((r.prices["annul"] - 304.4).abs() < 0.5, "{}", r.prices["annul"]);
        assert!((r.prices["fracture"] - 4023.5).abs() < 5.0, "{}", r.prices["fracture"]);
        assert!((r.prices["alchemy"] - 3.17).abs() < 0.05, "{}", r.prices["alchemy"]);
        assert!(r.missing.is_empty());
    }

    #[test]
    fn omens_come_from_the_ritual_category() {
        let r = extract(&fixture("ninja_ritual.json"), &w(&[("omen_sinistral_exaltation", "omen-of-sinistral-exaltation"), ("omen_dextral_annulment", "omen-of-dextral-annulment")])).unwrap();
        assert!((r.prices["omen_sinistral_exaltation"] - 37.6).abs() < 0.5);
        assert!((r.prices["omen_dextral_annulment"] - 4066.0).abs() < 10.0);
    }

    #[test]
    fn unknown_or_zero_priced_items_are_reported_not_invented() {
        let doc = json!({"core": {"primary": "exalted", "rates": {}}, "lines": [{"id": "a", "primaryValue": 2.5}, {"id": "z", "primaryValue": 0.0}]});
        let r = extract(&doc, &w(&[("pa", "a"), ("pz", "z"), ("pn", "absent")])).unwrap();
        assert_eq!(r.prices.len(), 1);
        assert_eq!(r.prices["pa"], 2.5); // référence = Exalted : pas de conversion
        assert_eq!(r.missing, ["pz", "pn"]);
    }

    #[test]
    fn malformed_or_empty_responses_are_errors() {
        assert!(extract(&json!({"core": {"primary": "divine"}, "lines": []}), &[]).unwrap_err().contains("vide"));
        assert!(extract(&json!({"lines": [{"id": "a", "primaryValue": 1}]}), &[]).is_err());
        let no_rate = json!({"core": {"primary": "divine", "rates": {"chaos": 8.0}}, "lines": [{"id": "a", "primaryValue": 1.0}]});
        assert!(extract(&no_rate, &w(&[("p", "a")])).unwrap_err().contains("Exalted"));
    }

    #[test]
    fn league_list_takes_the_first_entry() {
        assert_eq!(first_league(&json!([{"id": "Forbidden Rites", "name": "Forbidden Rites"}, {"id": "Standard"}])).unwrap(), "Forbidden Rites");
        assert!(first_league(&json!([])).is_err());
    }

    #[test]
    fn every_priced_action_of_the_sample_dataset_has_a_source() {
        let ds = crate::Dataset::embedded();
        // "base_*", "essence_*", "desecrate_*" et les Omens de Désécration : prix estimés à la main
        // (poe.ninja ne suit pas encore ces mécaniques dans notre import), documenté dans meta.notice —
        // exception volontaire, pas un oubli.
        let manual: std::collections::HashSet<&str> = [
            "omen_sovereign",
            "omen_liege",
            "omen_blackblooded",
            "omen_sinistral_necromancy",
            "omen_dextral_necromancy",
            "omen_sinistral_coronation",
            "omen_dextral_coronation",
            "omen_sinistral_erasure",
            "omen_dextral_erasure",
            "omen_light",
            "omen_whittling",
            "alloy_mystic",
        ]
        .into_iter()
        .collect();
        for key in ds.prices.keys().filter(|k| !k.starts_with("base_") && !k.starts_with("essence_") && !k.starts_with("desecrate_") && !k.starts_with("liquid_") && !manual.contains(k.as_str())) {
            assert!(ds.price_sources.contains_key(key), "pas de source poe.ninja pour « {key} »");
        }
    }
}
