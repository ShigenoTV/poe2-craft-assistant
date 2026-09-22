import { useMemo, useState } from "react";
import type { GroupInfo, PoolView, WantedReq } from "@/lib/types";
import { pct, prettyText } from "@/lib/format";

const MAX_WANTED = 6;

function TierBar({ g, maxTier, onChange }: { g: GroupInfo; maxTier: number; onChange: (t: number) => void }) {
  const maxW = Math.max(...g.tiers.map((t) => t.weight));
  return (
    <div className="tierbar" role="group" aria-label={`Tier minimum pour ${g.family}`}>
      {g.tiers.map((t) => (
        <button
          key={t.tier} type="button" className={t.tier <= maxTier ? "on" : ""} onClick={() => onChange(t.tier)}
          title={`T${t.tier} · niveau ${t.level} · poids ${t.weight}\n${prettyText(t.text)}`} aria-pressed={t.tier <= maxTier}
        >
          <i style={{ height: `${25 + (75 * t.weight) / maxW}%` }} />
          <span>{t.tier}</span>
        </button>
      ))}
    </div>
  );
}

interface Props { pool: PoolView; wanted: WantedReq[]; onChange: (w: WantedReq[]) => void }

export function GoalPicker({ pool, wanted, onChange }: Props) {
  const [q, setQ] = useState("");
  const byKey = useMemo(() => new Map(pool.groups.map((g) => [g.key, g])), [pool]);
  const slotTotal = useMemo(() => {
    const t = { prefix: 0, suffix: 0 };
    pool.groups.forEach((g) => (t[g.slot] += g.totalWeight));
    return t;
  }, [pool]);
  const sel = wanted.map((w) => ({ w, g: byKey.get(w.group) })).filter((x): x is { w: WantedReq; g: GroupInfo } => !!x.g);
  const nP = sel.filter((s) => s.g.slot === "prefix").length;
  const nS = sel.length - nP;
  const chosen = new Set(wanted.map((w) => w.group));
  const canAdd = (g: GroupInfo) => sel.length < MAX_WANTED && (g.slot === "prefix" ? nP < 3 : nS < 3);
  const filtered = pool.groups.filter((g) => !chosen.has(g.key) && g.family.toLowerCase().includes(q.trim().toLowerCase()));

  return (
    <div className="stack" style={{ gap: 10 }}>
      <div className="row">
        <h3 className="hd">Affixes voulus</h3>
        <span className="muted small right">{sel.length}/{MAX_WANTED} · préfixes {nP}/3 · suffixes {nS}/3</span>
      </div>
      {sel.length === 0 && <p className="note small">Choisis les affixes que l'objet final doit porter. Le tier indiqué est le minimum accepté : « T3 » accepte T1, T2 et T3.</p>}
      {sel.map(({ w, g }) => {
        const accepted = g.tiers.filter((t) => t.tier <= w.maxTier).reduce((s, t) => s + t.weight, 0);
        const cur = g.tiers.find((t) => t.tier === w.maxTier)!;
        return (
          <div className="want" key={g.key}>
            <div className="want-top">
              <b className="grow">{g.family}</b>
              <span className={`pill ${g.slot}`}>{g.slot === "prefix" ? "préfixe" : "suffixe"}</span>
              <button className="btn ghost sm" onClick={() => onChange(wanted.filter((x) => x.group !== g.key))} aria-label={`Retirer ${g.family}`}>Retirer</button>
            </div>
            <TierBar g={g} maxTier={w.maxTier} onChange={(t) => onChange(wanted.map((x) => (x.group === g.key ? { ...x, maxTier: t } : x)))} />
            <div className="small muted">
              T{cur.tier} minimum : {prettyText(cur.text)} · {pct(accepted / g.totalWeight)} des tirages du groupe
            </div>
          </div>
        );
      })}
      <input type="search" placeholder="Ajouter un affixe…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Rechercher un affixe" />
      <div className="addlist">
        {filtered.length === 0 && <div className="muted small" style={{ padding: 10 }}>Aucun affixe ne correspond.</div>}
        {filtered.map((g) => (
          <button key={g.key} className="addrow" disabled={!canAdd(g)} onClick={() => onChange([...wanted, { group: g.key, maxTier: Math.min(3, g.tiers.length) }])}>
            <span className={`pill ${g.slot}`}>{g.slot === "prefix" ? "P" : "S"}</span>
            <span>{g.family}</span>
            <span className="share" title="Part du poids total du slot">{pct(g.totalWeight / slotTotal[g.slot])}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
