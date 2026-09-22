import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "@/store";
import { api } from "@/lib/ipc";
import { GoalPicker } from "@/components/GoalPicker";
import { goalFamilies, ItemCard } from "@/components/ItemCard";
import { cost, num, pct, shortUnit } from "@/lib/format";
import type { ActionView, ItemDetail, SimResult, WantedReq } from "@/lib/types";

const blank = (ilvl: number): ItemDetail => ({ view: { rarity: "normal", ilvl, mods: [] }, mods: [] });

function applicable(a: ActionView, it: ItemDetail): boolean {
  const r = it.view.rarity, n = it.mods.length;
  switch (a.kind) {
    case "transmute": case "alchemy": return r === "normal";
    case "augment": return r === "magic" && n < 2;
    case "regal": return r === "magic";
    case "exalt": return r === "rare" && n < 6;
    case "chaos": return r === "rare" && it.mods.some((m) => !m.fractured);
    case "annul": return r !== "normal" && it.mods.some((m) => !m.fractured);
    case "fracture": return r === "rare" && n >= 4 && !it.mods.some((m) => m.fractured);
  }
}

export function SandboxPage() {
  const { info, pools, actions, ensurePool, prices } = useStore();
  const unit = shortUnit(info?.priceUnit);
  const [baseId, setBaseId] = useState(info?.bases[0]?.id ?? "");
  const [ilvl, setIlvl] = useState(81);
  const [item, setItem] = useState<ItemDetail>(blank(81));
  const [hist, setHist] = useState<ItemDetail[]>([]);
  const [log, setLog] = useState<{ label: string; ok: boolean; cost: number }[]>([]);
  const [wanted, setWanted] = useState<WantedReq[]>([]);
  const [simCur, setSimCur] = useState("exalt");
  const [trials, setTrials] = useState(200000);
  const [maxOrbs, setMaxOrbs] = useState(6);
  const [res, setRes] = useState<SimResult | null>(null);
  const [prog, setProg] = useState<number | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const pool = pools[baseId];
  const running = useRef(false);

  useEffect(() => { if (baseId) void ensurePool(baseId); }, [baseId, ensurePool]);
  const reset = (l = ilvl) => { setItem(blank(l)); setHist([]); setLog([]); };
  const spent = log.reduce((s, l) => s + l.cost, 0);

  const apply = async (a: ActionView) => {
    try {
      const r = await api.sandboxApply(baseId, item.view, a.id);
      if (r.applied) setHist((h) => [...h, item]);
      setItem(r.item);
      setLog((l) => [{ label: a.label, ok: r.applied, cost: r.cost }, ...l]);
    } catch (e) { setErr(String(e)); }
  };
  const undo = () => { const last = hist[hist.length - 1]; if (!last) return; setItem(last); setHist(hist.slice(0, -1)); setLog((l) => { const i = l.findIndex((x) => x.ok); return i < 0 ? l : l.filter((_, k) => k !== i); }); };

  const plain = actions.filter((a) => !a.addSlot && !a.removeSlot);
  const omens = actions.filter((a) => a.addSlot || a.removeSlot);
  const fam = useMemo(() => (pool ? goalFamilies(wanted.map((w) => ({ label: "", slot: "prefix" as const, group: pool.groups.find((g) => g.key === w.group)?.group ?? -1, maxTier: w.maxTier })), pool.groups) : new Set<string>()), [wanted, pool]);

  const run = async () => {
    if (!pool || wanted.length === 0) return;
    setErr(null); setRes(null); setProg(0); running.current = true;
    try {
      const r = await api.runSimulation(
        { baseId, ilvl, start: item.view, wanted, currencyId: simCur, maxOrbs, trials, seed: Math.floor(Math.random() * 2 ** 31), baseCost: prices["base_white"] ?? 0 },
        (p) => setProg(p.total ? p.done / p.total : 0),
      );
      setRes(r);
    } catch (e) { setErr(String(e)); } finally { setProg(null); running.current = false; }
  };

  return (
    <div className="page">
      <div className="page-head"><h1>Simulateur</h1><p>Applique des monnaies une à une sur un objet, ou mesure la probabilité d'obtenir tes affixes en répétant une monnaie.</p></div>
      <div className="grid2">
        <div className="stack">
          <div className="panel pad row">
            <label className="f grow">Base
              <select value={baseId} onChange={(e) => { setBaseId(e.target.value); setWanted([]); reset(); }}>
                {info?.bases.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
              </select>
            </label>
            <label className="f" style={{ width: 90 }}>Niveau d'objet
              <input type="number" min={1} max={100} value={ilvl} onChange={(e) => { const v = Math.max(1, Math.min(100, +e.target.value || 1)); setIlvl(v); reset(v); }} />
            </label>
          </div>
          <ItemCard item={item} title={pool ? pool.base.name : ""} goal={{ families: fam }} />
          <div className="panel pad stack" style={{ gap: 10 }}>
            <div className="row"><h3 className="hd">Monnaies</h3>
              <span className="muted small right">Dépensé : <b style={{ color: "var(--rare)" }}>{cost(spent, unit)}</b> · {log.filter((l) => l.ok).length} applications</span></div>
            <div className="cur-grid">
              {plain.map((a) => (
                <button key={a.id} className="cur" disabled={!applicable(a, item)} onClick={() => void apply(a)}>
                  {a.label}<small>{cost(a.unitCost, unit)}{a.minModLevel > 0 && ` · mod niv. ${a.minModLevel}+`}</small>
                </button>
              ))}
            </div>
            <details className="box"><summary>Avec un Omen ({omens.length})</summary>
              <div className="cur-grid" style={{ paddingTop: 8 }}>
                {omens.map((a) => (
                  <button key={a.id} className="cur" disabled={!applicable(a, item)} onClick={() => void apply(a)}>
                    {a.label}<small>{cost(a.unitCost, unit)}</small>
                  </button>
                ))}
              </div>
            </details>
            <div className="row">
              <button className="btn" onClick={undo} disabled={hist.length === 0}>Annuler la dernière</button>
              <button className="btn" onClick={() => reset()}>Nouvelle base</button>
            </div>
            {log.length > 0 && <div className="log">{log.map((l, i) => <div key={i}><b>{l.label}</b> {l.ok ? `− ${cost(l.cost, unit)}` : "— sans effet (conditions non remplies)"}</div>)}</div>}
          </div>
        </div>

        <div className="stack">
          <div className="panel pad stack">
            {pool && <GoalPicker pool={pool} wanted={wanted} onChange={setWanted} />}
          </div>
          <div className="panel pad stack" style={{ gap: 10 }}>
            <h3 className="hd">Probabilité par répétition</h3>
            <p className="muted small">Répète la monnaie choisie sur l'objet ci-dessus (à partir de son état actuel) jusqu'à obtenir l'objectif, dans la limite indiquée.</p>
            <div className="row">
              <label className="f grow">Monnaie
                <select value={simCur} onChange={(e) => setSimCur(e.target.value)}>{actions.map((a) => <option key={a.id} value={a.id}>{a.label}</option>)}</select>
              </label>
              <label className="f" style={{ width: 96 }}>Répétitions max
                <input type="number" min={1} max={50} value={maxOrbs} onChange={(e) => setMaxOrbs(Math.max(1, +e.target.value || 1))} />
              </label>
              <label className="f" style={{ width: 130 }}>Essais
                <select value={trials} onChange={(e) => setTrials(+e.target.value)}><option value={20000}>20 000</option><option value={200000}>200 000</option><option value={2000000}>2 000 000</option></select>
              </label>
            </div>
            <div className="row">
              <button className="btn primary" disabled={prog !== null || wanted.length === 0} onClick={() => void run()}>{prog !== null ? "Simulation…" : "Lancer la simulation"}</button>
              {prog !== null && <button className="btn" onClick={() => void api.cancelJob()}>Annuler</button>}
            </div>
            {prog !== null && <div className={`progress ${prog === 0 ? "ind" : ""}`}><i style={{ width: `${prog * 100}%` }} /></div>}
            {err && <div className="err">{err}</div>}
            {res && (
              <table className="t"><tbody>
                <tr><td>Probabilité d'obtenir l'objectif</td><td className="n"><b>{pct(res.pHat)}</b> <span className="muted small">[{pct(res.ci95[0])} ; {pct(res.ci95[1])}]</span></td></tr>
                <tr><td>Monnaies utilisées en moyenne (toutes tentatives)</td><td className="n">{num(res.meanOrbsAll, 2)}</td></tr>
                <tr><td>… quand ça réussit</td><td className="n">{res.meanOrbsOnSuccess === null ? "—" : num(res.meanOrbsOnSuccess, 2)}</td></tr>
                <tr><td>Coût moyen par réussite (bases comprises)</td><td className="n">{res.costPerSuccess === null ? "—" : cost(res.costPerSuccess, unit)}</td></tr>
                <tr><td className="muted small" colSpan={2}>{num(res.trials, 0)} essais · intervalle de confiance à 95 %</td></tr>
              </tbody></table>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
