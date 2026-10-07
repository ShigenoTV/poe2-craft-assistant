import { useStore } from "@/store";
import type { ComparedPath, CraftPlan } from "@/lib/types";
import { cost, num } from "@/lib/format";

/** Indice du chemin qui minimise `f` parmi ceux vérifiés sur le moteur exact (`-1` s'il y en a moins de deux). */
function best(paths: ComparedPath[], f: (p: ComparedPath) => number | undefined): number {
  let bi = -1, bv = Infinity, n = 0;
  paths.forEach((p, i) => {
    const v = f(p);
    if (v === undefined || !Number.isFinite(v)) return;
    n++;
    if (v < bv) { bv = v; bi = i; }
  });
  return n >= 2 ? bi : -1;
}

export function ComparePaths({ plan, unit }: { plan: CraftPlan; unit: string }) {
  const { comparison, comparing, compareProgress, compareError, compare, usePath, cancel, solving } = useStore();
  // base neuve + instillation éventuelle : coûts fixes identiques pour tous les chemins
  const b = plan.baseCost + (plan.instill?.cost ?? 0);
  const paths = comparison ?? [];
  const cheapest = best(paths, (p) => p.mc?.meanCost);
  const steadiest = best(paths, (p) => p.mc?.stdDev);
  const safest = best(paths, (p) => p.mc?.p99Cost);
  const tag = (i: number) => [i === cheapest && "le moins cher en moyenne", i === steadiest && "le plus régulier", i === safest && "pire cas le plus bas"].filter(Boolean).join(" · ");
  const pct01 = compareProgress && compareProgress.total > 0 ? compareProgress.done / compareProgress.total : 0;

  return (
    <div className="panel">
      <div className="pad stack" style={{ gap: 8 }}>
        <p className="small" style={{ margin: 0 }}>
          Compare le plan optimal aux meilleurs plans qui se passent d'une famille de monnaies qu'il utilise (sans Chaos, sans Essences, sans Omens…).
          Chaque chemin est rejoué sur le moteur exact : coût moyen, médiane, pire cas (99 crafts sur 100 coûtent moins) et écart-type, qui mesure la régularité.
        </p>
        <div className="row">
          <button className="btn primary" disabled={comparing || solving} onClick={() => void compare()}>{comparison ? "Recalculer la comparaison" : "Comparer les chemins"}</button>
          {comparing && <button className="btn" onClick={() => void cancel()}>Annuler</button>}
        </div>
        {comparing && (
          <div>
            <div className={`progress ${pct01 === 0 ? "ind" : ""}`}><i style={{ width: `${pct01 * 100}%` }} /></div>
            <div className="small muted" style={{ marginTop: 4 }}>
              {compareProgress?.stage === "verifying" ? `Vérification sur le moteur exact : ${num(compareProgress.done, 0)} / ${num(compareProgress.total, 0)}` : "Résolution des chemins alternatifs…"}
            </div>
          </div>
        )}
        {compareError && <div className="err">{compareError}</div>}
      </div>
      {comparison && (
        <>
          <table className="t">
            <thead>
              <tr>
                <th>Chemin</th><th className="n">Coût moyen</th><th className="n">Une fois sur deux</th><th className="n">Pire cas (99 sur 100)</th><th className="n">Écart-type</th><th>Monnaies principales</th><th />
              </tr>
            </thead>
            <tbody>
              {paths.map((p, i) => (
                <tr key={p.label}>
                  <td>
                    <b>{p.label}</b>
                    {tag(i) && <div className="small ok-t">{tag(i)}</div>}
                    {!p.converged && <div className="small warn-t">résultat approché</div>}
                  </td>
                  <td className="n">{cost((p.mc?.meanCost ?? p.expectedCost) + b, unit)}</td>
                  <td className="n">{p.mc ? cost(p.mc.medianCost + b, unit) : "—"}</td>
                  <td className="n">{p.mc ? cost(p.mc.p99Cost + b, unit) : "—"}</td>
                  <td className="n">{p.mc?.stdDev !== undefined ? cost(p.mc.stdDev, unit) : "—"}</td>
                  <td className="small">{p.mainCurrencies.map((l) => `${l.label} (${cost(l.expectedCost, unit)})`).join(", ")}</td>
                  <td>{p.excluded && <button className="btn sm" disabled={solving || comparing} onClick={() => void usePath(p)}>Suivre ce chemin</button>}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="muted small" style={{ padding: "10px 12px" }}>
            {paths.length < 2
              ? "Aucune alternative réaliste : sans l'une de ses monnaies, l'objectif devient inatteignable ou le chemin redevient identique."
              : `Coûts base neuve${plan.instill ? " et instillation" : ""} comprise${plan.instill ? "s" : ""}. « Suivre ce chemin » retire ces monnaies des monnaies autorisées et recalcule le plan.`}
          </p>
        </>
      )}
    </div>
  );
}
