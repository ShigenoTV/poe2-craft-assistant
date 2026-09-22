//! CLI de développement : `craft-cli solve gloves_dex 81 IncreasedLife:3 FireResistance:2 ...`
use craft_api::craft_core::{ItemState, Mod, Rarity};
use craft_api::craft_data::ParsedItem;
use craft_api::*;
use craft_solver::CraftNode;
use std::io::Read;
use std::sync::atomic::AtomicBool;

fn usage() -> ! {
    eprintln!(
        "usage:\n  craft-cli bases\n  craft-cli groups <base>\n  craft-cli solve <base> <ilvl> <groupe:tierMax>... [--json <fichier>] [--trials N]\n  craft-cli parse <fichier|-> [base]"
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ds = craft_data::Dataset::embedded();
    match args.first().map(|s| s.as_str()) {
        Some("bases") => ds.bases.iter().for_each(|b| println!("{:12} {} ({})", b.id, b.name, b.item_class)),
        Some("groups") => {
            let bp = ds.build_pool(args.get(1).unwrap_or_else(|| usage())).unwrap();
            for g in &bp.groups {
                println!("{:16} {:7?} {:2} tiers  poids {:5}  {}", g.key, g.slot, g.tiers.len(), g.total_weight, g.family);
            }
        }
        Some("solve") => {
            let (base, ilvl) = (args.get(1).unwrap_or_else(|| usage()).clone(), args.get(2).unwrap_or_else(|| usage()).parse::<u8>().unwrap());
            let mut wanted = vec![];
            let (mut json, mut trials) = (None, 20_000u64);
            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        json = Some(args[i + 1].clone());
                        i += 1
                    }
                    "--trials" => {
                        trials = args[i + 1].parse().unwrap();
                        i += 1
                    }
                    w => {
                        let (g, t) = w.split_once(':').unwrap_or((w, "8"));
                        wanted.push(WantedReq { group: g.into(), max_tier: t.parse().unwrap() });
                    }
                }
                i += 1;
            }
            let req = PlanRequest { base_id: base, ilvl, wanted, enabled_actions: None, prices: None, allow_abandon: true, mc_trials: trials, node_cap: 220, seed: 42, prices_label: None };
            let t0 = std::time::Instant::now();
            let ctx = build_context(&ds, &req, &ds.prices, &AtomicBool::new(false)).unwrap_or_else(|e| {
                eprintln!("erreur : {e}");
                std::process::exit(1)
            });
            let plan = make_plan(&ctx, |_, _| true).unwrap();
            println!(
                "Objectif : {}\nÉtats : {}  sweeps : {}  convergé : {}  ({} ms résolution, {} ms total)",
                plan.goal.iter().map(|g| g.label.as_str()).collect::<Vec<_>>().join(" | "),
                plan.solver.states, plan.solver.sweeps, plan.solver.converged, plan.solver.millis, t0.elapsed().as_millis()
            );
            println!("Coût espéré (hors 1re base) : {:.2}   contrôle par visites : {:.2}", plan.expected_cost, plan.solver.cost_from_visits);
            if let Some(mc) = &plan.mc {
                println!(
                    "Monte-Carlo exact ({} essais) : moyenne {:.2} [{:.2} ; {:.2}]  médiane {:.2}  P90 {:.2}  P99 {:.2}  écart au modèle {:+.1} %",
                    mc.trials, mc.mean_cost, mc.ci95_mean.0, mc.ci95_mean.1, mc.median_cost, mc.p90_cost, mc.p99_cost,
                    (mc.mean_cost / plan.expected_cost - 1.0) * 100.0
                );
            }
            println!("\nListe de courses attendue :");
            for l in plan.shopping.iter().take(8) {
                println!("  {:>9.2} × {:<58} = {:>9.2}", l.expected_count, l.label, l.expected_cost);
            }
            println!("\nPremière étape :");
            if let Some(CraftNode::Action(n)) = plan.nodes.get(&plan.root_id) {
                println!("  {}  (coût restant {:.2})", n.action.label, n.cost_to_go);
                for b in &n.branches {
                    println!("    {:>6.1} % [{}] {}", b.probability * 100.0, b.kind, b.label);
                }
            }
            println!("\nNœuds du plan : {}", plan.nodes.len());
            if let Some(p) = json {
                std::fs::write(&p, serde_json::to_string(&plan).unwrap()).unwrap();
                println!("→ {p}");
            }
        }
        Some("export-fixtures") => {
            // données réelles du moteur pour le mode « mock » du frontend (dev dans un navigateur)
            let dir = std::path::PathBuf::from(args.get(1).unwrap_or_else(|| usage()));
            std::fs::create_dir_all(&dir).unwrap();
            let w = |name: &str, v: &dyn erased::Ser| std::fs::write(dir.join(name), v.json()).unwrap();
            w("dataset_info.json", &craft_api::dataset_info(&ds));
            w("actions.json", &list_actions(&ds, &ds.prices).unwrap());
            for b in &ds.bases {
                w(&format!("pool_{}.json", b.id), &pool_view(&ds, &b.id).unwrap());
            }
            w("prices.json", &ds.prices);
            let req = PlanRequest {
                base_id: "gloves_dex".into(),
                ilvl: 81,
                wanted: ["IncreasedLife", "FireResistance", "ColdResistance", "IncreasedAccuracy"].iter().map(|g| WantedReq { group: (*g).into(), max_tier: 3 }).collect(),
                enabled_actions: None,
                prices: None,
                allow_abandon: true,
                mc_trials: 20_000,
                node_cap: 220,
                seed: 42,
                prices_label: None,
            };
            let ctx = build_context(&ds, &req, &ds.prices, &AtomicBool::new(false)).unwrap();
            w("plan.json", &make_plan(&ctx, |_, _| true).unwrap());
            // capture d'exemple : rare avec Vie T2 (voulue), Évasion % T6 (tier trop bas → bloque), 1 mauvais suffixe
            let pick = |key: &str, tier: u8| ctx.bp.groups.iter().find(|g| g.key == key).unwrap().tiers.iter().find(|t| t.tier == tier).unwrap().affix_idx;
            let mut it = ItemState::new(Rarity::Rare, 81);
            for (k, t) in [("IncreasedLife", 2u8), ("BaseLocalDefences", 6), ("FireResistance", 2), ("IncreasedAccuracy", 4)] {
                it.push(Mod { idx: pick(k, t), fractured: k == "IncreasedLife" });
            }
            let advice = advise_item(&ctx, &it, &AtomicBool::new(false)).unwrap();
            let analysis = ItemAnalysis {
                parsed: ParsedItem {
                    item_class: Some("Gloves".into()), rarity_label: Some("Rare".into()), rarity: Some(Rarity::Rare), name: Some("Doom Grip".into()),
                    base_type: Some("Evasion Gloves".into()), item_level: Some(81), corrupted: false, advanced: true, mods: vec![],
                },
                base_id: Some("gloves_dex".into()),
                detail: Some(detail(&ctx.bp.pool, &it)),
                unmatched: vec!["+12 to Level of all Minion Skills".into()],
                error: None,
            };
            w("capture.json", &serde_json::json!({ "analysis": analysis, "advice": advice, "adviceError": null, "raw": "Item Class: Gloves\nRarity: Rare\nDoom Grip\nEvasion Gloves\n--------\nItem Level: 81" }));
            println!("fixtures → {}", dir.display());
        }
        Some("parse") => {
            let mut text = String::new();
            match args.get(1).map(|s| s.as_str()) {
                Some("-") | None => {
                    std::io::stdin().read_to_string(&mut text).unwrap();
                }
                Some(f) => text = std::fs::read_to_string(f).unwrap(),
            }
            let a = analyze_item(&ds, &text, args.get(2).map(|s| s.as_str()), 80);
            println!("{}", serde_json::to_string_pretty(&a).unwrap());
        }
        _ => usage(),
    }
}

mod erased {
    pub trait Ser {
        fn json(&self) -> String;
    }
    impl<T: serde::Serialize> Ser for T {
        fn json(&self) -> String {
            serde_json::to_string(self).unwrap()
        }
    }
}
