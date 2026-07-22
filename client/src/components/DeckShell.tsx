import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Session, api } from "../api";
import { daemonLabel, Target, targetOf } from "../connectionStore";
import { needsAttention } from "../status";
import { useDaemonStates } from "../useDaemons";

type LayoutId = "2x4" | "4x4" | "4x6";
type DeckLayout = { id: LayoutId; rows: number; cols: number };
type DeckSession = {
  daemonId: string;
  daemonName: string;
  target: Target;
  session: Session;
};

const LAYOUTS: DeckLayout[] = [
  { id: "2x4", rows: 2, cols: 4 },
  { id: "4x4", rows: 4, cols: 4 },
  { id: "4x6", rows: 4, cols: 6 },
];
const LAYOUT_KEY = "asm.deckLayout";

function initialLayout(): LayoutId {
  try {
    const value = localStorage.getItem(LAYOUT_KEY);
    if (LAYOUTS.some((layout) => layout.id === value)) return value as LayoutId;
  } catch {
    // Storage is optional; the compact physical-deck default is still usable.
  }
  return "4x4";
}

/**
 * Mobile-first Stream Deck controller. The UI maps a pure grid of named button
 * actions; the daemon owns approval parsing/response, which is the same contract
 * a physical deck integration can call later.
 */
export function DeckShell() {
  const { t } = useTranslation();
  const states = useDaemonStates();
  const queryClient = useQueryClient();
  const [layoutId, setLayoutId] = useState<LayoutId>(initialLayout);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [page, setPage] = useState(0);
  const [selected, setSelected] = useState<DeckSession | null>(null);
  const layout = LAYOUTS.find((item) => item.id === layoutId) ?? LAYOUTS[1];

  const { controlled, idleCount, reachable } = useMemo(() => {
    const visible: DeckSession[] = [];
    let idle = 0;
    let online = 0;
    for (const state of states) {
      if (!state.daemon.connected || !state.data) continue;
      online += 1;
      for (const session of state.data.sessions) {
        if (session.status !== "running" && session.status !== "starting") continue;
        const blocked = needsAttention(session.attention_state);
        const working = session.attention_state === "activity" || session.status === "starting";
        if (blocked || working) {
          visible.push({
            daemonId: state.daemon.id,
            daemonName: daemonLabel(state.daemon),
            target: targetOf(state.daemon),
            session,
          });
        } else {
          idle += 1;
        }
      }
    }
    visible.sort((a, b) => {
      const attentionDelta = Number(needsAttention(b.session.attention_state)) -
        Number(needsAttention(a.session.attention_state));
      return attentionDelta || b.session.last_activity_at - a.session.last_activity_at;
    });
    return { controlled: visible, idleCount: idle, reachable: online };
  }, [states]);

  // If a poll says the selected approval is gone, return to the overview.
  useEffect(() => {
    if (!selected) return;
    const current = controlled.find(
      (item) => item.daemonId === selected.daemonId && item.session.id === selected.session.id,
    );
    if (!current || !needsAttention(current.session.attention_state)) setSelected(null);
  }, [controlled, selected]);

  const saveLayout = (id: LayoutId) => {
    setLayoutId(id);
    setPage(0);
    setSettingsOpen(false);
    try {
      localStorage.setItem(LAYOUT_KEY, id);
    } catch {
      // Keep the in-memory selection when persistence is unavailable.
    }
  };

  const promptQuery = useQuery({
    queryKey: ["deck-prompt", selected?.daemonId, selected?.session.id],
    queryFn: () => api.deckPrompt(selected!.target, selected!.session.id),
    enabled: Boolean(selected),
    refetchInterval: selected ? 1000 : false,
    retry: false,
  });
  const respond = useMutation({
    mutationFn: (optionId: number) =>
      api.respondToDeckPrompt(
        selected!.target,
        selected!.session.id,
        promptQuery.data!.revision,
        optionId,
      ),
    onSuccess: () => {
      setSelected(null);
      void queryClient.invalidateQueries({ queryKey: ["daemon"] });
    },
    onError: () => void promptQuery.refetch(),
  });

  const openTerminal = (item: DeckSession) => {
    window.location.assign(`/#s=${item.daemonId}:${item.session.id}`);
  };

  return (
    <main className="deck-shell">
      <header className="deck-header">
        <a href="/" className="deck-exit" title={t("deck.exitTitle")}>
          <span className="deck-exit-chevron" aria-hidden="true" />
          <span>{t("deck.manager")}</span>
        </a>
        <div className="deck-wordmark">
          <span className="deck-wordmark-mark" aria-hidden="true">
            <i /><i /><i /><i />
          </span>
          <span>{t("deck.title")}</span>
        </div>
        <div className="deck-head-actions">
          <span className="deck-online" title={t("deck.onlineTitle", { count: reachable })}>
            <i className={reachable ? "online" : ""} />
            {reachable}
          </span>
          <button
            className="deck-layout-button"
            onClick={() => setSettingsOpen((open) => !open)}
            aria-expanded={settingsOpen}
            aria-label={t("deck.chooseLayout")}
          >
            {layout.rows} × {layout.cols}
          </button>
        </div>
        {settingsOpen && (
          <div className="deck-layout-menu" role="menu" aria-label={t("deck.layouts")}>
            {LAYOUTS.map((item) => (
              <button
                key={item.id}
                className={item.id === layoutId ? "active" : ""}
                onClick={() => saveLayout(item.id)}
                role="menuitem"
              >
                <span className="deck-mini-grid" style={{ gridTemplateColumns: `repeat(${item.cols}, 1fr)` }}>
                  {Array.from({ length: item.rows * item.cols }, (_, i) => <i key={i} />)}
                </span>
                {item.rows} × {item.cols}
              </button>
            ))}
          </div>
        )}
      </header>

      {selected ? (
        <ApprovalDeck
          item={selected}
          layout={layout}
          prompt={promptQuery.data}
          loading={promptQuery.isLoading}
          error={promptQuery.error ?? respond.error}
          responding={respond.isPending}
          onBack={() => setSelected(null)}
          onTerminal={() => openTerminal(selected)}
          onRespond={(optionId) => respond.mutate(optionId)}
          t={t}
        />
      ) : (
        <OverviewDeck
          controlled={controlled}
          idleCount={idleCount}
          layout={layout}
          page={page}
          setPage={setPage}
          onSession={(item) =>
            needsAttention(item.session.attention_state) ? setSelected(item) : openTerminal(item)
          }
          t={t}
        />
      )}
    </main>
  );
}

type Translate = ReturnType<typeof useTranslation>["t"];

function OverviewDeck({
  controlled,
  idleCount,
  layout,
  page,
  setPage,
  onSession,
  t,
}: {
  controlled: DeckSession[];
  idleCount: number;
  layout: DeckLayout;
  page: number;
  setPage: (page: number) => void;
  onSession: (item: DeckSession) => void;
  t: Translate;
}) {
  const cells = layout.rows * layout.cols;
  const paged = controlled.length > cells - 1;
  const sessionCells = Math.max(1, cells - 1 - (paged ? 2 : 0));
  const pages = Math.max(1, Math.ceil(controlled.length / sessionCells));
  const safePage = Math.min(page, pages - 1);
  const visible = controlled.slice(safePage * sessionCells, (safePage + 1) * sessionCells);
  const used = visible.length + 1 + (paged ? 2 : 0);

  useEffect(() => {
    if (safePage !== page) setPage(safePage);
  }, [page, safePage, setPage]);

  return (
    <section className="deck-stage" aria-label={t("deck.sessions")}>
      <div className="deck-overview-label">
        <span>{t("deck.liveControl")}</span>
        <span>{t("deck.page", { current: safePage + 1, total: pages })}</span>
      </div>
      <div
        className="deck-grid"
        style={{
          gridTemplateColumns: `repeat(${layout.cols}, minmax(0, 1fr))`,
          gridTemplateRows: `repeat(${layout.rows}, minmax(0, 1fr))`,
          aspectRatio: `${layout.cols} / ${layout.rows}`,
        }}
      >
        {visible.map((item) => {
          const blocked = needsAttention(item.session.attention_state);
          const title = sessionTitle(item.session);
          return (
            <DeckButton
              key={`${item.daemonId}:${item.session.id}`}
              tone={blocked ? "blocked" : "working"}
              actionId={`session:${item.daemonId}:${item.session.id}`}
              label={t(blocked ? "deck.openBlocked" : "deck.openWorking", { title })}
              onClick={() => onSession(item)}
            >
              <span className="deck-tile-topline">
                <span className={`deck-state-glyph ${blocked ? "blocked" : "working"}`} />
                <span>{t(blocked ? "deck.blocked" : "deck.working")}</span>
              </span>
              <Marquee text={title} />
              <span className="deck-tile-meta">{item.session.agent_plugin_id} · {item.daemonName}</span>
            </DeckButton>
          );
        })}
        <DeckButton tone="idle" actionId="summary:idle" label={t("deck.idleCount", { count: idleCount })}>
          <span className="deck-idle-number">{idleCount}</span>
          <span className="deck-idle-label">{t("deck.idle")}</span>
          <span className="deck-tile-meta">{t("deck.quietSessions")}</span>
        </DeckButton>
        {paged && (
          <>
            <DeckButton
              tone="nav"
              actionId="page:previous"
              label={t("deck.previousPage")}
              disabled={safePage === 0}
              onClick={() => setPage(Math.max(0, safePage - 1))}
            >
              <span className="deck-nav-arrow deck-nav-arrow--previous" aria-hidden="true" />
              <span>{t("deck.previous")}</span>
            </DeckButton>
            <DeckButton
              tone="nav"
              actionId="page:next"
              label={t("deck.nextPage")}
              disabled={safePage === pages - 1}
              onClick={() => setPage(Math.min(pages - 1, safePage + 1))}
            >
              <span className="deck-nav-arrow deck-nav-arrow--next" aria-hidden="true" />
              <span>{t("deck.next")}</span>
            </DeckButton>
          </>
        )}
        {Array.from({ length: Math.max(0, cells - used) }, (_, i) => (
          <div className="deck-empty-key" key={`empty-${i}`} aria-hidden="true" />
        ))}
      </div>
    </section>
  );
}

function ApprovalDeck({
  item,
  layout,
  prompt,
  loading,
  error,
  responding,
  onBack,
  onTerminal,
  onRespond,
  t,
}: {
  item: DeckSession;
  layout: DeckLayout;
  prompt: Awaited<ReturnType<typeof api.deckPrompt>> | undefined;
  loading: boolean;
  error: Error | null;
  responding: boolean;
  onBack: () => void;
  onTerminal: () => void;
  onRespond: (id: number) => void;
  t: Translate;
}) {
  const cells = layout.rows * layout.cols;
  const detail = prompt?.detail || item.session.attention_reason || "";
  return (
    <section className="deck-stage deck-approval-stage" aria-label={t("deck.approval")}>
      <div className="deck-prompt-card">
        <div className="deck-prompt-eyebrow">
          <span className="deck-state-glyph blocked" />
          {t("deck.needsYou")} · {sessionTitle(item.session)}
        </div>
        <h1>{prompt?.question ?? (loading ? t("deck.readingPrompt") : t("deck.inputNeeded"))}</h1>
        {detail && (
          <div className="deck-prompt-why">
            <span>{t("deck.why")}</span>
            <pre>{detail}</pre>
          </div>
        )}
        {error && <div className="deck-prompt-error">{error.message}</div>}
      </div>
      <div
        className="deck-grid deck-action-grid"
        style={{
          gridTemplateColumns: `repeat(${layout.cols}, minmax(0, 1fr))`,
          gridTemplateRows: `repeat(${layout.rows}, minmax(0, 1fr))`,
          aspectRatio: `${layout.cols} / ${layout.rows}`,
        }}
      >
        <DeckButton tone="nav" actionId="approval:back" label={t("deck.back")} onClick={onBack}>
          <span className="deck-nav-arrow deck-nav-arrow--previous" aria-hidden="true" />
          <span>{t("deck.back")}</span>
        </DeckButton>
        {prompt?.options.map((option) => (
          <DeckButton
            key={option.id}
            tone={optionTone(option.label)}
            actionId={`approval:option:${option.id}`}
            label={option.label}
            disabled={responding}
            onClick={() => onRespond(option.id)}
          >
            <span className="deck-option-number">{option.id}</span>
            <span className="deck-option-label">{option.label}</span>
            {option.id === prompt.selected && (
              <span className="deck-selected-label">{t("deck.selected")}</span>
            )}
          </DeckButton>
        ))}
        {!loading && !prompt && (
          <DeckButton tone="terminal" actionId="approval:terminal" label={t("deck.openTerminal")} onClick={onTerminal}>
            <span className="deck-terminal-icon" aria-hidden="true" />
            <span>{t("deck.openTerminal")}</span>
          </DeckButton>
        )}
        {Array.from({ length: Math.max(0, cells - 1 - (prompt?.options.length ?? 1)) }, (_, i) => (
          <div className="deck-empty-key" key={`approval-empty-${i}`} aria-hidden="true" />
        ))}
      </div>
    </section>
  );
}

function DeckButton({
  tone,
  actionId,
  label,
  disabled,
  onClick,
  children,
}: {
  tone: string;
  actionId: string;
  label: string;
  disabled?: boolean;
  onClick?: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      className={`deck-key deck-key--${tone}`}
      data-deck-action={actionId}
      aria-label={label}
      disabled={disabled || !onClick}
      onClick={onClick}
    >
      <span className="deck-key-face">{children}</span>
    </button>
  );
}

function Marquee({ text }: { text: string }) {
  return (
    <span className="deck-marquee" title={text}>
      <span className="deck-marquee-track">
        <span>{text}</span><span aria-hidden="true">{text}</span>
      </span>
    </span>
  );
}

function optionTone(label: string): "approve" | "persistent" | "deny" | "choice" {
  const lower = label.toLowerCase();
  if (lower.startsWith("no") || lower.includes("cancel") || lower.includes("reject")) return "deny";
  if (lower.includes("always") || lower.includes("don't ask") || lower.includes("bypass")) return "persistent";
  if (lower.startsWith("yes") || lower.includes("proceed") || lower.includes("approve")) return "approve";
  return "choice";
}

function sessionTitle(session: Session): string {
  if (session.title) return session.title;
  const parts = session.working_directory.split(/[/\\]/).filter(Boolean);
  return parts.at(-1) ?? session.agent_plugin_id;
}
