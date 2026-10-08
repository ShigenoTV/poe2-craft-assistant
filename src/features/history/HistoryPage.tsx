import { useEffect, useState } from "react";
import { api, listen } from "@/lib/ipc";
import { num } from "@/lib/format";
import { Cost, useCost } from "@/lib/display";
import { gapOf } from "@/lib/money";
import type { CraftRecord } from "@/lib/types";

const date = new Intl.DateTimeFormat("fr-FR", { dateStyle: "short", timeStyle: "short" });

function Gap({ planned, real }: { planned: number; real: number }) {
  const g = gapOf(planned, real);
  if (g === null) return <>—</>;
  const cls = g <= 0 ? "ok-t" : g < 0.25 ? "warn-t" : "bad-t";
  return <span className={cls} style={{ whiteSpace: "nowrap" }}>{`${g >= 0 ? "+" : "−"}${num(Math.abs(g) * 100, 0)}\u202f%`}</span>;
}

/** Historique des crafts suivis en direct : coût prévu par le plan contre coût réel des monnaies saisies. */
export function HistoryPage() {
  const [records, setRecords] = useState<CraftRecord[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fmt = useCost();

  useEffect(() => {
    const load = () => void api.historyList().then(setRecords, (e) => setError(String(e)));
    load();
    let off: (() => void) | undefined;
    void listen("history-updated", load).then((f) => (off = f));
    return () => off?.();
  }, []);

  const del = async (id: number | null) => {
    if (id === null && !window.confirm("Effacer tout l'historique des crafts ?")) return;
    try {
      await api.historyDelete(id);
      setRecords(await api.historyList());
    } catch (e) {
      setError(String(e));
    }
  };

  const list = records ?? [];
  const real = list.reduce((a, r) => a + r.realCost, 0);
  const won = list.filter((r) => r.success);
  const wins = won.length;
  // l'écart n'a de sens que sur un craft mené au bout : le prévu est le coût d'un craft complet
  const wonPlanned = won.reduce((a, r) => a + r.plannedCost, 0);
  const wonReal = won.reduce((a, r) => a + r.realCost, 0);

  return (
    <div className="page">
      <div className="page-head">
        <h1>Historique</h1>
        <p>Chaque craft suivi en direct dans l'overlay, une fois enregistré : ce que le plan prévoyait contre ce que les monnaies saisies ont vraiment coûté.</p>
      </div>
      {error && <div className="err" style={{ marginBottom: 12 }}>{error}</div>}
      {records && list.length === 0 && (
        <div className="panel"><div className="empty">
          Aucun craft enregistré. Active un plan, suis le craft dans l'overlay (chaque saisie compte la monnaie conseillée), puis clique sur « Enregistrer » à la fin.
        </div></div>
      )}
      {list.length > 0 && (
        <div className="stack" style={{ gap: 16 }}>
          <div className="ledger">
            <div><div className="k">Crafts</div><div className="v">{list.length}</div><div className="s">{wins} réussi{wins > 1 ? "s" : ""} · {list.length - wins} abandonné{list.length - wins > 1 ? "s" : ""} · {fmt(real)} dépensés en tout</div></div>
            <div><div className="k">Prévu (crafts réussis)</div><div className="v">{wins > 0 ? <Cost ex={wonPlanned} /> : "—"}</div><div className="s">somme des coûts espérés</div></div>
            <div><div className="k">Réel (crafts réussis)</div><div className="v">{wins > 0 ? <Cost ex={wonReal} /> : "—"}</div><div className="s">somme des monnaies comptées</div></div>
            <div><div className="k">Écart</div><div className="v">{wins > 0 ? <Gap planned={wonPlanned} real={wonReal} /> : "—"}</div><div className="s">{wins > 0 ? `réel contre prévu, sur ${wins === 1 ? "le craft réussi" : `les ${wins} crafts réussis`}` : "aucun craft réussi pour comparer"}</div></div>
          </div>
          <div className="panel">
            <table className="t">
              <thead>
                <tr><th>Date</th><th>Objet</th><th>Objectif</th><th>Résultat</th><th className="n">Prévu</th><th className="n">Réel</th><th className="n">Écart</th><th>Monnaies utilisées</th><th /></tr>
              </thead>
              <tbody>
                {list.map((r) => (
                  <tr key={r.id}>
                    <td className="small">{date.format(new Date(r.finishedAt * 1000))}</td>
                    <td><b>{r.baseName}</b><div className="small muted">niveau {r.ilvl}</div></td>
                    <td className="small">{r.goal.join(", ")}</td>
                    <td>{r.success ? <span className="ok-t">Réussi</span> : <span className="muted">Abandonné</span>}</td>
                    <td className="n"><Cost ex={r.plannedCost} /></td>
                    <td className="n"><Cost ex={r.realCost} /></td>
                    <td className="n">{r.success ? <Gap planned={r.plannedCost} real={r.realCost} /> : <span className="muted" title="Craft arrêté avant la fin : le prévu est celui d'un craft complet">—</span>}</td>
                    <td className="small">{r.uses.map((u) => `${u.count} × ${u.label} (${fmt(u.cost)})`).join(", ") || "—"}<div className="faint">{r.steps} saisie{r.steps > 1 ? "s" : ""}</div></td>
                    <td><button className="btn sm" title="Supprimer cette fiche" onClick={() => void del(r.id)}>✕</button></td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="row" style={{ padding: "10px 12px" }}>
              <p className="muted small grow" style={{ margin: 0 }}>
                Coûts hors première base, aux prix du plan au moment de chaque saisie. Un rachat de base (« Base neuve ») compte le prix de la base moins sa revente, comme le plan. Un craft abandonné compte dans le coût réel total mais pas dans l'écart, car son prévu est celui d'un craft complet.
              </p>
              <button className="btn sm" onClick={() => void del(null)}>Tout effacer</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
