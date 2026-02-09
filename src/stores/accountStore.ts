import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { Account, PlatformType } from "@/types";

// Backend account type (from Rust)
interface BackendAccount {
  id: number;
  platform: string;
  display_name: string;
  avatar_url: string | null;
  chrome_profile_dir: string;
  is_logged_in: boolean;
  last_checked_at: string | null;
  created_at: string;
}

function mapBackendAccount(a: BackendAccount): Account {
  return {
    id: a.id,
    platform: a.platform as PlatformType,
    displayName: a.display_name,
    avatarUrl: a.avatar_url ?? undefined,
    chromeProfileDir: a.chrome_profile_dir,
    isLoggedIn: a.is_logged_in,
    lastCheckedAt: a.last_checked_at ?? undefined,
    createdAt: a.created_at,
  };
}

interface AccountStore {
  accounts: Account[];
  loading: boolean;
  error: string | null;
  fetchAccounts: () => Promise<void>;
  addAccount: (platform: PlatformType, displayName: string) => Promise<void>;
  removeAccount: (id: number) => Promise<void>;
  openLogin: (id: number) => Promise<void>;
  openPlatform: (id: number) => Promise<void>;
  updateLoginStatus: (id: number, isLoggedIn: boolean) => Promise<void>;
}

export const useAccountStore = create<AccountStore>((set, get) => ({
  accounts: [],
  loading: false,
  error: null,

  fetchAccounts: async () => {
    set({ loading: true, error: null });
    try {
      const result = await invoke<BackendAccount[]>("get_accounts");
      set({ accounts: result.map(mapBackendAccount), loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  addAccount: async (platform, displayName) => {
    try {
      const result = await invoke<BackendAccount>("add_account", {
        platform,
        displayName,
      });
      set((state) => ({
        accounts: [mapBackendAccount(result), ...state.accounts],
      }));
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removeAccount: async (id) => {
    try {
      await invoke("delete_account", { accountId: id });
      set((state) => ({
        accounts: state.accounts.filter((a) => a.id !== id),
      }));
    } catch (e) {
      set({ error: String(e) });
    }
  },

  openLogin: async (id) => {
    try {
      await invoke("open_login", { accountId: id });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  openPlatform: async (id) => {
    try {
      await invoke("open_platform", { accountId: id });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  updateLoginStatus: async (id, isLoggedIn) => {
    try {
      await invoke("update_login_status", { accountId: id, isLoggedIn });
      set((state) => ({
        accounts: state.accounts.map((a) =>
          a.id === id ? { ...a, isLoggedIn } : a
        ),
      }));
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));
