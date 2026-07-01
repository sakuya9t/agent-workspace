import { create } from "zustand";

interface UiState {
  activeSessionId: string | null;
  setActive: (id: string | null) => void;
  showNewSession: boolean;
  setShowNewSession: (v: boolean) => void;
}

/** Local UI-only state. Server-derived data lives in TanStack Query. */
export const useUiStore = create<UiState>((set) => ({
  activeSessionId: null,
  setActive: (id) => set({ activeSessionId: id }),
  showNewSession: false,
  setShowNewSession: (v) => set({ showNewSession: v }),
}));
