import { useEffect, useState } from "react";
import { useStore } from "@/store";
import { markNotesSeen, releases, unseenNotes } from "@/lib/changelog";
import type { ChangelogSection } from "../../tools/changelog.mjs";

/** Texte d'une version : « ### Titre » et puces « - », le reste en paragraphes. */
export function NotesText({ text }: { text: string }) {
  const blocks: JSX.Element[] = [];
  let items: string[] = [];
  const flush = () => { if (items.length) blocks.push(<ul key={blocks.length}>{items.map((t, i) => <li key={i}>{t}</li>)}</ul>); items = []; };
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line.startsWith("- ") || line.startsWith("* ")) { items.push(line.slice(2)); continue; }
    flush();
    if (line.startsWith("### ")) blocks.push(<h4 key={blocks.length}>{line.slice(4)}</h4>);
    else if (line) blocks.push(<p key={blocks.length}>{line}</p>);
  }
  flush();
  return <div className="notes-text">{blocks}</div>;
}

const title = (s: ChangelogSection) => `Version ${s.version}${s.date ? ` (${s.date})` : ""}`;

/** Fenêtre « Nouveautés » : une seule fois après une mise à jour. */
export function WhatsNew() {
  const version = useStore((s) => s.version);
  const [shown, setShown] = useState<ChangelogSection[]>([]);
  useEffect(() => {
    if (!version) return;
    const unseen = unseenNotes(version);
    if (unseen.length) setShown(unseen); else markNotesSeen(version);
  }, [version]);
  if (!shown.length) return null;
  const close = () => { markNotesSeen(version); setShown([]); };
  return (
    <div className="modal-bg" onClick={close}>
      <div className="modal panel pad stack" role="dialog" aria-modal="true" aria-label="Nouveautés" onClick={(e) => e.stopPropagation()}>
        <h2 className="hd">Nouveautés</h2>
        <div className="modal-body stack">
          {shown.map((s) => <section key={s.version}><b>{title(s)}</b><NotesText text={s.body} /></section>)}
        </div>
        <div className="row" style={{ justifyContent: "flex-end" }}><button className="btn primary" autoFocus onClick={close}>Fermer</button></div>
      </div>
    </div>
  );
}

/** Historique des dernières versions, dans les Réglages (les plus récentes, celles qui servent encore). */
const HISTORY_SIZE = 5;

export function ReleaseHistory() {
  const all = releases().slice(0, HISTORY_SIZE);
  return (
    <div className="panel pad stack" style={{ gap: 10 }}>
      <h3 className="hd">Historique des versions</h3>
      {all.length === 0 && <p className="note small">Aucune note de version pour l'instant.</p>}
      {all.map((s, i) => (
        <details key={s.version} className="box" open={i === 0}><summary>{title(s)}</summary><div><NotesText text={s.body} /></div></details>
      ))}
    </div>
  );
}
