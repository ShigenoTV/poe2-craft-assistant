import type { VerifyResult } from "./types";

/**
 * Probabilité de finir le craft en dépensant au plus `budget` (HORS base neuve et instillation), en suivant
 * le plan. Même calcul que `VerifyResult::success_probability` côté Rust : palier inférieur des quantiles
 * simulés (jamais interpolé), essais interrompus comptés comme des échecs.
 */
export function successProbability(mc: VerifyResult, budget: number): number | null {
  const q = mc.costQuantiles;
  if (!q || q.length < 2) return null;
  const b = budget + Math.abs(budget) * 1e-6; // coûts simulés stockés en f32
  if (b < q[0]) return 0;
  let i = 0;
  while (i + 1 < q.length && q[i + 1] <= b) i++;
  return (i / (q.length - 1)) * (mc.trials / (mc.trials + mc.censored));
}

/** Plus petit budget (hors base neuve) qui donne au moins `p` de chances de réussir ; `null` si hors d'atteinte. */
export function budgetFor(mc: VerifyResult, p: number): number | null {
  const q = mc.costQuantiles;
  if (!q || q.length < 2) return null;
  const done = mc.trials / (mc.trials + mc.censored);
  const i = Math.ceil((p / done) * (q.length - 1) - 1e-9);
  return i <= q.length - 1 ? q[Math.max(0, i)] : null;
}
