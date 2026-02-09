import { create } from "zustand";

interface PublishFormState {
  videoPath: string | null;
  videoName: string | null;
  title: string;
  description: string;
  tags: string[];
  isOriginal: boolean;
  isScheduled: boolean;
  scheduledAt: string | null;
  manualConfirm: boolean;
  selectedAccountIds: number[];
}

interface PublishStore extends PublishFormState {
  setVideoPath: (path: string | null, name?: string | null) => void;
  setTitle: (title: string) => void;
  setDescription: (description: string) => void;
  addTag: (tag: string) => void;
  removeTag: (tag: string) => void;
  setIsOriginal: (value: boolean) => void;
  setIsScheduled: (value: boolean) => void;
  setScheduledAt: (value: string | null) => void;
  setManualConfirm: (value: boolean) => void;
  toggleAccount: (accountId: number) => void;
  selectAllAccounts: (accountIds: number[]) => void;
  deselectAllAccounts: () => void;
  resetForm: () => void;
}

const initialState: PublishFormState = {
  videoPath: null,
  videoName: null,
  title: "",
  description: "",
  tags: [],
  isOriginal: true,
  isScheduled: false,
  scheduledAt: null,
  manualConfirm: true,
  selectedAccountIds: [],
};

export const usePublishStore = create<PublishStore>((set) => ({
  ...initialState,

  setVideoPath: (path, name) => set({ videoPath: path, videoName: name ?? null }),
  setTitle: (title) => set({ title }),
  setDescription: (description) => set({ description }),
  addTag: (tag) =>
    set((state) => ({
      tags: state.tags.includes(tag) ? state.tags : [...state.tags, tag],
    })),
  removeTag: (tag) =>
    set((state) => ({ tags: state.tags.filter((t) => t !== tag) })),
  setIsOriginal: (value) => set({ isOriginal: value }),
  setIsScheduled: (value) => set({ isScheduled: value }),
  setScheduledAt: (value) => set({ scheduledAt: value }),
  setManualConfirm: (value) => set({ manualConfirm: value }),
  toggleAccount: (accountId) =>
    set((state) => ({
      selectedAccountIds: state.selectedAccountIds.includes(accountId)
        ? state.selectedAccountIds.filter((id) => id !== accountId)
        : [...state.selectedAccountIds, accountId],
    })),
  selectAllAccounts: (accountIds) => set({ selectedAccountIds: accountIds }),
  deselectAllAccounts: () => set({ selectedAccountIds: [] }),
  resetForm: () => set(initialState),
}));
