import text from "../../CHANGELOG.md?raw";
import { newerThan, notesFor, parseChangelog, type ChangelogSection } from "../../tools/changelog.mjs";

/** Notes de version embarquées dans l'appli (CHANGELOG.md, lisible hors ligne). */
export const releases = (): ChangelogSection[] => parseChangelog(text).filter((s) => /^\d+\.\d+\.\d+$/.test(s.version) && notesFor(text, s.version));

const SEEN = "notes-version-vues";
const read = () => { try { return localStorage.getItem(SEEN); } catch { return null; } };

/**
 * Notes à montrer une fois dans « Nouveautés » : versions publiées depuis la dernière vue, jusqu'à `current`.
 * Première fois (aucune version vue) : seulement celles de `current`, pas tout l'historique.
 */
export function unseenNotes(current: string): ChangelogSection[] {
  const seen = read();
  if (seen === current) return [];
  if (!seen) { const body = notesFor(text, current); return body ? [{ version: current, date: releases().find((r) => r.version === current)?.date ?? null, body }] : []; }
  return newerThan(text, seen, current);
}

export function markNotesSeen(current: string) { try { localStorage.setItem(SEEN, current); } catch { /* stockage indisponible : la fenêtre reviendra */ } }
