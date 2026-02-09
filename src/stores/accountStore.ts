import { create } from "zustand";
import type { Account, PlatformType } from "@/types";

interface AccountStore {
  accounts: Account[];
  addAccount: (account: Account) => void;
  removeAccount: (id: number) => void;
  updateAccount: (id: number, updates: Partial<Account>) => void;
  getAccountsByPlatform: (platform: PlatformType) => Account[];
}

// Demo data for UI development
const demoAccounts: Account[] = [
  {
    id: 1,
    platform: "douyin",
    displayName: "创作者小王",
    chromeProfileDir: "~/.multi-publisher/profiles/douyin-1",
    isLoggedIn: true,
    lastCheckedAt: new Date().toISOString(),
    createdAt: new Date().toISOString(),
  },
  {
    id: 2,
    platform: "bilibili",
    displayName: "创作者小王",
    chromeProfileDir: "~/.multi-publisher/profiles/bilibili-1",
    isLoggedIn: true,
    lastCheckedAt: new Date().toISOString(),
    createdAt: new Date().toISOString(),
  },
  {
    id: 3,
    platform: "youtube",
    displayName: "Creator Wang",
    chromeProfileDir: "~/.multi-publisher/profiles/youtube-1",
    isLoggedIn: true,
    lastCheckedAt: new Date().toISOString(),
    createdAt: new Date().toISOString(),
  },
  {
    id: 4,
    platform: "xiaohongshu",
    displayName: "创作者小王",
    chromeProfileDir: "~/.multi-publisher/profiles/xiaohongshu-1",
    isLoggedIn: false,
    lastCheckedAt: new Date().toISOString(),
    createdAt: new Date().toISOString(),
  },
  {
    id: 5,
    platform: "wechat",
    displayName: "创作者小王",
    chromeProfileDir: "~/.multi-publisher/profiles/wechat-1",
    isLoggedIn: true,
    lastCheckedAt: new Date().toISOString(),
    createdAt: new Date().toISOString(),
  },
];

export const useAccountStore = create<AccountStore>((set, get) => ({
  accounts: demoAccounts,

  addAccount: (account) =>
    set((state) => ({ accounts: [...state.accounts, account] })),

  removeAccount: (id) =>
    set((state) => ({ accounts: state.accounts.filter((a) => a.id !== id) })),

  updateAccount: (id, updates) =>
    set((state) => ({
      accounts: state.accounts.map((a) =>
        a.id === id ? { ...a, ...updates } : a
      ),
    })),

  getAccountsByPlatform: (platform) =>
    get().accounts.filter((a) => a.platform === platform),
}));
