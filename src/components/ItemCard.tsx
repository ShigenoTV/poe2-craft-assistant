import type { GoalItem, ItemDetail, Rarity } from "@/lib/types";
import { prettyText, rarityLabel } from "@/lib/format";

const CAP: Record<Rarity, [number, number]> = { normal: [0, 0], magic: [1, 1], rare: [3, 3] };

interface Props {
  item: ItemDetail;
  title?: string;
  subtitle?: string;
  /** groupes voulus (clé = famille) pour surligner ce qui compte */
  goal?: { families: Set<string> };
}

export function ItemCard({ item, title, subtitle, goal }: Props) {
  const { rarity, ilvl } = item.view;
  const [capP, capS] = CAP[rarity];
  const pre = item.mods.filter((m) => m.slot === "prefix");
  const suf = item.mods.filter((m) => m.slot === "suffix");
  const free = (n: number, label: string) => Array.from({ length: n }, (_, i) => <div key={`${label}${i}`} className="slot-free">{label} libre</div>);
  const row = (m: ItemDetail["mods"][number]) => (
    <div key={m.affixIdx} className={`mod ${m.fractured ? "fractured" : ""} ${goal?.families.has(m.family) ? "hit" : ""}`}>
      <span className="tier" title={`niveau de mod ${m.level}`}>T{m.tier}</span>
      <span className="tx">{prettyText(m.text)}<span className="fam">{m.family}</span></span>
      {m.fractured && <span className="lock" title="Fracturé : ne peut plus être retiré">◆</span>}
    </div>
  );
  return (
    <div className={`item ${rarity}`}>
      <div className="item-head">
        <b>{title ?? `Objet ${rarityLabel[rarity].toLowerCase()}`}</b>
        <span>{subtitle ?? `${rarityLabel[rarity]} · niveau d'objet ${ilvl}`}</span>
      </div>
      <div className="item-body">
        {pre.map(row)}{free(capP - pre.length, "Préfixe")}
        {rarity !== "normal" && <div style={{ height: 1, background: "#26333a", margin: "3px 0" }} />}
        {suf.map(row)}{free(capS - suf.length, "Suffixe")}
        {rarity === "normal" && <div className="slot-free" style={{ paddingLeft: 0 }}>Aucun modificateur</div>}
      </div>
    </div>
  );
}

export const goalFamilies = (goal: GoalItem[], groups: { group: number; family: string }[]) =>
  new Set(goal.map((g) => groups.find((x) => x.group === g.group)?.family).filter((x): x is string => !!x));
