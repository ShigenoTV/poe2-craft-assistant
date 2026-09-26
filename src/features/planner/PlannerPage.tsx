import { useEffect, useMemo, useState } from "react";
import { useStore } from "@/store";
import { GoalPicker } from "@/components/GoalPicker";
import { cost, num, pct, shortUnit } from "@/lib/format";
import { PlanGraph } from "./PlanGraph";
import { ShoppingList } from "./ShoppingList";
import type { ActionView } from "@/lib/types";

const KIND_ORDER = ["transmute", "augment", "regal", "alchemy", "exalt", "chaos", "annul", "fracture"];
const KIND_LABEL: Record<string, string> = { transmute: "Transmutation", augment: "Augmentation", regal: "Regal", alchemy: "Alchimie", exalt: "Exaltation", chaos: "Chaos", annul: "Annulation", fracture: "Fracture" };

function ActionsPicker() {
  const { actions, enabled, setPlanner, info } = useStore();
  const unit = shortUnit(info?.priceUnit);
  const on = new Set(enabled);
  const byKind = useMemo(() => {
    const m = new Map<string, ActionView[]>();
    actions.forEach((a) => m.set(a.kind, [...(m.get(a.kind) ?? []), a]));
    return KIND_ORDER.filter((k) => m.has(k)).map((k) => [k, m.get(k)!] as const);
  }, [actions]);
  const toggle = (id: string) => setPlanner({ enabled: on.has(id) ? enabled.filter((x) => x !== id) : [...enabled, id] });
  return (
    <details className="box">
      <summary>Monnaies autorisées ({enabled.length}/{actions.length})</summary>
      <div className="stack" style={{ gap: 10 }}>
        <div className="row">
          <button className="btn sm" onClick={() => setPlanner({ enabled: actions.filter((a) => a.defaultEnabled).map((a) => a.id) })}>Par défaut</button>
          <button className="btn sm" onClick={() => setPlanner({ enabled: actions.map((a) => a.id) })}>Tout</button>
          <button className="btn sm" onClick={() => setPlanner({ enabled: actions.filter((a) => !a.addSlot && !a.removeSlot && a.id.split("+").length === 1 && !/greater|perfect/.test(a.id)).map((a) => a.id) })}>Sans Omens ni Greater/Perfect</button>
        </div>
        {byKind.map(([k, list]) => (
          <div key={k}>
            <div className="small muted" style={{ marginBottom: 3 }}>{KIND_LABEL[k]}</div>
            {list.map((a) => (
              <label key={a.id} className="row small" style={{ padding: "2px 0" }}>
                <input type="checkbox" checked={on.has(a.id)} onChange={() => toggle(a.id)} />
                <span className="grow">{a.label}</span><span className="muted">{cost(a.unitCost, unit)}</span>
              </label>
            ))}
          </div>
        ))}
      </div>
    </details>
  );
}

function Ledger() {
  const { plan, info } = useStore();
  if (!plan) return null;
  const unit = shortUnit(info?.priceUnit);
  const b = plan.baseCost;
  const mc = plan.mc;
  const gap = mc ? mc.meanCost / plan.expectedCost - 1 : null;
  const gapCls = gap === null ? "" : Math.abs(gap) < 0.05 ? "ok-t" : Math.abs(gap) < 0.1 ? "warn-t" : "bad-t";
  return (
    <div className="ledger">
      <div><div className="k">Coût moyen</div><div className="v">{cost(plan.expectedCost + b, unit)}</div><div className="s">base neuve comprise</div></div>
      <div><div className="k">Une fois sur deux</div><div className="v">{mc ? cost(mc.medianCost + b, unit) : "—"}</div><div className="s">médiane simulée</div></div>
      <div><div className="k">Budget sûr (9 sur 10)</div><div className="v">{mc ? cost(mc.p90Cost + b, unit) : "—"}</div><div className="s">{mc ? `99 sur 100 : ${cost(mc.p99Cost + b, unit)}` : ""}</div></div>
      <div>
        <div className="k">Vérification moteur exact</div>
        <div className={`v ${gapCls}`}>{gap === null ? "—" : `${gap >= 0 ? "+" : "−"}${num(Math.abs(gap) * 100, 1)} %`}</div>
        <div className="s">{mc ? `${num(mc.trials, 0)} essais · ${plan.solver.states} états · ${plan.solver.millis} ms` : ""}</div>
      </div>
    </div>
  );
}

function StartingItemPicker() {
  const { startingItem, startingItemAnalysis, startingItemError, analyzingStartingItem, analyzeStartingItem, clearStartingItem } = useStore();
  const [text, setText] = useState("");
  return (
    <details className="box">
      <summary>Objet de départ {startingItem ? "(objet existant)" : "(base neuve)"}</summary>
      <div className="stack" style={{ gap: 8 }}>
        <p className="muted small">
          Colle le texte d'un objet déjà en ta possession (Ctrl+Alt+C en jeu) pour que le plan reparte de
          là où tu en es, au lieu d'une base neuve. Laisse vide pour repartir d'une base neuve, comme avant.
        </p>
        {startingItem ? (
          <div className="stack" style={{ gap: 6 }}>
            <div className="small">
              {startingItemAnalysis?.detail?.mods.length ?? 0} mod(s) reconnu(s), objet {startingItem.rarity === "rare" ? "Rare" : startingItem.rarity === "magic" ? "Magique" : "Normal"}.
            </div>
            {startingItemAnalysis?.detail?.mods.map((m) => (
              <div key={m.affixIdx} className="mod" style={{ padding: "2px 0" }}>
                <span className="tier">T{m.tier}</span><span className="tx">{m.text}</span>
              </div>
            ))}
            <button className="btn sm" onClick={() => { clearStartingItem(); setText(""); }}>Repartir d'une base neuve</button>
          </div>
        ) : (
          <>
            <textarea rows={5} value={text} onChange={(e) => setText(e.target.value)} placeholder="Colle ici le texte de l'objet…" />
            <div className="row">
              <button className="btn sm" disabled={!text.trim() || analyzingStartingItem} onClick={() => void analyzeStartingItem(text)}>
                {analyzingStartingItem ? "Analyse…" : "Analyser"}
              </button>
            </div>
            {startingItemError && <div className="err small">{startingItemError}</div>}
          </>
        )}
      </div>
    </details>
  );
}

export function PlannerPage() {
  const s = useStore();
  const { info, pools, baseId, ilvl, wanted, setPlanner, plan, solving, progress, solveError } = s;
  const [tab, setTab] = useState<"graph" | "shop" | "goal">("graph");
  const pool = pools[baseId];
  useEffect(() => { if (baseId) void s.ensurePool(baseId); }, [baseId]); // eslint-disable-line react-hooks/exhaustive-deps

  const pct01 = progress && progress.total > 0 ? progress.done / progress.total : 0;
  const stale = plan && (plan.baseId !== baseId);
  return (
    <div className="page">
      <div className="page-head">
        <h1>Reverse-crafting</h1>
        <p>Décris l'objet que tu veux : le solveur cherche la suite de monnaies au coût moyen minimal, avec ses embranchements.</p>
      </div>
      <div className="planner">
        <aside className="stack">
          <div className="panel pad stack">
            <div className="row">
              <label className="f grow">Base
                <select value={baseId} onChange={(e) => setPlanner({ baseId: e.target.value, wanted: [], startingItem: null })}>
                  {info?.bases.map((b) => <option key={b.id} value={b.id}>{b.name} ({b.itemClass})</option>)}
                </select>
              </label>
              <label className="f" style={{ width: 84 }}>Niveau d'objet
                <input type="number" min={1} max={100} value={ilvl} onChange={(e) => setPlanner({ ilvl: Math.max(1, Math.min(100, +e.target.value || 1)) })} />
              </label>
            </div>
            {pool ? <GoalPicker pool={pool} wanted={wanted} onChange={(w) => setPlanner({ wanted: w })} /> : <p className="muted">Chargement de la base…</p>}
          </div>
          <StartingItemPicker />
          <ActionsPicker />
          <div className="panel pad stack" style={{ gap: 10 }}>
            <label className="row small"><input type="checkbox" checked={s.activate} onChange={(e) => setPlanner({ activate: e.target.checked })} /> Utiliser ce plan dans l'overlay en jeu</label>
            <label className="f">Essais de vérification sur le moteur exact
              <select value={s.mcTrials} onChange={(e) => setPlanner({ mcTrials: +e.target.value })}>
                <option value={0}>Aucun (plus rapide)</option><option value={5000}>5 000</option><option value={20000}>20 000</option><option value={100000}>100 000 (précis)</option>
              </select>
            </label>
            <div className="row">
              <button className="btn primary grow" disabled={solving || wanted.length === 0 || !pool} onClick={() => void s.solve()}>
                {solving ? "Calcul en cours…" : "Calculer le plan"}
              </button>
              {solving && <button className="btn" onClick={() => void s.cancel()}>Annuler</button>}
            </div>
            {solving && (
              <div>
                <div className={`progress ${pct01 === 0 ? "ind" : ""}`}><i style={{ width: `${pct01 * 100}%` }} /></div>
                <div className="small muted" style={{ marginTop: 4 }}>{progress?.stage === "verifying" ? `Vérification sur le moteur exact : ${num(progress.done, 0)} / ${num(progress.total, 0)}` : "Résolution du plan optimal…"}</div>
              </div>
            )}
            {solveError && <div className="err">{solveError}</div>}
          </div>
        </aside>

        <section className="stack" style={{ minWidth: 0 }}>
          {!plan && !solving && (
            <div className="panel empty">
              <h3 className="hd" style={{ marginBottom: 6 }}>Aucun plan pour l'instant</h3>
              Ajoute de 1 à 6 affixes (3 préfixes et 3 suffixes au maximum) puis lance le calcul. Tu obtiens le coût moyen, un budget sûr, la liste d'achats et l'arbre des décisions à suivre selon les tirages.
            </div>
          )}
          {plan && (
            <>
              {stale && <div className="note">Ce plan concerne une autre base que celle sélectionnée.</div>}
              <Ledger />
              <div className="tabs" role="tablist">
                {([["graph", "Arbre de décision"], ["shop", "Liste de courses"], ["goal", "Objectif"]] as const).map(([k, l]) => (
                  <button key={k} role="tab" aria-selected={tab === k} onClick={() => setTab(k)}>{l}</button>
                ))}
              </div>
              {tab === "graph" && <PlanGraph plan={plan} />}
              {tab === "shop" && <ShoppingList plan={plan} unit={shortUnit(info?.priceUnit)} />}
              {tab === "goal" && (
                <div className="panel pad stack">
                  <div>{plan.goal.map((g) => <div key={g.label} className="row" style={{ padding: "3px 0" }}><span className={`pill ${g.slot}`}>{g.slot === "prefix" ? "préfixe" : "suffixe"}</span>{g.label}</div>)}</div>
                  <p className="muted small">
                    Prix : {plan.pricesSource}. Le coût de la première base ({cost(plan.baseCost, shortUnit(info?.priceUnit))}) est inclus en haut de l'écran ;
                    {plan.mc && ` chaque craft abandonné en coûte en moyenne ${num(plan.mc.meanAbandons, 2)} de plus.`}
                    {" "}Probabilité de réussir un craft sans jamais abandonner : voir l'arbre (chemin vert).
                    {plan.mc && plan.mc.censored > 0 && ` ${pct(plan.mc.censored / (plan.mc.trials + plan.mc.censored))} des essais ont été interrompus (trop longs) et sont exclus des statistiques.`}
                  </p>
                  <p className="muted small">Convergence du solveur : {plan.solver.converged ? "atteinte" : "non atteinte (résultat approché)"} en {plan.solver.sweeps} balayages. Contrôle d'intégrité : coût recalculé par les visites {num(plan.solver.costFromVisits, 2)} contre {num(plan.expectedCost, 2)}.</p>
                </div>
              )}
            </>
          )}
        </section>
      </div>
    </div>
  );
}
