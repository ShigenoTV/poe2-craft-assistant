//! Suivi de craft en direct : l'utilisateur saisit ce qu'il vient d'obtenir en jeu (affixe ajouté, retiré,
//! remplacé, rareté changée) et le conseil est recalculé sur l'objet réel ainsi décrit. Chaque saisie est
//! une étape d'historique : une erreur se corrige en annulant la dernière étape.

use crate::{advise_item, detail, AdviceResult, ItemDetail, ItemView, ModView, PlanContext};
use craft_core::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;

/// Une modification de l'objet saisie à la main.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LiveEdit {
    /// Affixe obtenu (Transmute, Augment, Regal, Exalt, Essence...). Un objet Normal devient Magique, un
    /// objet Magique qui n'a plus de place dans ce slot devient Rare : un seul clic après un Regal.
    #[serde(rename_all = "camelCase")]
    Add { affix_idx: u16 },
    /// Affixe retiré (Annulment) ou saisi par erreur.
    #[serde(rename_all = "camelCase")]
    Remove { affix_idx: u16 },
    /// Un affixe remplacé par un autre (Chaos, Essence Perfect) ou un tier corrigé ; garde l'état fracturé.
    #[serde(rename_all = "camelCase")]
    Replace { from: u16, to: u16 },
    /// Nouvelle rareté ; repasser en Normal retire tous les affixes.
    #[serde(rename_all = "camelCase")]
    Rarity { rarity: Rarity },
    /// Affixe devenu fracturé (Fracturing Orb).
    #[serde(rename_all = "camelCase")]
    Fracture { affix_idx: u16 },
}

const MAX_HISTORY: usize = 200;

/// Objet suivi et son historique de saisies (le dernier élément est l'état courant).
#[derive(Clone, Debug)]
pub struct LiveSession {
    pub base_id: String,
    history: Vec<ItemView>,
}

impl LiveSession {
    pub fn new(base_id: impl Into<String>, item: ItemView) -> Self {
        Self { base_id: base_id.into(), history: vec![item] }
    }

    pub fn current(&self) -> &ItemView {
        self.history.last().expect("historique jamais vide")
    }

    pub fn can_undo(&self) -> bool {
        self.history.len() > 1
    }

    /// Nombre de saisies enregistrées depuis le début du suivi.
    pub fn steps(&self) -> usize {
        self.history.len() - 1
    }

    /// Applique les modifications d'un seul coup (une seule étape d'historique) ; rien n'est changé en cas d'erreur.
    pub fn apply(&mut self, pool: &AffixPool, edits: &[LiveEdit]) -> Result<(), String> {
        let mut it = self.current().to_state(pool)?;
        for e in edits {
            apply_edit(pool, &mut it, e)?;
        }
        self.push(ItemView::from_state(&it));
        Ok(())
    }

    /// Remplace l'objet suivi par un état connu (objet copié en jeu, base neuve), annulable comme une saisie.
    pub fn set(&mut self, item: ItemView) {
        self.push(item);
    }

    /// Revient à l'état précédent ; `false` s'il n'y en a pas.
    pub fn undo(&mut self) -> bool {
        if !self.can_undo() {
            return false;
        }
        self.history.pop();
        true
    }

    fn push(&mut self, item: ItemView) {
        let same = {
            let c = self.current();
            c.rarity == item.rarity && c.ilvl == item.ilvl && c.mods.len() == item.mods.len() && c.mods.iter().zip(&item.mods).all(|(a, b)| a.affix_idx == b.affix_idx && a.fractured == b.fractured)
        };
        if same {
            return; // rien n'a changé : pas d'étape vide à annuler
        }
        self.history.push(item);
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }
}

fn pos_of(it: &ItemState, idx: u16) -> Result<usize, String> {
    it.mods().iter().position(|m| m.idx == idx).ok_or_else(|| "cet affixe n'est pas sur l'objet".to_string())
}

fn check_affix(pool: &AffixPool, idx: u16) -> Result<&Affix, String> {
    pool.affixes.get(idx as usize).ok_or_else(|| "affixe hors du pool de cette base".to_string())
}

/// Vérifie qu'un ajout est possible (groupe libre, une seule Désécration, six affixes au plus).
fn check_add(pool: &AffixPool, it: &ItemState, idx: u16) -> Result<(), String> {
    let a = check_affix(pool, idx)?;
    if it.mods().iter().any(|m| pool.affixes[m.idx as usize].group == a.group) {
        return Err(format!("l'objet porte déjà un affixe du groupe « {} » (retire-le ou remplace-le)", a.family));
    }
    if a.desecrated && pool.has_desecrated(it) {
        return Err("l'objet porte déjà un mod Désécré".into());
    }
    if it.len() >= 6 {
        return Err("l'objet a déjà six affixes".into());
    }
    Ok(())
}

/// Chaque slot reste sous le plafond de l'objet à sa rareté.
fn check_cap(pool: &AffixPool, it: &ItemState) -> Result<(), String> {
    let (cp, cs) = pool.cap_of(it);
    if pool.count(it, Slot::Prefix) > cp {
        return Err(format!("trop de préfixes pour cette rareté ({cp} au plus)"));
    }
    if pool.count(it, Slot::Suffix) > cs {
        return Err(format!("trop de suffixes pour cette rareté ({cs} au plus)"));
    }
    Ok(())
}

pub fn apply_edit(pool: &AffixPool, it: &mut ItemState, e: &LiveEdit) -> Result<(), String> {
    match *e {
        LiveEdit::Add { affix_idx } => {
            check_add(pool, it, affix_idx)?;
            if it.rarity == Rarity::Normal {
                it.rarity = Rarity::Magic;
            }
            it.push(Mod { idx: affix_idx, fractured: false });
            if it.rarity == Rarity::Magic && check_cap(pool, it).is_err() {
                it.rarity = Rarity::Rare;
            }
            check_cap(pool, it)
        }
        LiveEdit::Remove { affix_idx } => {
            let p = pos_of(it, affix_idx)?;
            it.remove(p);
            Ok(())
        }
        LiveEdit::Replace { from, to } => {
            let p = pos_of(it, from)?;
            let fractured = it.mods()[p].fractured;
            it.remove(p);
            check_add(pool, it, to)?;
            it.push(Mod { idx: to, fractured });
            check_cap(pool, it)
        }
        LiveEdit::Rarity { rarity } => {
            it.rarity = rarity;
            if rarity == Rarity::Normal {
                *it = ItemState::new(Rarity::Normal, it.ilvl);
            }
            check_cap(pool, it)
        }
        LiveEdit::Fracture { affix_idx } => {
            let p = pos_of(it, affix_idx)?;
            if it.has_fractured() {
                return Err("l'objet porte déjà un affixe fracturé".into());
            }
            it.set_fractured(p);
            Ok(())
        }
    }
}

/// Objet suivi et conseil du solveur pour lui.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveView {
    pub base_id: String,
    pub item: ItemDetail,
    pub advice: Option<AdviceResult>,
    pub advice_error: Option<String>,
    pub can_undo: bool,
    pub steps: usize,
}

/// Recalcule le meilleur coup suivant pour l'objet suivi (résolution à la volée si l'état est nouveau).
pub fn live_view(ctx: &PlanContext, s: &LiveSession, cancel: &AtomicBool) -> Result<LiveView, String> {
    if s.base_id != ctx.req.base_id {
        return Err("l'objet suivi n'est pas de la base du plan actif".into());
    }
    let it = s.current().to_state(&ctx.bp.pool)?;
    let (advice, advice_error) = match advise_item(ctx, &it, cancel) {
        Ok(a) => (Some(a), None),
        Err(e) => (None, Some(e)),
    };
    Ok(LiveView { base_id: s.base_id.clone(), item: detail(&ctx.bp.pool, &it), advice, advice_error, can_undo: s.can_undo(), steps: s.steps() })
}

/// Point de départ du suivi pour un plan : l'objet de départ du plan s'il y en a un, sinon une base neuve.
pub fn live_start(ctx: &PlanContext) -> LiveSession {
    let item = ctx.req.starting_item.clone().unwrap_or(ItemView { rarity: Rarity::Normal, ilvl: ctx.req.ilvl, mods: Vec::<ModView>::new() });
    LiveSession::new(ctx.req.base_id.clone(), item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_context, PlanRequest, WantedReq};
    use craft_data::Dataset;

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

    fn idx(ctx: &PlanContext, id: &str) -> u16 {
        ctx.bp.pool.affixes.iter().position(|a| a.id == id).unwrap_or_else(|| panic!("{id} absent du pool")) as u16
    }

    /// Un affixe non voulu du slot demandé, d'un autre groupe que ceux déjà pris.
    fn junk(ctx: &PlanContext, slot: Slot, avoid: &[u16]) -> u16 {
        let goal_groups: Vec<u16> = ctx.goal_items.iter().map(|g| g.group).collect();
        let taken: Vec<u16> = avoid.iter().map(|&i| ctx.bp.pool.affixes[i as usize].group).collect();
        ctx.bp
            .pool
            .affixes
            .iter()
            .position(|a| a.slot == slot && !a.desecrated && a.weight > 0 && a.req_ilvl <= 82 && a.cap_shift == (0, 0) && !goal_groups.contains(&a.group) && !taken.contains(&a.group))
            .expect("affixe inutile introuvable") as u16
    }

    fn view(ctx: &PlanContext, s: &LiveSession) -> LiveView {
        live_view(ctx, s, &AtomicBool::new(false)).expect("live_view")
    }

    fn action_id(v: &LiveView) -> Option<String> {
        v.advice.as_ref()?.advice.as_ref()?.action.as_ref().map(|a| a.id.clone())
    }

    fn cost_to_go(v: &LiveView) -> f64 {
        v.advice.as_ref().unwrap().advice.as_ref().unwrap().cost_to_go.unwrap()
    }

    /// Bout en bout : chaque saisie de l'objet obtenu change l'état vu par le solveur, et le conseil suit.
    /// Base neuve → Transmute ; mod voulu obtenu → coût restant en baisse et objectif atteint ; mod inutile
    /// obtenu à la place → le solveur ne conseille plus de Transmute (l'objet n'est plus Normal) ; une
    /// erreur de saisie annulée rend exactement le conseil précédent.
    #[test]
    fn live_tracking_recomputes_the_next_step_after_each_input() {
        let ctx = ctx();
        let mut s = live_start(&ctx);
        let v0 = view(&ctx, &s);
        assert_eq!(v0.item.view.rarity, Rarity::Normal);
        assert_eq!(action_id(&v0).as_deref(), Some("transmute"), "base neuve : le premier coup doit être un Transmute");
        let c0 = cost_to_go(&v0);

        // Transmute raté : un suffixe inutile. L'objet devient Magique tout seul.
        let bad = junk(&ctx, Slot::Suffix, &[]);
        s.apply(&ctx.bp.pool, &[LiveEdit::Add { affix_idx: bad }]).unwrap();
        let v1 = view(&ctx, &s);
        assert_eq!(v1.item.view.rarity, Rarity::Magic);
        assert_eq!(v1.advice.as_ref().unwrap().advice.as_ref().unwrap().state.bad_suffixes, 1);
        assert_ne!(action_id(&v1).as_deref(), Some("transmute"), "un objet Magique ne prend plus de Transmute");
        assert_eq!(v1.advice.as_ref().unwrap().wanted_status, vec!["missing".to_string()]);

        // Erreur de saisie : en fait c'était le bon préfixe. On annule puis on saisit le bon.
        assert!(s.undo());
        let back = view(&ctx, &s);
        assert_eq!(action_id(&back), action_id(&v0));
        assert_eq!(back.advice.as_ref().unwrap().advice.as_ref().unwrap().state_key, v0.advice.as_ref().unwrap().advice.as_ref().unwrap().state_key);

        let good = idx(&ctx, "LocalAddedPhysicalDamage5");
        s.apply(&ctx.bp.pool, &[LiveEdit::Add { affix_idx: good }]).unwrap();
        let v2 = view(&ctx, &s);
        let a2 = v2.advice.as_ref().unwrap();
        assert_eq!(a2.wanted_status, vec!["held".to_string()]);
        assert!(a2.advice.as_ref().unwrap().goal_reached, "le seul affixe voulu est là : objectif atteint");
        assert!(cost_to_go(&v2) < c0);
        assert_eq!(v2.steps, 1);

        // Le même mod en tier trop bas (correction du tier) bloque le groupe : le conseil repart.
        let low = ctx.bp.pool.affixes.iter().position(|a| a.group == ctx.bp.pool.affixes[good as usize].group && a.family_id == ctx.bp.pool.affixes[good as usize].family_id && a.tier > 5).expect("tier bas") as u16;
        s.apply(&ctx.bp.pool, &[LiveEdit::Replace { from: good, to: low }]).unwrap();
        let v3 = view(&ctx, &s);
        assert_eq!(v3.advice.as_ref().unwrap().wanted_status, vec!["blocked".to_string()]);
        assert!(action_id(&v3).is_some() && !v3.advice.as_ref().unwrap().advice.as_ref().unwrap().goal_reached);
    }

    /// Bout en bout : un objet Magique plein qui reçoit un affixe (Regal) passe Rare sans saisie de rareté,
    /// et le solveur conseille alors un coup d'objet Rare (jamais Augment/Regal).
    #[test]
    fn live_tracking_promotes_a_full_magic_item_to_rare() {
        let ctx = ctx();
        let mut s = live_start(&ctx);
        let p = junk(&ctx, Slot::Prefix, &[]);
        let s1 = junk(&ctx, Slot::Suffix, &[p]);
        s.apply(&ctx.bp.pool, &[LiveEdit::Add { affix_idx: p }, LiveEdit::Add { affix_idx: s1 }]).unwrap();
        assert_eq!(s.current().rarity, Rarity::Magic);
        assert_eq!(s.steps(), 1, "plusieurs modifications envoyées ensemble = une seule étape");
        let s2 = junk(&ctx, Slot::Suffix, &[p, s1]);
        s.apply(&ctx.bp.pool, &[LiveEdit::Add { affix_idx: s2 }]).unwrap();
        let v = view(&ctx, &s);
        assert_eq!(v.item.view.rarity, Rarity::Rare);
        let id = action_id(&v).expect("un coup doit être conseillé");
        assert!(!["augment", "regal", "transmute"].contains(&id.as_str()), "coup d'objet Magique conseillé sur un Rare : {id}");
    }

    #[test]
    fn live_edits_reject_impossible_items_without_changing_anything() {
        let ctx = ctx();
        let pool = &ctx.bp.pool;
        let mut s = live_start(&ctx);
        let good = idx(&ctx, "LocalAddedPhysicalDamage5");
        s.apply(pool, &[LiveEdit::Add { affix_idx: good }]).unwrap();
        let before = s.current().clone();
        let same_group = pool.affixes.iter().position(|a| a.group == pool.affixes[good as usize].group && a.id != pool.affixes[good as usize].id).unwrap() as u16;
        assert!(s.apply(pool, &[LiveEdit::Add { affix_idx: same_group }]).is_err(), "deux affixes du même groupe");
        assert!(s.apply(pool, &[LiveEdit::Remove { affix_idx: same_group }]).is_err(), "retrait d'un affixe absent");
        // quatre préfixes : impossible même en Rare (3 au plus)
        let mut taken = vec![good];
        let mut edits = vec![];
        for _ in 0..3 {
            let j = junk(&ctx, Slot::Prefix, &taken);
            taken.push(j);
            edits.push(LiveEdit::Add { affix_idx: j });
        }
        assert!(s.apply(pool, &edits).is_err());
        assert_eq!(s.current().mods.len(), before.mods.len(), "une saisie refusée ne change rien");
        assert_eq!(s.steps(), 1);
        // fracture unique ; repasser Normal vide l'objet
        assert!(s.apply(pool, &[LiveEdit::Rarity { rarity: Rarity::Rare }, LiveEdit::Fracture { affix_idx: good }]).is_ok());
        assert!(s.apply(pool, &[LiveEdit::Fracture { affix_idx: good }]).is_err(), "un seul affixe fracturé");
        s.apply(pool, &[LiveEdit::Rarity { rarity: Rarity::Normal }]).unwrap();
        assert!(s.current().mods.is_empty());
        assert_eq!(s.steps(), 3);
    }
}
