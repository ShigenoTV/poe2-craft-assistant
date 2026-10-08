// Moteur factice pour développer l'UI dans un navigateur : données réelles exportées par `craft-cli export-fixtures`,
// sandbox réimplémenté en JS (tirage pondéré). Ne sert JAMAIS dans l'application Tauri.
import type * as T from "../lib/types";

const fx = import.meta.glob("./fixtures/*.json", { eager: true, import: "default" }) as Record<string, unknown>;
const get = <R>(name: string) => fx[`./fixtures/${name}.json`] as R;
const listeners = new Map<string, Set<(p: unknown) => void>>();
export function listen(event: string, cb: (p: unknown) => void) {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event)!.add(cb);
  if (event === "item-captured" && new URLSearchParams(location.search).has("capture")) {
    const cap = get<T.ItemCaptured | undefined>("capture");
    if (cap) setTimeout(() => cb(cap), 50);
  }
  return () => listeners.get(event)!.delete(cb);
}
const emit = (e: string, p: unknown) => listeners.get(e)?.forEach((f) => f(p));

const wait = (ms = 60) => new Promise((r) => setTimeout(r, ms));
let settings: T.Settings = {
  hotkeyToggle: "Ctrl+D", hotkeyInteractive: "Ctrl+Shift+D", watchClipboard: true, checkUpdatesOnStart: true, autoShowOnCopy: true, cpuThreads: 0,
  defaultIlvl: 80, gameWindowTitle: "Path of Exile 2", overlayAutoHideSecs: 10, priceLeague: "", autoRefreshPrices: true, priceRefreshMinutes: 60, overlayWidth: 400, overlayHeight: 640, overlayMarginX: 24, overlayMarginY: 96, costUnit: "ex",
};
let overrides: Record<string, number> = {};

function drawAffix(pool: T.PoolView, item: T.ItemView, min: number, force: T.Slot | null): number | null {
  const cap = item.rarity === "normal" ? [0, 0] : item.rarity === "magic" ? [1, 1] : [3, 3];
  const held = new Set(item.mods.map((m) => pool.affixes[m.affixIdx].group));
  const cnt = (s: T.Slot) => item.mods.filter((m) => pool.affixes[m.affixIdx].slot === s).length;
  const open = { prefix: cnt("prefix") < cap[0], suffix: cnt("suffix") < cap[1] };
  const el = pool.affixes.map((a, i) => ({ a, i })).filter(({ a }) =>
    a.weight > 0 && a.reqIlvl <= item.ilvl && a.reqIlvl >= min && open[a.slot] && (!force || force === a.slot) && !held.has(a.group));
  const total = el.reduce((s, { a }) => s + a.weight, 0);
  if (!total) return null;
  let r = Math.random() * total;
  for (const { a, i } of el) { if (r < a.weight) return i; r -= a.weight; }
  return el[el.length - 1].i;
}

function detail(pool: T.PoolView, item: T.ItemView): T.ItemDetail {
  return {
    view: item,
    mods: item.mods.map((m) => {
      const a = pool.affixes[m.affixIdx];
      return { affixIdx: m.affixIdx, name: a.name, family: a.family, text: a.text, tier: a.tier, level: a.reqIlvl, slot: a.slot, fractured: m.fractured, weight: a.weight };
    }),
  };
}

function applyCur(pool: T.PoolView, item: T.ItemView, c: T.ActionView): boolean {
  const add = () => { const i = drawAffix(pool, item, c.minModLevel, c.addSlot); if (i !== null) item.mods.push({ affixIdx: i, fractured: false }); };
  const remove = () => {
    const idx = item.mods.map((m, i) => ({ m, i })).filter(({ m }) => !m.fractured && (!c.removeSlot || pool.affixes[m.affixIdx].slot === c.removeSlot));
    if (!idx.length) return false;
    item.mods.splice(idx[Math.floor(Math.random() * idx.length)].i, 1);
    return true;
  };
  const n = item.mods.length;
  switch (c.kind) {
    case "transmute": if (item.rarity !== "normal") return false; item.rarity = "magic"; add(); return true;
    case "augment": if (item.rarity !== "magic" || n >= 2) return false; add(); return true;
    case "regal": if (item.rarity !== "magic") return false; item.rarity = "rare"; add(); return true;
    case "alchemy": if (item.rarity !== "normal") return false; item.rarity = "rare"; for (let k = 0; k < 4; k++) add(); return true;
    case "exalt": if (item.rarity !== "rare" || n >= 6) return false; add(); return true;
    case "chaos": if (item.rarity !== "rare" || !remove()) return false; add(); return true;
    case "annul": return item.rarity !== "normal" && remove();
    case "fracture":
      if (item.rarity !== "rare" || n < 4 || item.mods.some((m) => m.fractured)) return false;
      item.mods[Math.floor(Math.random() * n)].fractured = true; return true;
    default: return false;
  }
}

// Suivi en direct factice (`?live`) : vraie mécanique de saisie, conseil figé repris de la capture d'exemple.
const liveMode = () => typeof location !== "undefined" && new URLSearchParams(location.search).has("live");
let liveHist: T.ItemView[] = [];
// monnaie comptée pour chaque étape (même index que liveHist ; la première est l'état de départ)
let liveSpend: (T.Spend | null)[] = [];
let history: T.CraftRecord[] = [];
function spendChoices(): T.Spend[] {
  const acts = get<T.ActionView[]>("actions").filter((a) => a.defaultEnabled).map((a) => ({ actionId: a.id, label: a.label, cost: a.unitCost }));
  return [...acts, { actionId: "new_base", label: "Base neuve", cost: get<T.CraftPlan>("plan").baseCost }];
}
const advisedSpend = (): T.Spend | null => {
  const a = get<T.ItemCaptured>("capture").advice?.advice?.action;
  return a && !a.isAbandon ? { actionId: a.id, label: a.label, cost: a.unitCost } : null;
};
const spent = () => liveSpend.reduce((x, s) => x + (s?.cost ?? 0), 0);
function liveView(): T.LiveView {
  const plan = get<T.CraftPlan>("plan");
  const pool = get<T.PoolView>(`pool_${plan.baseId}`);
  if (!liveHist.length) { liveHist = [{ rarity: "normal", ilvl: plan.ilvl, mods: [] }]; liveSpend = [null]; }
  const item = liveHist[liveHist.length - 1];
  const cap = get<T.ItemCaptured>("capture");
  const status = plan.goal.map((g) => {
    const m = item.mods.map((x) => pool.affixes[x.affixIdx]).find((a) => a.group === g.group);
    return !m ? "missing" : m.tier <= g.maxTier ? "held" : "blocked";
  }) as T.AdviceResult["wantedStatus"];
  const advice = cap.advice && { ...cap.advice, wantedStatus: status };
  return {
    baseId: plan.baseId, item: detail(pool, item), advice, adviceError: null, canUndo: liveHist.length > 1, steps: liveHist.length - 1,
    plannedCost: plan.expectedCost, spent: spent(), lastSpend: liveHist.length > 1 ? liveSpend[liveSpend.length - 1] : null, spendChoices: spendChoices(),
  };
}
function liveEdit(edits: T.LiveEdit[]) {
  const pool = get<T.PoolView>(`pool_${get<T.CraftPlan>("plan").baseId}`);
  const it = structuredClone(liveHist[liveHist.length - 1]);
  const cnt = (s: T.Slot) => it.mods.filter((m) => pool.affixes[m.affixIdx].slot === s).length;
  const over = () => { const c = it.rarity === "magic" ? 1 : it.rarity === "rare" ? 3 : 0; return cnt("prefix") > c || cnt("suffix") > c; };
  for (const e of edits) {
    if (e.kind === "add") {
      if (it.mods.some((m) => pool.affixes[m.affixIdx].group === pool.affixes[e.affixIdx].group)) throw "l'objet porte déjà un affixe de ce groupe";
      if (it.rarity === "normal") it.rarity = "magic";
      it.mods.push({ affixIdx: e.affixIdx, fractured: false });
      if (it.rarity === "magic" && over()) it.rarity = "rare";
    } else if (e.kind === "remove") it.mods = it.mods.filter((m) => m.affixIdx !== e.affixIdx);
    else if (e.kind === "replace") it.mods = it.mods.map((m) => (m.affixIdx === e.from ? { ...m, affixIdx: e.to } : m));
    else if (e.kind === "rarity") { it.rarity = e.rarity; if (e.rarity === "normal") it.mods = []; }
    else if (e.kind === "fracture") it.mods = it.mods.map((m) => (m.affixIdx === e.affixIdx ? { ...m, fractured: true } : m));
    if (over()) throw "trop d'affixes pour cette rareté";
  }
  liveHist.push(it);
  liveSpend.push(advisedSpend());
}
function liveFinish(): T.CraftRecord {
  const plan = get<T.CraftPlan>("plan");
  const v = liveView();
  if (v.steps === 0) throw "rien à enregistrer : aucune saisie depuis le début du suivi";
  const uses: T.UseLine[] = [];
  for (const s of liveSpend) {
    if (!s) continue;
    const u = uses.find((x) => x.actionId === s.actionId);
    if (u) { u.count++; u.cost += s.cost; } else uses.push({ actionId: s.actionId, label: s.label, count: 1, cost: s.cost });
  }
  uses.sort((a, b) => b.cost - a.cost);
  const r: T.CraftRecord = {
    id: Math.max(0, ...history.map((h) => h.id)) + 1, finishedAt: Math.floor(Date.now() / 1000), baseId: plan.baseId,
    baseName: get<T.DatasetInfo>("dataset_info").bases.find((b) => b.id === plan.baseId)?.name ?? plan.baseId, ilvl: plan.ilvl,
    goal: plan.goal.map((g) => g.label), success: v.advice?.wantedStatus.every((x) => x === "held") ?? false,
    plannedCost: v.plannedCost, realCost: v.spent, steps: v.steps, uses, pricesSource: plan.pricesSource,
  };
  history = [r, ...history];
  liveHist = [];
  liveSpend = [];
  emit("history-updated", null);
  return r;
}

export async function handle(cmd: string, a: Record<string, unknown>): Promise<unknown> {
  await wait();
  switch (cmd) {
    case "dataset_info": return get("dataset_info");
    case "base_pool": return get(`pool_${a.baseId}`);
    case "list_actions_cmd": return get("actions");
    case "get_prices": return { ...get<Record<string, number>>("prices"), ...overrides };
    case "set_prices": overrides = a.overrides as Record<string, number>; return null;
    case "sandbox_apply": {
      const pool = get<T.PoolView>(`pool_${a.baseId}`);
      const acts = get<T.ActionView[]>("actions");
      const cur = acts.find((x) => x.id === a.currencyId)!;
      const item = structuredClone(a.item as T.ItemView);
      const applied = applyCur(pool, item, cur);
      return { applied, item: detail(pool, item), cost: applied ? cur.unitCost : 0 } satisfies T.ApplyResult;
    }
    case "item_detail": return detail(get(`pool_${a.baseId}`), a.item as T.ItemView);
    case "run_simulation": {
      const ch = a.onEvent as { onmessage: (p: T.Progress) => void };
      const req = a.req as T.SimRequest;
      for (let i = 1; i <= 5; i++) { await wait(80); ch.onmessage({ stage: "simulating", done: (req.trials * i) / 5, total: req.trials }); }
      const p = 0.043;
      return { trials: req.trials, successes: Math.round(p * req.trials), pHat: p, ci95: [p - 0.001, p + 0.001], meanOrbsAll: 1.9, meanOrbsOnSuccess: 3.4, costPerSuccess: 41.2 } satisfies T.SimResult;
    }
    case "solve_plan": {
      const ch = a.onEvent as { onmessage: (p: T.Progress) => void };
      ch.onmessage({ stage: "solving", done: 0, total: 0 });
      await wait(400);
      const req = a.req as T.PlanRequest;
      for (let i = 1; i <= 4; i++) { await wait(120); ch.onmessage({ stage: "verifying", done: (req.mcTrials * i) / 4, total: req.mcTrials }); }
      return get("plan");
    }
    case "compare_paths": {
      const ch = a.onEvent as { onmessage: (p: T.Progress) => void };
      ch.onmessage({ stage: "solving", done: 0, total: 0 });
      await wait(400);
      const plan = get<T.CraftPlan>("plan");
      const lines = plan.shopping.filter((l) => l.id !== "__base");
      const path = (label: string, excluded: string | null, f: number, sd: number): T.ComparedPath => {
        const mean = plan.expectedCost * f;
        const mc = plan.mc ? { ...plan.mc, meanCost: mean, medianCost: mean * 0.75, p99Cost: mean * (3 + sd), stdDev: mean * sd } : null;
        return { label, excluded, excludedActions: excluded ? [excluded] : [], expectedCost: mean, mainCurrencies: lines.slice(0, 3), converged: true, mc };
      };
      return [path("Plan optimal", null, 1, 0.95), path("Sans Chaos", "chaos", 1.12, 0.7), path("Sans Omens", "omens", 1.3, 1.1)];
    }
    case "cancel_job": return null;
    case "active_plan": {
      if (!liveMode()) return null;
      const plan = get<T.CraftPlan>("plan");
      return { baseId: plan.baseId, ilvl: plan.ilvl, goal: plan.goal, expectedCost: plan.expectedCost } satisfies T.ActiveInfo;
    }
    case "live_state": return liveMode() ? liveView() : null;
    case "live_edit": liveEdit(a.edits as T.LiveEdit[]); return liveView();
    case "live_undo": if (liveHist.length > 1) { liveHist.pop(); liveSpend.pop(); } return liveView();
    case "live_reset": liveView(); liveHist.push({ rarity: "normal", ilvl: get<T.CraftPlan>("plan").ilvl, mods: [] }); { const c = spendChoices(); liveSpend.push(c[c.length - 1]); } return liveView();
    case "live_set_spend": if (liveHist.length > 1) liveSpend[liveSpend.length - 1] = spendChoices().find((c) => c.actionId === a.actionId) ?? null; return liveView();
    case "live_finish": return liveFinish();
    case "history_list": return history;
    case "history_delete": history = a.id === null || a.id === undefined ? [] : history.filter((h) => h.id !== a.id); emit("history-updated", null); return null;
    case "clear_active_plan": return null;
    case "submit_item_text": return get("capture") ?? null;
    case "analyze_item_text": return (get("capture") as { analysis?: T.ItemAnalysis } | null)?.analysis ?? { parsed: { itemClass: null, rarityLabel: null, rarity: null, name: null, baseType: null, itemLevel: null, corrupted: false, advanced: false, mods: [] }, baseId: null, detail: null, unmatched: [], error: "Mode démo : analyse d'objet non simulée." };
    case "last_clipboard": return "";
    case "get_settings": return settings;
    case "set_settings": settings = a.settings as T.Settings; emit("settings-changed", settings); return null;
    case "hotkey_status": return null;
    case "price_state": case "refresh_prices": {
      const eff = { ...get<Record<string, number>>("prices"), ...overrides };
      const market = cmd === "refresh_prices" ? Object.keys(get<Record<string, number>>("prices")).filter((k) => !k.startsWith("base_")) : [];
      const now = Math.floor(Date.now() / 1000), fetchedAt = market.length ? now - 720 : null;
      const updatedAt = Object.fromEntries(market.map((k, i) => [k, now - (i % 5 === 4 ? 4 * 3600 : 720)]));
      return { effective: eff, overrides, marketKeys: market, league: market.length ? "Forbidden Rites" : null, fetchedAt, updatedAt, missing: [], now, note: null, lastError: null, nextRefreshAt: fetchedAt && settings.autoRefreshPrices ? fetchedAt + settings.priceRefreshMinutes * 60 : null } satisfies T.PriceState;
    }
    case "overlay_toggle": emit("overlay-wanted", true); return null;
    case "overlay_set_interactive": emit("overlay-interactive", a.value); return null;
    case "overlay_state": return [true, liveMode()];
    case "app_version": return "0.1.0";
    case "check_update":
      if (new URLSearchParams(location.search).has("update")) return { version: "0.2.0", current: "0.1.0", notes: "Nouveaux réglages d'overlay et corrections du parseur.", date: "2026-09-28" };
      if (new URLSearchParams(location.search).has("unconfigured")) throw "not_configured";
      return null;
    case "install_update": {
      const ch = a.onEvent as { onmessage: (p: T.UpdateProgress) => void };
      for (let i = 1; i <= 5; i++) { await wait(300); ch.onmessage({ stage: "downloading", downloaded: i * 2_000_000, total: 10_000_000 }); }
      ch.onmessage({ stage: "installing", downloaded: 0, total: null });
      return null;
    }
    default: throw new Error(`commande factice inconnue : ${cmd}`);
  }
}
