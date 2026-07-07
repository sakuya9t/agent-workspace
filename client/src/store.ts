import { create } from "zustand";

/** Identifies a session by its owning daemon plus id. */
export interface ActiveRef {
  daemonId: string;
  sessionId: string;
}

/** Drag limits (px) for the resizable side panels; center takes the rest. */
export const PANEL_LIMITS = {
  left: { min: 180, max: 600, default: 280 },
  right: { min: 220, max: 640, default: 320 },
} as const;

/** Width (px) of each draggable divider column; two sit in the grid. */
export const RESIZER_W = 6;
/** Smallest the center (terminal) column is allowed to get. */
export const MIN_CENTER = 320;

const WIDTHS_KEY = "asm.panelWidths";

export interface PanelWidths {
  left: number;
  right: number;
}

const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

/**
 * Shrink the stored side-panel widths so the center column keeps at least
 * MIN_CENTER, given the current viewport. Panels never grow here — only shrink,
 * and never below their own minimums. Applied at render time so a wide layout
 * saved earlier (or a since-narrowed window) can't collapse the terminal or make
 * the two divider hit-targets overlap. Panels shrink proportionally so neither
 * side visually dominates the squeeze.
 */
export function fitPanels(left: number, right: number, viewportW: number): PanelWidths {
  const budget = viewportW - 2 * RESIZER_W - MIN_CENTER;
  if (left + right <= budget || left + right <= 0) return { left, right };
  const scale = budget / (left + right);
  return {
    left: Math.max(PANEL_LIMITS.left.min, Math.floor(left * scale)),
    right: Math.max(PANEL_LIMITS.right.min, Math.floor(right * scale)),
  };
}

function loadWidths(): PanelWidths {
  const fallback = { left: PANEL_LIMITS.left.default, right: PANEL_LIMITS.right.default };
  try {
    const raw = localStorage.getItem(WIDTHS_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<PanelWidths>;
      return {
        left: clamp(p.left ?? fallback.left, PANEL_LIMITS.left.min, PANEL_LIMITS.left.max),
        right: clamp(p.right ?? fallback.right, PANEL_LIMITS.right.min, PANEL_LIMITS.right.max),
      };
    }
  } catch {
    /* ignore */
  }
  return fallback;
}

function persistWidths(w: PanelWidths) {
  try {
    localStorage.setItem(WIDTHS_KEY, JSON.stringify(w));
  } catch {
    /* ignore */
  }
}

interface UiState {
  activeSession: ActiveRef | null;
  setActive: (a: ActiveRef | null) => void;
  showNewSession: boolean;
  setShowNewSession: (v: boolean) => void;
  /** Daemon to preselect when the new-session dialog opens. */
  newSessionDaemonId: string | null;
  /**
   * Workspace to preselect when the new-session dialog opens. When set (the
   * dialog was opened from a workspace's "+"), daemon and workspace are locked.
   */
  newSessionWorkspaceId: string | null;
  openNewSession: (daemonId?: string | null, workspaceId?: string | null) => void;
  showNewWorkspace: boolean;
  setShowNewWorkspace: (v: boolean) => void;
  /** Daemon the new-workspace dialog registers into. */
  newWorkspaceDaemonId: string | null;
  openNewWorkspace: (daemonId: string) => void;
  showConnection: boolean;
  setShowConnection: (v: boolean) => void;
  /** Usage-transcript modal for the active session (per-agent, view-only). */
  showUsage: boolean;
  setShowUsage: (v: boolean) => void;
  /** Width (px) of the left session list; persisted across reloads. */
  leftWidth: number;
  /** Width (px) of the right details panel; persisted across reloads. */
  rightWidth: number;
  setLeftWidth: (px: number) => void;
  setRightWidth: (px: number) => void;
}

const initialWidths = loadWidths();

/** Local UI-only state. Server-derived data lives in TanStack Query. */
export const useUiStore = create<UiState>((set, get) => ({
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
  showNewWorkspace: false,
  setShowNewWorkspace: (v) => set({ showNewWorkspace: v }),
  newWorkspaceDaemonId: null,
  openNewWorkspace: (daemonId) =>
    set({ showNewWorkspace: true, newWorkspaceDaemonId: daemonId }),
  showConnection: false,
  setShowConnection: (v) => set({ showConnection: v }),
  showUsage: false,
  setShowUsage: (v) => set({ showUsage: v }),
  leftWidth: initialWidths.left,
  rightWidth: initialWidths.right,
  setLeftWidth: (px) => {
    const left = clamp(px, PANEL_LIMITS.left.min, PANEL_LIMITS.left.max);
    set({ leftWidth: left });
    persistWidths({ left, right: get().rightWidth });
  },
  setRightWidth: (px) => {
    const right = clamp(px, PANEL_LIMITS.right.min, PANEL_LIMITS.right.max);
    set({ rightWidth: right });
    persistWidths({ left: get().leftWidth, right });
  },
}));
