import type { ItemCaptured } from "@/lib/types";
import { cost, num, pct } from "@/lib/format";

/** Conseil du solveur pour l'objet capturé (utilisé dans l'app et dans l'overlay).
 * `compact` : mode overlay en jeu — la prochaine étape prend toute la place, le reste est replié. */
export function AdviceView({ cap, compact = false }: { cap: ItemCaptured; compact?: boolean }) {
  const r = cap.advice;
  if (cap.adviceError) return <div className="ov-warn">{cap.adviceError}</div>;
  if (!r) return <p className="muted small">Aucun plan actif : pas de conseil. Calcule un plan dans l'application.</p>;
  if (r.dead) return <div className="ov-warn">Objet inutilisable pour ce plan : il porte un affixe fracturé qui n'est pas voulu. Repars d'une base neuve.</div>;
  const a = r.advice;
  const status = { held: "✓", blocked: "✕", missing: "○" } as const;

  const details = (
    <>
      <div className="ov-goal">
        {r.goal.map((g, k) => (
          <div key={g.label} style={{ color: r.wantedStatus[k] === "held" ? "var(--ok)" : r.wantedStatus[k] === "blocked" ? "var(--bad)" : "var(--muted)" }}>
            <span style={{ width: 14 }}>{status[r.wantedStatus[k]]}</span><span>{g.label}</span>
            {r.wantedStatus[k] === "blocked" && <span className="small">tier trop bas</span>}
          </div>
        ))}
      </div>
      {a && a.outcomes.length > 0 && (
        <div style={{ marginTop: 8 }}>
          {a.outcomes.slice(0, compact ? 4 : 6).map((o, i) => <div key={i} className={`ov-out ${o.kind}`}><b>{pct(o.probability)}</b><span>{o.label}</span></div>)}
        </div>
      )}
    </>
  );

  return (
    <div className="stack" style={{ gap: 10 }}>
      {a?.goalReached && (
        <div className="ov-next ov-next-big">
          <div className="act" style={{ color: "var(--ok)" }}>✓ Objectif atteint</div>
        </div>
      )}
      {a && !a.goalReached && a.action && (
        <div className="ov-next ov-next-big">
          <div className="small muted">Prochaine étape</div>
          <div className="act">{a.action.isAbandon ? "Abandonner cet objet" : a.action.label}</div>
          <div className="small muted" style={{ marginTop: 2 }}>
            {a.costToGo !== null && <>Coût restant espéré {cost(a.costToGo)}</>}
            {a.repeat && a.repeat.expectedAttempts > 1.15 && <> · à répéter jusqu'à changement (~{num(a.repeat.expectedAttempts, 1)} fois)</>}
          </div>
        </div>
      )}
      {a && !a.action && !a.goalReached && <div className="ov-warn">Aucune action utile depuis cet état avec les monnaies autorisées.</div>}
      {compact ? (
        <details className="ov-details">
          <summary className="small muted">Détails (objectif, probabilités)</summary>
          <div style={{ marginTop: 6 }}>{details}</div>
        </details>
      ) : details}
    </div>
  );
}
