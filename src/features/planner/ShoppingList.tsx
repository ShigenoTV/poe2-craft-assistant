import type { CraftPlan } from "@/lib/types";
import { num } from "@/lib/format";
import { Cost } from "@/lib/display";

export function ShoppingList({ plan }: { plan: CraftPlan }) {
  const total = plan.shopping.reduce((s, l) => s + l.expectedCost, 0);
  const max = Math.max(...plan.shopping.map((l) => l.expectedCost), 1);
  return (
    <div className="panel">
      <table className="t">
        <thead><tr><th>Monnaie</th><th className="n">Quantité moyenne</th><th className="n">Prix unitaire</th><th className="n">Coût moyen</th><th style={{ width: 140 }} /></tr></thead>
        <tbody>
          {plan.shopping.map((l) => (
            <tr key={l.id}>
              <td>{l.label}</td>
              <td className="n">{num(l.expectedCount, l.expectedCount < 10 ? 2 : 1)}</td>
              <td className="n muted"><Cost ex={l.unitCost} /></td>
              <td className="n"><Cost ex={l.expectedCost} /></td>
              <td><div className="bar"><i style={{ width: `${(100 * l.expectedCost) / max}%` }} /></div></td>
            </tr>
          ))}
          <tr><td colSpan={3}><b>Total moyen</b></td><td className="n"><b><Cost ex={total} /></b></td><td /></tr>
        </tbody>
      </table>
      <p className="muted small" style={{ padding: "10px 12px" }}>
        Ce sont des moyennes : un craft réel coûte parfois beaucoup moins, parfois beaucoup plus. Le budget « 9 fois sur 10 » est indiqué en haut de l'écran.
      </p>
    </div>
  );
}
