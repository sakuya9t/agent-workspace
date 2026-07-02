import { create } from "zustand";

/** Identifies a session by its owning daemon plus id. */
export interface ActiveRef {
  daemonId: string;
  sessionId: string;
}

interface UiState {
  activeSession: ActiveRef | null;
  setActive: (a: ActiveRef | null) => void;
  showNewSession: boolean;
  setShowNewSession: (v: boolean) => void;
  /** Daemon to preselect when the new-session dialog opens. */
  newSessionDaemonId: string | null;
  /** Workspace to preselect when the new-session dialog opens. */
  newSessionWorkspaceId: string | null;
  openNewSession: (daemonId?: string | null, workspaceId?: string | null) => void;
  showConnection: boolean;
  setShowConnection: (v: boolean) => void;
}

/** Local UI-only state. Server-derived data lives in TanStack Query. */
export const useUiStore = create<UiState>((set) => ({
  activeSession: null,
  setActive: (a) => set({ activeSession: a }),
  showNewSession: false,
  setShowNewSession: (v) => set({ showNewSession: v }),
  newSessionDaemonId: null,
  newSessionWorkspaceId: null,
  openNewSession: (daemonId = null, workspaceId = null) =>
    set({
      showNewSession: true,
      newSessionDaemonId: daemonId,
      newSessionWorkspaceId: workspaceId,
    }),
  showConnection: false,
  setShowConnection: (v) => set({ showConnection: v }),
}));
