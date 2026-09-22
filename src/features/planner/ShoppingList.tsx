import type { CraftPlan } from "@/lib/types";
import { cost, num } from "@/lib/format";

export function ShoppingList({ plan, unit }: { plan: CraftPlan; unit: string }) {
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
              <td className="n muted">{cost(l.unitCost, unit)}</td>
              <td className="n">{cost(l.expectedCost, unit)}</td>
              <td><div className="bar"><i style={{ width: `${(100 * l.expectedCost) / max}%` }} /></div></td>
            </tr>
          ))}
          <tr><td colSpan={3}><b>Total moyen</b></td><td className="n"><b>{cost(total, unit)}</b></td><td /></tr>
        </tbody>
      </table>
      <p className="muted small" style={{ padding: "10px 12px" }}>
        Ce sont des moyennes : un craft réel coûte parfois beaucoup moins, parfois beaucoup plus. Le budget « 9 fois sur 10 » est indiqué en haut de l'écran.
      </p>
    </div>
  );
}
