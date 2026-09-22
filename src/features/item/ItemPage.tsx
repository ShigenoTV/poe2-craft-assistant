import { useEffect, useState } from "react";
import { api, listen } from "@/lib/ipc";
import { useStore } from "@/store";
import { ItemCard } from "@/components/ItemCard";
import { AdviceView } from "@/components/AdviceView";
import type { ActiveInfo, ItemCaptured } from "@/lib/types";

export function ItemPage() {
  const [text, setText] = useState("");
  const [cap, setCap] = useState<ItemCaptured | null>(null);
  const [active, setActive] = useState<ActiveInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const info = useStore((s) => s.info);
  const plan = useStore((s) => s.plan);

  useEffect(() => {
    void api.activePlan().then(setActive);
    void api.lastClipboard().then((t) => t && setText(t));
    let off = () => {};
    void listen<ItemCaptured>("item-captured", (c) => { setCap(c); setText(c.raw); }).then((f) => (off = f));
    return () => off();
  }, [plan]);

  const analyze = async () => {
    setBusy(true); setErr(null);
    try { setCap(await api.submitItemText(text)); } catch (e) { setErr(String(e)); } finally { setBusy(false); }
  };
  const a = cap?.analysis;
  return (
    <div className="page">
      <div className="page-head"><h1>Objet en jeu</h1><p>Tout objet copié dans le jeu (<span className="kbd">Ctrl+C</span>, ou <span className="kbd">Ctrl+Alt+C</span> pour les détails complets) est analysé ici et dans l'overlay.</p></div>
      <div className="grid2">
        <div className="stack">
          <div className="panel pad stack" style={{ gap: 10 }}>
            <h3 className="hd">Texte de l'objet</h3>
            <textarea rows={16} value={text} onChange={(e) => setText(e.target.value)} placeholder="Copie un objet dans le jeu, ou colle son texte ici." spellCheck={false} />
            <div className="row">
              <button className="btn primary" disabled={busy || !text.trim()} onClick={() => void analyze()}>Analyser</button>
              <span className="muted small">
                {active ? <>Plan actif : {info?.bases.find((b) => b.id === active.baseId)?.name} · {active.goal.length} affixes voulus</> : "Aucun plan actif : calcule un plan dans « Reverse-crafting » pour recevoir des conseils."}
              </span>
            </div>
            {err && <div className="err">{err}</div>}
            <p className="note small">Formats non garantis : si des lignes ne sont pas reconnues, garde ce texte brut pour ajuster l'analyseur. Le format avancé (Ctrl+Alt+C) est le plus fiable.</p>
          </div>
        </div>
        <div className="stack">
          {!cap && <div className="panel empty">Rien d'analysé pour l'instant.</div>}
          {a?.error && <div className="err">{a.error}</div>}
          {a?.detail && <ItemCard item={a.detail} title={a.parsed.name ?? undefined} subtitle={`${a.parsed.baseType ?? ""} · niveau d'objet ${a.parsed.itemLevel ?? "?"}${a.parsed.advanced ? " · format avancé" : " · format simple"}`} />}
          {a && a.unmatched.length > 0 && (
            <div className="panel pad"><div className="hd small" style={{ marginBottom: 4 }}>Lignes non reconnues ({a.unmatched.length})</div>
              {a.unmatched.map((u, i) => <div key={i} className="muted small">{u}</div>)}</div>
          )}
          {cap && <div className="panel pad"><AdviceView cap={cap} /></div>}
        </div>
      </div>
    </div>
  );
}
