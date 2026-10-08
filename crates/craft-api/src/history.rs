//! Historique des crafts : à la fin d'un craft suivi en direct, son coût réel (monnaies comptées saisie par
//! saisie) est rangé à côté du coût que le plan prévoyait. Stockage local (`craft-history.json`).

use crate::live::{LiveSession, Spend};
use crate::PlanContext;
use serde::{Deserialize, Serialize};

/// Fiches gardées au plus (les plus anciennes partent d'abord).
pub const MAX_RECORDS: usize = 500;

/// Une monnaie utilisée pendant un craft : nombre d'utilisations et coût total.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UseLine {
    pub action_id: String,
    pub label: String,
    pub count: u32,
    pub cost: f64,
}

/// Un craft terminé.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftRecord {
    pub id: u64,
    /// date de fin (secondes unix)
    pub finished_at: u64,
    pub base_id: String,
    pub base_name: String,
    pub ilvl: u8,
    /// libellés des affixes voulus
    pub goal: Vec<String>,
    /// objectif atteint (sinon craft abandonné)
    pub success: bool,
    /// coût espéré au départ du suivi, hors première base (en Exalted)
    pub planned_cost: f64,
    /// coût des monnaies comptées, hors première base (en Exalted)
    pub real_cost: f64,
    /// saisies enregistrées
    pub steps: usize,
    pub uses: Vec<UseLine>,
    pub prices_source: String,
}

/// Regroupe les dépenses par monnaie, la plus coûteuse d'abord.
pub fn group_uses<'a>(spends: impl Iterator<Item = &'a Spend>) -> Vec<UseLine> {
    let mut v: Vec<UseLine> = Vec::new();
    for s in spends {
        match v.iter_mut().find(|u| u.action_id == s.action_id) {
            Some(u) => {
                u.count += 1;
                u.cost += s.cost;
            }
            None => v.push(UseLine { action_id: s.action_id.clone(), label: s.label.clone(), count: 1, cost: s.cost }),
        }
    }
    v.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal).then(a.label.cmp(&b.label)));
    v
}

/// Fiche du craft suivi par `s` (objectif atteint ou non d'après l'objet courant).
pub fn finish_record(ctx: &PlanContext, s: &LiveSession, id: u64, finished_at: u64) -> Result<CraftRecord, String> {
    if s.base_id != ctx.req.base_id {
        return Err("l'objet suivi n'est pas de la base du plan actif".into());
    }
    if s.steps() == 0 && s.spends().next().is_none() {
        return Err("rien à enregistrer : aucune saisie depuis le début du suivi".into());
    }
    let it = s.current().to_state(&ctx.bp.pool)?;
    let success = ctx.model.project(&it).is_some_and(|m| ctx.model.is_goal(&m));
    Ok(CraftRecord {
        id,
        finished_at,
        base_id: ctx.req.base_id.clone(),
        base_name: ctx.bp.base.name.clone(),
        ilvl: ctx.req.ilvl,
        goal: ctx.goal_items.iter().map(|g| g.label.clone()).collect(),
        success,
        planned_cost: s.planned_cost,
        real_cost: s.spent(),
        steps: s.steps(),
        uses: group_uses(s.spends()),
        prices_source: ctx.prices_source.clone(),
    })
}

/// Historique, le plus récent en tête.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct History {
    pub records: Vec<CraftRecord>,
}

impl History {
    /// Identifiant libre pour une nouvelle fiche.
    pub fn next_id(&self) -> u64 {
        self.records.iter().map(|r| r.id).max().map_or(1, |m| m + 1)
    }

    pub fn add(&mut self, r: CraftRecord) {
        self.records.insert(0, r);
        self.records.truncate(MAX_RECORDS);
    }

    /// `false` si aucune fiche ne porte cet identifiant.
    pub fn remove(&mut self, id: u64) -> bool {
        let n = self.records.len();
        self.records.retain(|r| r.id != id);
        self.records.len() != n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::{advised_spend, live_start, new_base_spend, spend_of, LiveEdit};
    use crate::{build_context, ItemView, PlanRequest, WantedReq};
    use craft_core::{Rarity, Slot};
    use craft_data::Dataset;
    use std::sync::atomic::AtomicBool;

    fn ctx() -> PlanContext {
        let ds = Dataset::embedded();
        let enabled = ["transmute", "augment", "regal", "exalt", "annul"].iter().map(|s| s.to_string()).collect();
        let req = PlanRequest {
            base_id: "sword_1h".into(),
            ilvl: 82,
            wanted: vec![WantedReq { group: "PhysicalDamage".into(), max_tier: 5 }],
            enabled_actions: Some(enabled),
            prices: None,
            allow_abandon: true,
            mc_trials: 0,
            node_cap: 50,
            seed: 1,
            prices_label: None,
            starting_item: None,
            instill: None,
        };
        build_context(&ds, &req, &ds.prices, &AtomicBool::new(false)).expect("build_context")
    }

    fn junk_suffix(ctx: &PlanContext) -> u16 {
        let goal: Vec<u16> = ctx.goal_items.iter().map(|g| g.group).collect();
        ctx.bp.pool.affixes.iter().position(|a| a.slot == Slot::Suffix && !a.desecrated && a.weight > 0 && a.req_ilvl <= 82 && a.cap_shift == (0, 0) && !goal.contains(&a.group)).unwrap() as u16
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * a.abs().max(1.0)
    }

    /// Bout en bout : un craft suivi saisie par saisie compte la monnaie conseillée par le solveur à chaque
    /// étape (au prix du plan), le rachat d'une base, les corrections et annulations ; la fiche finale range
    /// ce coût réel à côté du coût espéré du plan, et l'historique se relit tel quel depuis son JSON.
    #[test]
    fn a_tracked_craft_records_planned_against_real_cost() {
        let ctx = ctx();
        let mut s = live_start(&ctx);
        let v0 = ctx.solution.value[ctx.solution.id(&ctx.start).unwrap()];
        assert!(v0 > 0.0 && close(s.planned_cost, v0), "coût prévu = coût espéré du plan au départ : {} contre {v0}", s.planned_cost);
        assert!(finish_record(&ctx, &s, 1, 0).is_err(), "rien saisi : rien à enregistrer");

        // Le solveur conseille un Transmute sur la base neuve : c'est lui qui est compté, à son prix.
        let transmute = advised_spend(&ctx, &s).expect("un coup conseillé");
        assert_eq!(transmute.action_id, "transmute");
        assert!(close(transmute.cost, spend_of(&ctx, "transmute").unwrap().cost) && transmute.cost > 0.0);
        let bad = junk_suffix(&ctx);
        s.apply_spending(&ctx.bp.pool, &[LiveEdit::Add { affix_idx: bad }], Some(transmute.clone())).unwrap();
        assert!(close(s.spent(), transmute.cost));
        assert_eq!(s.last_spend(), Some(&transmute));

        // Une saisie annulée ne coûte rien.
        assert!(s.undo());
        assert_eq!(s.spent(), 0.0);
        let sp = advised_spend(&ctx, &s);
        s.apply_spending(&ctx.bp.pool, &[LiveEdit::Add { affix_idx: bad }], sp).unwrap();

        // Le conseil sur un Magique raté n'est plus un Transmute ; la saisie suivante le compte.
        let next = advised_spend(&ctx, &s);
        assert_ne!(next.as_ref().map(|x| x.action_id.as_str()), Some("transmute"));

        // On rachète une base : prix de la base moins sa revente, comme le solveur.
        s.set_spending(ItemView { rarity: Rarity::Normal, ilvl: 82, mods: vec![] }, Some(new_base_spend(&ctx)));
        assert!(close(s.spent(), transmute.cost + ctx.model.abandon_extra));

        // Transmute réussi, saisi d'abord comme une Augment par erreur puis corrigé après coup.
        let good = ctx.bp.pool.affixes.iter().position(|a| a.id == "LocalAddedPhysicalDamage5").unwrap() as u16;
        s.apply_spending(&ctx.bp.pool, &[LiveEdit::Add { affix_idx: good }], Some(spend_of(&ctx, "augment").unwrap())).unwrap();
        s.set_last_spend(Some(advised_spend_before_last(&ctx))).unwrap();
        let expected = 2.0 * transmute.cost + ctx.model.abandon_extra;
        assert!(close(s.spent(), expected), "{} contre {expected}", s.spent());

        let r = finish_record(&ctx, &s, 7, 1_760_000_000).unwrap();
        assert!(r.success, "le seul affixe voulu est sur l'objet");
        assert!(close(r.planned_cost, v0) && close(r.real_cost, expected));
        assert_eq!(r.steps, 3);
        assert_eq!(r.goal.len(), 1);
        assert_eq!(r.base_name, ctx.bp.base.name);
        let count = |id: &str| r.uses.iter().find(|u| u.action_id == id).map_or(0, |u| u.count);
        assert_eq!((count("transmute"), count(crate::live::NEW_BASE), count("augment")), (2, 1, 0));
        assert!(close(r.uses.iter().map(|u| u.cost).sum::<f64>(), r.real_cost));

        // Craft abandonné : un Transmute raté puis on arrête.
        let mut s2 = live_start(&ctx);
        let sp = advised_spend(&ctx, &s2);
        s2.apply_spending(&ctx.bp.pool, &[LiveEdit::Add { affix_idx: bad }], sp).unwrap();
        let mut h = History::default();
        h.add(r.clone());
        let r2 = finish_record(&ctx, &s2, h.next_id(), 1_760_000_100).unwrap();
        assert!(!r2.success && r2.id == 8);
        h.add(r2);
        assert_eq!(h.records[0].id, 8, "le plus récent en tête");

        let back: History = serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!(back.records, h.records);
        assert!(h.remove(7) && !h.remove(7));
        assert_eq!(h.records.len(), 1);
    }

    /// Le Transmute qu'aurait conseillé le solveur sur une base neuve.
    fn advised_spend_before_last(ctx: &PlanContext) -> Spend {
        advised_spend(ctx, &live_start(ctx)).unwrap()
    }

    #[test]
    fn history_keeps_the_most_recent_records() {
        let mut h = History::default();
        let rec = |id| CraftRecord { id, finished_at: 0, base_id: "b".into(), base_name: "B".into(), ilvl: 80, goal: vec![], success: true, planned_cost: 1.0, real_cost: 2.0, steps: 1, uses: vec![], prices_source: String::new() };
        for _ in 0..MAX_RECORDS + 5 {
            let id = h.next_id();
            h.add(rec(id));
        }
        assert_eq!(h.records.len(), MAX_RECORDS);
        assert_eq!(h.records[0].id, (MAX_RECORDS + 5) as u64);
        // un ancien fichier sans champ ne casse rien
        assert!(serde_json::from_str::<History>("{}").unwrap().records.is_empty());
    }
}
