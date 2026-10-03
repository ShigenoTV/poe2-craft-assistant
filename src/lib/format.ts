import type { Rarity, ItemSummary } from "./types";

const nf = (max: number) => new Intl.NumberFormat("fr-FR", { maximumFractionDigits: max, minimumFractionDigits: 0 });
const n0 = nf(0), n1 = nf(1), n2 = nf(2);

/** Coût : plus de décimales quand c'est petit. */
export function cost(v: number, unit = "ex"): string {
  const a = Math.abs(v);
  const s = a >= 100 ? n0.format(v) : a >= 10 ? n1.format(v) : n2.format(v);
  return `${s}\u202f${unit}`;
}
export const num = (v: number, d = 1) => nf(d).format(v);
export function pct(p: number): string {
  const v = p * 100;
  if (v >= 10) return `${n0.format(v)}\u202f%`;
  if (v >= 1) return `${n1.format(v)}\u202f%`;
  if (v >= 0.1) return `${n2.format(v)}\u202f%`;
  return "<\u202f0,1\u202f%";
}
export const rarityLabel: Record<Rarity, string> = { normal: "Normal", magic: "Magique", rare: "Rare" };

export function summarizeState(s: ItemSummary, goalLen: number): string {
  const parts: string[] = [];
  parts.push(`${s.heldWanted.length}/${goalLen} voulus`);
  if (s.blockedWanted.length) parts.push(`${s.blockedWanted.length} bloqué${s.blockedWanted.length > 1 ? "s" : ""}`);
  const junk = s.badPrefixes + s.badSuffixes;
  if (junk) parts.push(`${junk} inutile${junk > 1 ? "s" : ""}`);
  return parts.join(" · ");
}

/** Montre la plage entre parenthèses de façon lisible : « +(40-49) » → « +(40–49) ». */
export const prettyText = (t: string) => t.replace(/\((\d+(?:\.\d+)?)-(\d+(?:\.\d+)?)\)/g, "($1–$2)");

/** « Exalted Orb » → « ex » : les montants restent lisibles dans les tableaux. */
export const shortUnit = (u?: string) => (!u || /exalt/i.test(u) ? "ex" : u);
