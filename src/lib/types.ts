// Miroir des types sérialisés par les crates Rust (serde, camelCase). Garder synchronisé avec craft-api / craft-solver.

export type Slot = "prefix" | "suffix";
export type Rarity = "normal" | "magic" | "rare";
export type CurrencyKind = "transmute" | "augment" | "regal" | "alchemy" | "exalt" | "chaos" | "annul" | "fracture";

export interface BaseView { id: string; name: string; itemClass: string; tags: string[] }
export interface DatasetInfo {
  source: string; gameVersion: string; generatedAt: string; notice: string; priceUnit: string;
  modCount: number; bases: BaseView[];
}
export interface TierInfo { tier: number; level: number; weight: number; name: string; text: string; affixIdx: number }
export interface GroupInfo { group: number; key: string; family: string; slot: Slot; totalWeight: number; tiers: TierInfo[] }
export interface Affix {
  id: string; name: string; family: string; text: string; group: number; slot: Slot;
  tier: number; reqIlvl: number; weight: number; tags: number;
}
export interface PoolView { base: BaseView; groups: GroupInfo[]; affixes: Affix[] }

export interface ActionView {
  id: string; label: string; kind: CurrencyKind; minModLevel: number;
  addSlot: Slot | null; removeSlot: Slot | null; unitCost: number; defaultEnabled: boolean;
}

export interface ModView { affixIdx: number; fractured: boolean }
export interface ItemView { rarity: Rarity; ilvl: number; mods: ModView[] }
export interface ModDetail {
  affixIdx: number; name: string; family: string; text: string; tier: number; level: number;
  slot: Slot; fractured: boolean; weight: number;
}
export interface ItemDetail { view: ItemView; mods: ModDetail[] }
export interface ApplyResult { applied: boolean; item: ItemDetail; cost: number }

export interface WantedReq { group: string; maxTier: number }

export interface PlanRequest {
  baseId: string; ilvl: number; wanted: WantedReq[];
  enabledActions?: string[] | null; prices?: Record<string, number> | null;
  allowAbandon: boolean; mcTrials: number; nodeCap: number; seed: number;
  startingItem?: ItemView | null;
}
export interface SimRequest {
  baseId: string; ilvl: number; start: ItemView; wanted: WantedReq[]; currencyId: string;
  maxOrbs: number; trials: number; seed: number; baseCost: number;
}
export interface SimResult {
  trials: number; successes: number; pHat: number; ci95: [number, number];
  meanOrbsAll: number; meanOrbsOnSuccess: number | null; costPerSuccess: number | null;
}
export interface Progress { stage: "simulating" | "solving" | "verifying"; done: number; total: number }

// ── Plan de craft
export interface GoalItem { label: string; slot: Slot; group: number; maxTier: number }
export interface ItemSummary {
  rarity: Rarity; heldWanted: number[]; blockedWanted: number[]; fracturedWanted: number | null;
  badPrefixes: number; badSuffixes: number;
}
export interface ActionInfo { id: string; label: string; unitCost: number; isAbandon: boolean }
export interface Repeat { selfLoopProbability: number; expectedAttempts: number; p90Attempts: number }
export interface Branch {
  id: string; kind: "success" | "failure"; label: string; probability: number; to: string;
  loopback: boolean; extraCost: number;
}
export interface ActionNodeData {
  kind: "action"; id: string; stateKey: string; state: ItemSummary; expectedVisits: number; costToGo: number;
  action: ActionInfo; repeat: Repeat | null; branches: Branch[]; mergedMinorProbability: number; mergedMinorCount: number;
}
export interface TerminalNodeData {
  kind: "terminal"; id: string; stateKey: string; state: ItemSummary; expectedVisits: number; costToGo: number;
  result: string; note: string | null;
}
export type CraftNode = ActionNodeData | TerminalNodeData;
export interface ShoppingLine { id: string; label: string; expectedCount: number; unitCost: number; expectedCost: number }
export interface VerifyResult {
  trials: number; meanCost: number; ci95Mean: [number, number]; medianCost: number; p90Cost: number; p99Cost: number;
  meanSteps: number; meanAbandons: number; censored: number;
}
export interface CraftPlan {
  version: number; baseId: string; ilvl: number; goal: GoalItem[]; rootId: string;
  nodes: Record<string, CraftNode>; expectedCost: number; baseCost: number; shopping: ShoppingLine[];
  solver: { states: number; sweeps: number; converged: boolean; millis: number; costFromVisits: number };
  mc: VerifyResult | null; pricesSource: string;
}

// ── Presse-papiers / overlay
export interface ParsedMod {
  kind: "implicit" | "explicit" | "rune" | "enchant"; slot: Slot | null; name: string | null; tier: number | null;
  tags: string[]; lines: string[]; fractured: boolean; desecrated: boolean;
}
export interface ParsedItem {
  itemClass: string | null; rarityLabel: string | null; rarity: Rarity | null; name: string | null;
  baseType: string | null; itemLevel: number | null; corrupted: boolean; advanced: boolean; mods: ParsedMod[];
}
export interface ItemAnalysis { parsed: ParsedItem; baseId: string | null; detail: ItemDetail | null; unmatched: string[]; error: string | null }
export interface AdviceOutcome { label: string; probability: number; kind: "success" | "failure"; costToGo: number | null }
export interface Advice {
  stateKey: string; state: ItemSummary; goalReached: boolean; costToGo: number | null;
  action: ActionInfo | null; repeat: Repeat | null; outcomes: AdviceOutcome[];
}
export interface AdviceResult { dead: boolean; advice: Advice | null; goal: GoalItem[]; wantedStatus: ("held" | "blocked" | "missing")[] }
export interface ItemCaptured { analysis: ItemAnalysis; advice: AdviceResult | null; adviceError: string | null; raw: string }
export interface ActiveInfo { baseId: string; ilvl: number; goal: GoalItem[]; expectedCost: number }

export interface Settings {
  hotkeyToggle: string; hotkeyInteractive: string; watchClipboard: boolean; checkUpdatesOnStart: boolean; autoShowOnCopy: boolean;
  cpuThreads: number; defaultIlvl: number; gameWindowTitle: string;
  overlayAutoHideSecs: number; priceLeague: string; autoRefreshPrices: boolean; overlayWidth: number; overlayHeight: number; overlayMarginX: number; overlayMarginY: number;
}

export interface UpdateInfo { version: string; current: string; notes: string | null; date: string | null }
export interface UpdateProgress { stage: "downloading" | "installing"; downloaded: number; total: number | null }

export interface PriceState {
  effective: Record<string, number>; overrides: Record<string, number>; marketKeys: string[];
  league: string | null; fetchedAt: number | null; missing: string[]; now: number; note: string | null;
}
