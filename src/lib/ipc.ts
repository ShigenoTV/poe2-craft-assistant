import { Channel, invoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import type * as T from "./types";

/** Hors Tauri (navigateur, `npm run dev:web`), on branche un moteur factice alimenté par des fixtures réelles. */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type MockModule = typeof import("../mock/mock");
let mockPromise: Promise<MockModule> | null = null;
const mock = () => (mockPromise ??= import("../mock/mock"));

async function call<R>(cmd: string, args?: Record<string, unknown>): Promise<R> {
  if (isTauri) return invoke<R>(cmd, args);
  return (await mock()).handle(cmd, args ?? {}) as Promise<R>;
}

export async function listen<P>(event: string, cb: (payload: P) => void): Promise<() => void> {
  if (isTauri) return tauriListen<P>(event, (e) => cb(e.payload));
  return (await mock()).listen(event, cb as (p: unknown) => void);
}

function channel<P = T.Progress>(onProgress?: (p: P) => void) {
  if (!isTauri) return { onmessage: (p: P) => onProgress?.(p) } as unknown as Channel<P>;
  const ch = new Channel<P>();
  ch.onmessage = (p) => onProgress?.(p);
  return ch;
}

export const api = {
  datasetInfo: () => call<T.DatasetInfo>("dataset_info"),
  basePool: (baseId: string) => call<T.PoolView>("base_pool", { baseId }),
  listActions: () => call<T.ActionView[]>("list_actions_cmd"),
  getPrices: () => call<Record<string, number>>("get_prices"),
  priceState: () => call<T.PriceState>("price_state"),
  refreshPrices: () => call<T.PriceState>("refresh_prices"),
  setPrices: (overrides: Record<string, number>) => call<void>("set_prices", { overrides }),
  sandboxApply: (baseId: string, item: T.ItemView, currencyId: string) =>
    call<T.ApplyResult>("sandbox_apply", { baseId, item, currencyId }),
  itemDetail: (baseId: string, item: T.ItemView) => call<T.ItemDetail>("item_detail", { baseId, item }),
  runSimulation: (req: T.SimRequest, onProgress?: (p: T.Progress) => void) =>
    call<T.SimResult | null>("run_simulation", { req, onEvent: channel(onProgress) }),
  solvePlan: (req: T.PlanRequest, activate: boolean, onProgress?: (p: T.Progress) => void) =>
    call<T.CraftPlan>("solve_plan", { req, activate, onEvent: channel(onProgress) }),
  cancelJob: () => call<void>("cancel_job"),
  activePlan: () => call<T.ActiveInfo | null>("active_plan"),
  clearActivePlan: () => call<void>("clear_active_plan"),
  submitItemText: (text: string) => call<T.ItemCaptured>("submit_item_text", { text }),
  analyzeItemText: (text: string, baseHint: string | null, fallbackIlvl: number) => call<T.ItemAnalysis>("analyze_item_text", { text, baseHint, fallbackIlvl }),
  lastClipboard: () => call<string>("last_clipboard"),
  getSettings: () => call<T.Settings>("get_settings"),
  setSettings: (settings: T.Settings) => call<void>("set_settings", { settings }),
  hotkeyStatus: () => call<string | null>("hotkey_status"),
  overlayToggle: () => call<void>("overlay_toggle"),
  overlaySetInteractive: (value: boolean) => call<void>("overlay_set_interactive", { value }),
  overlayState: () => call<[boolean, boolean]>("overlay_state"),
  appVersion: () => call<string>("app_version"),
  checkUpdate: () => call<T.UpdateInfo | null>("check_update"),
  installUpdate: (onProgress?: (p: T.UpdateProgress) => void) =>
    call<void>("install_update", { onEvent: channel<T.UpdateProgress>(onProgress) }),
};
