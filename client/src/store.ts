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

/** The origin of a fork, as the dialog needs to describe and submit it. */
export interface ForkSource {
  daemonId: string;
  sessionId: string;
  /** Shown so the user can see what they are forking. */
  title: string;
  /** The origin's agent — the fork's default, though it can be changed. */
  agentPluginId: string;
  /** The origin's branch, or null if it isn't on one (a direct checkout). */
  branch: string | null;
  /** Whether the origin is still running: a same-branch fork would share its
   *  working directory with a live agent, which the dialog warns about. */
  live: boolean;
  /**
   * Whether the origin's agent kept a conversation we can resume. With it, a
   * fork onto the same agent carries the whole conversation; without it — or onto
   * a different agent — it carries a written summary instead.
   */
  hasConversation: boolean;
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
  /**
   * The session being forked, when the dialog is open in fork mode. A fork
   * inherits its origin's daemon, workspace and place, so the dialog hides those
   * choices and offers only the two a fork actually has: which agent to hand the
   * work to, and whether to stay on the origin's branch or branch off it.
   */
  forkSource: ForkSource | null;
  openFork: (source: ForkSource) => void;
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
  /**
   * Mobile-only: the Details sheet (RightPanel) is open over the terminal
   * screen. Ignored by the desktop shell, which always shows the panel.
   */
  showDetails: boolean;
  setShowDetails: (v: boolean) => void;
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
      forkSource: null,
    }),
  forkSource: null,
  openFork: (source) =>
    set({
      showNewSession: true,
      newSessionDaemonId: source.daemonId,
      newSessionWorkspaceId: null,
      forkSource: source,
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
  showDetails: false,
  setShowDetails: (v) => set({ showDetails: v }),
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
