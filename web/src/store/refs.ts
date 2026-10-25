import { create } from "zustand";
import { api } from "../api/client";
import type { Account, Fund, Me, Project } from "../api/types";
import { useAuthStore } from "./auth";

/** Reference data many pages need for pickers and code → name lookups. */
interface RefsState {
  baseCurrency: string;
  funds: Fund[];
  projects: Project[];
  accounts: Account[];
  loaded: boolean;
  load: (force?: boolean) => Promise<void>;
}

export const useRefs = create<RefsState>((set, get) => ({
  baseCurrency: "SAR",
  funds: [],
  projects: [],
  accounts: [],
  loaded: false,
  load: async (force = false) => {
    if (get().loaded && !force) return;
    const can = useAuthStore.getState().can;
    const [me, funds, projects, accounts] = await Promise.all([
      api.get<Me & { base_currency: string }>("/auth/me"),
      can("reports.read") ? api.get<Fund[]>("/funds") : Promise.resolve([]),
      can("reports.read") ? api.get<Project[]>("/projects") : Promise.resolve([]),
      can("ledger.read") ? api.get<Account[]>("/accounts") : Promise.resolve([]),
    ]);
    set({ baseCurrency: me.base_currency, funds, projects, accounts, loaded: true });
  },
}));

export function localName(item: { name_ar: string; name_en: string } | undefined, lang: string): string {
  if (!item) return "—";
  return lang.startsWith("ar") ? item.name_ar || item.name_en : item.name_en || item.name_ar;
}
