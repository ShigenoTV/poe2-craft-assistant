import { useState } from "react";
import type { InstillView } from "@/lib/types";
import { Cost } from "@/lib/display";

/** Instillation d'amulette (The Withered Willow) : trois Liquid Emotions, dans l'ordre, ajoutent un passif
 * à l'amulette une fois l'objectif atteint. Coût fixe, ajouté au plan. */
export function InstillPicker({ instills, prices, value, onChange }: {
  instills: InstillView[]; prices: Record<string, number>; value: number | null; onChange: (skill: number | null) => void;
}) {
  const [q, setQ] = useState("");
  const recipeCost = (i: InstillView) => i.emotionIds.reduce((s, e) => s + (prices[e] ?? 0), 0);
  const chosen = instills.find((i) => i.skill === value);
  const qq = q.trim().toLowerCase();
  const filtered = qq.length < 2 ? [] : instills.filter((i) => i.name.toLowerCase().includes(qq) || i.stats.some((s) => s.toLowerCase().includes(qq))).slice(0, 40);
  return (
    <div className="stack" style={{ gap: 8 }}>
      <h3 className="hd">Instillation (facultatif)</h3>
      {chosen ? (
        <div className="want">
          <div className="want-top">
            <b className="grow">{chosen.name}</b>
            <span className="muted small"><Cost ex={recipeCost(chosen)} /></span>
            <button className="btn ghost sm" onClick={() => onChange(null)} aria-label={`Retirer ${chosen.name}`}>Retirer</button>
          </div>
          {chosen.stats.map((s) => <div key={s} className="small">{s}</div>)}
          <div className="small muted">Dans l'ordre : {chosen.emotions.join(" → ")}</div>
        </div>
      ) : (
        <>
          <p className="muted small">Un passif notable ajouté à l'amulette une fois les affixes obtenus, avec trois Liquid Emotions.</p>
          <input type="search" placeholder="Chercher un passif notable…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Rechercher un passif à instiller" />
          {qq.length >= 2 && (
            <div className="addlist">
              {filtered.length === 0 && <div className="muted small" style={{ padding: 10 }}>Aucun passif ne correspond.</div>}
              {filtered.map((i) => (
                <button key={i.skill} className="addrow" title={`${i.stats.join("\n")}\n${i.emotions.join(" → ")}`} onClick={() => { onChange(i.skill); setQ(""); }}>
                  <span>{i.name}</span>
                  <span className="share"><Cost ex={recipeCost(i)} /></span>
                </button>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
