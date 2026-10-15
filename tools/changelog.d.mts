export interface ChangelogSection { version: string; date: string | null; body: string }
export const NEXT: string;
export function parseChangelog(text: string): ChangelogSection[];
export function notesFor(text: string, version: string): string | null;
export function stampNext(text: string, version: string, date: string): string;
export function newerThan(text: string, since: string | null, upTo: string): ChangelogSection[];
