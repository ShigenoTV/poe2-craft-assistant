import { useEffect, useMemo, useState } from "react";
import { api, listen } from "@/lib/ipc";
import { AdviceView } from "@/components/AdviceView";
import { prettyText, rarityLabel } from "@/lib/format";
import type { ActiveInfo, GroupInfo, LiveEdit, LiveView, PoolView, Rarity, Slot, TierInfo } from "@/lib/types";

const slotLabel: Record<Slot, string> = { prefix: "préfixe", suffix: "suffixe" };

/** Suivi de craft en direct : on saisit ce qu'on vient d'obtenir en jeu, le meilleur coup suivant est recalculé
 * à chaque saisie. Une copie d'objet en jeu (Ctrl+Alt+C) remplace l'objet suivi. */
export function LivePanel({ active, interactive, hotkey }: { active: ActiveInfo; interactive: boolean; hotkey: string }) {
  const [view, setView] = useState<LiveView | null>(null);
  const [pool, setPool] = useState<PoolView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [picker, setPicker] = useState<{ slot: Slot; replace?: number } | null>(null);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    void api.basePool(active.baseId).then(setPool);
    void api.liveState().then(setView);
    const offs: (() => void)[] = [];
    void listen<LiveView>("live-updated", setView).then((f) => offs.push(f));
    void listen<null>("plan-refreshed", () => void api.liveState().then(setView)).then((f) => offs.push(f));
    return () => offs.forEach((f) => f());
  }, [active.baseId, active.expectedCost]);

  const run = async (p: Promise<LiveView | null>) => {
    setBusy(true);
    setError(null);
    try {
      const v = await p;
      if (v) setView(v);
      setPicker(null);
      setFilter("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const edit = (...edits: LiveEdit[]) => void run(api.liveEdit(edits));

  const groups = pool?.groups ?? [];
  const wanted = (g: GroupInfo) => active.goal.some((x) => x.group === g.group && (x.familyId ?? 0) === (g.familyId ?? 0));
  const groupOf = (idx: number) => groups.find((g) => g.tiers.some((t) => t.affixIdx === idx));
  const item = view?.item;
  const ilvl = item?.view.ilvl ?? active.ilvl;
  const available = (g: GroupInfo) => g.tiers.filter((t) => t.level <= ilvl).sort((a, b) => a.tier - b.tier);

  const heldGroups = useMemo(() => new Set((item?.mods ?? []).map((m) => pool?.affixes[m.affixIdx]?.group)), [item, pool]);
  const candidates = useMemo(() => {
    if (!picker) return [];
    const from = picker.replace !== undefined ? pool?.affixes[picker.replace]?.group : undefined;
    const q = filter.trim().toLowerCase();
    return groups
      .filter((g) => g.slot === picker.slot && (!heldGroups.has(g.group) || g.group === from) && available(g).length > 0)
      .filter((g) => !q || g.family.toLowerCase().includes(q) || g.tiers.some((t) => t.text.toLowerCase().includes(q) || t.name.toLowerCase().includes(q)))
      .sort((a, b) => Number(wanted(b)) - Number(wanted(a)) || b.totalWeight - a.totalWeight);
  }, [picker, filter, groups, heldGroups, pool]);

  const choose = (t: TierInfo) => {
    if (!picker) return;
    if (picker.replace !== undefined) edit({ kind: "replace", from: picker.replace, to: t.affixIdx });
    else edit({ kind: "add", affixIdx: t.affixIdx });
  };
  /** Affixe non voulu le plus courant du slot : son identité ne change pas le conseil, seul le slot occupé compte. */
  const junk = () => {
    const g = candidates.find((g) => !wanted(g) && available(g).some((t) => !pool?.affixes[t.affixIdx]?.desecrated));
    const t = g && [...available(g)].filter((t) => !pool?.affixes[t.affixIdx]?.desecrated).sort((a, b) => b.weight - a.weight)[0];
    if (t) choose(t);
  };
  const stepTier = (idx: number, dir: -1 | 1) => {
    const g = groupOf(idx);
    const cur = pool?.affixes[idx];
    if (!g || !cur) return null;
    return available(g).find((t) => t.tier === cur.tier + dir) ?? null;
  };

  const rarity: Rarity = item?.view.rarity ?? "normal";
  const off = !interactive || busy;
  const count = (s: Slot) => item?.mods.filter((m) => m.slot === s).length ?? 0;
  const cap = (s: Slot) => (rarity === "normal" ? 0 : rarity === "magic" ? 1 : s === "prefix" ? pool?.base.maxPrefixes ?? 3 : pool?.base.maxSuffixes ?? 3);
  const hasFractured = item?.mods.some((m) => m.fractured) ?? false;

  return (
    <div className="stack" style={{ gap: 10 }}>
      <div className="lv-head">
        <span className="small muted grow">Suivi en direct{view && view.steps > 0 ? ` · ${view.steps} saisie${view.steps > 1 ? "s" : ""}` : ""}{busy ? " · calcul…" : ""}</span>
        <button className="btn sm" disabled={off || !view?.canUndo} title="Annuler la dernière saisie" onClick={() => void run(api.liveUndo())}>↶ Annuler</button>
        <button className="btn sm" disabled={off} title="Repartir d'une base neuve" onClick={() => void run(api.liveReset())}>Base neuve</button>
      </div>
      {!interactive && <p className="muted small" style={{ margin: 0 }}>Passe l'overlay en interactif (<span className="kbd">{hotkey}</span>) pour saisir ce que tu viens d'obtenir, ou copie l'objet en jeu.</p>}

      <div className="lv-rarity">
        {(["normal", "magic", "rare"] as Rarity[]).map((r) => (
          <button key={r} className={`btn sm ${r} ${rarity === r ? "on" : ""}`} disabled={off || rarity === r} onClick={() => edit({ kind: "rarity", rarity: r })}>{rarityLabel[r]}</button>
        ))}
        <span className="small muted grow" style={{ textAlign: "right" }}>P {count("prefix")}/{cap("prefix")} · S {count("suffix")}/{cap("suffix")}</span>
      </div>

      <div className="lv-mods">
        {item && item.mods.length === 0 && <p className="muted small" style={{ margin: 0 }}>Aucun affixe.</p>}
        {item?.mods.map((m) => {
          const better = stepTier(m.affixIdx, -1), worse = stepTier(m.affixIdx, 1);
          const g = groupOf(m.affixIdx);
          const goal = g && active.goal.find((x) => x.group === g.group && (x.familyId ?? 0) === (g.familyId ?? 0));
          const cls = goal ? (m.tier <= goal.maxTier ? "hit" : "block") : "";
          return (
            <div key={m.affixIdx} className={`lv-mod ${cls} ${m.fractured ? "fractured" : ""}`}>
              <span className="lv-slot">{m.slot === "prefix" ? "P" : "S"}</span>
              <span className="lv-tier">
                <button disabled={off || !better} title="Meilleur tier" onClick={() => better && edit({ kind: "replace", from: m.affixIdx, to: better.affixIdx })}>▲</button>
                <b>T{m.tier}</b>
                <button disabled={off || !worse} title="Tier plus bas" onClick={() => worse && edit({ kind: "replace", from: m.affixIdx, to: worse.affixIdx })}>▼</button>
              </span>
              <span className="tx">{prettyText(m.text)}{m.fractured && <span className="lock"> ◆</span>}</span>
              <span className="lv-acts">
                {rarity === "rare" && !hasFractured && <button disabled={off} title="Fracturé (Fracturing Orb)" onClick={() => edit({ kind: "fracture", affixIdx: m.affixIdx })}>◆</button>}
                <button disabled={off} title="Remplacé par… (Chaos)" onClick={() => setPicker({ slot: m.slot, replace: m.affixIdx })}>⇄</button>
                <button disabled={off} title="Retiré (Annulment)" onClick={() => edit({ kind: "remove", affixIdx: m.affixIdx })}>✕</button>
              </span>
            </div>
          );
        })}
        <div className="lv-add">
          <button className="btn sm" disabled={off} onClick={() => setPicker({ slot: "prefix" })}>+ Préfixe obtenu</button>
          <button className="btn sm" disabled={off} onClick={() => setPicker({ slot: "suffix" })}>+ Suffixe obtenu</button>
        </div>
      </div>

      {picker && interactive && (
        <div className="lv-pick">
          <div className="lv-head">
            <span className="small grow">{picker.replace !== undefined ? `Remplacé par quel ${slotLabel[picker.slot]} ?` : `Quel ${slotLabel[picker.slot]} as-tu obtenu ?`}</span>
            <button className="ov-close" title="Fermer" onClick={() => setPicker(null)}>✕</button>
          </div>
          <input type="search" autoFocus placeholder="Chercher (vie, résistance, Virile…)" value={filter} onChange={(e) => setFilter(e.target.value)} />
          <button className="btn sm" disabled={busy} onClick={junk} title="Son identité ne change pas le conseil : seul le slot occupé compte">Un {slotLabel[picker.slot]} non voulu quelconque</button>
          <div className="lv-list">
            {candidates.map((g) => (
              <div key={g.key} className={`lv-fam ${wanted(g) ? "want" : ""}`}>
                <span className="lv-famname">{wanted(g) && "★ "}{g.family}</span>
                <span className="lv-chips">
                  {available(g).map((t) => (
                    <button key={t.affixIdx} disabled={busy} title={`${t.name} · niv. ${t.level} · ${prettyText(t.text)}`} onClick={() => choose(t)}>T{t.tier}</button>
                  ))}
                </span>
              </div>
            ))}
            {candidates.length === 0 && <p className="muted small">Aucun affixe ne correspond.</p>}
          </div>
        </div>
      )}

      {error && <div className="ov-warn">{error}</div>}
      {view && <AdviceView cap={view} compact />}
    </div>
  );
}
