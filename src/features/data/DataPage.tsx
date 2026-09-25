import { useEffect, useState } from "react";
import { useStore } from "@/store";
import { pct, prettyText } from "@/lib/format";

export function DataPage() {
  const { info, pools, ensurePool } = useStore();
  const [baseId, setBaseId] = useState(info?.bases[0]?.id ?? "");
  const [q, setQ] = useState("");
  const pool = pools[baseId];
  useEffect(() => { if (baseId) void ensurePool(baseId); }, [baseId, ensurePool]);
  if (!info) return null;

  const qq = q.toLowerCase();
  const groups = (pool?.groups ?? []).filter((g) => g.family.toLowerCase().includes(qq) || g.tiers.some((t) => t.text.toLowerCase().includes(qq)));

  return (
    <div className="page">
      <div className="page-head"><h1>Données</h1><p>Affixes, tiers et poids utilisés par tous les calculs.</p></div>
      <div className="panel pad stack" style={{ marginBottom: 16 }}>
        <div className="row">
          <div className="grow">
            <b>{info.source}</b> <span className="muted">· {info.modCount} modificateurs · version du jeu {info.gameVersion || "?"} · générées le {info.generatedAt}</span>
            {info.notice && <p className="note small" style={{ marginTop: 6 }}>{info.notice}</p>}
          </div>
        </div>
      </div>
      <div className="row" style={{ marginBottom: 12 }}>
        <label className="f" style={{ width: 300 }}>Base
          <select value={baseId} onChange={(e) => setBaseId(e.target.value)}>{info.bases.map((b) => <option key={b.id} value={b.id}>{b.name} ({b.itemClass})</option>)}</select>
        </label>
        <label className="f grow" style={{ maxWidth: 320 }}>Filtrer<input type="search" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Nom d'affixe ou texte d'un mod…" /></label>
      </div>
      <div className="panel">
        {groups.map((g) => (
          <details key={g.key} className="box" style={{ border: 0, borderBottom: "1px solid var(--line-soft)", borderRadius: 0, background: "none" }}>
            <summary className="row" style={{ display: "flex" }}>
              <span className={`pill ${g.slot}`}>{g.slot === "prefix" ? "préfixe" : "suffixe"}</span>&nbsp;{g.family}
              <span className="muted small right">{g.tiers.length} tiers · poids {g.totalWeight}</span>
            </summary>
            <div style={{ padding: 0 }}>
              {g.tiers.map((t) => (
                <div key={t.tier} className="tierrow">
                  <span className="muted">T{t.tier}</span><span className="muted">niv. {t.level}</span>
                  <span style={{ color: "#b6c4ff" }}>{prettyText(t.text)}</span>
                  <span className="muted" style={{ textAlign: "right" }}>{t.weight}</span>
                  <span className="muted" style={{ textAlign: "right" }}>{pct(t.weight / g.totalWeight)}</span>
                </div>
              ))}
            </div>
          </details>
        ))}
      </div>
    </div>
  );
}
