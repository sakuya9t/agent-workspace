import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useUiStore } from "../store";
import { daemonLabel } from "../connectionStore";
import { useActiveSession } from "../useActiveSession";
import { useVisualViewportHeight } from "../useVisualViewportHeight";
import { USAGE_AGENTS } from "../agents";
import { CtrlLatch, TerminalHandle } from "../terminalTypes";
import { statusLabel } from "../i18n/labels";
import { SessionList } from "./SessionList";
import { TerminalView } from "./Terminal";
import { TermKeyBar } from "./TermKeyBar";
import { RightPanel } from "./RightPanel";
import { UsageModal } from "./UsageModal";

/**
 * Mirror the mobile UI layer stack (home → terminal → details sheet) onto the
 * browser history stack, so the Android back gesture / iOS edge-swipe closes
 * the top-most layer instead of leaving the app. One reconciliation path: every
 * backward move — the system back button AND our own back affordances, which
 * call `history.back()` — arrives as a popstate and pops exactly one layer.
 */
function useMobileHistory() {
  const active = useUiStore((s) => s.activeSession);
  const showDetails = useUiStore((s) => s.showDetails);
  // 0 = home, 1 = terminal, 2 = terminal + details sheet.
  const depth = (active ? 1 : 0) + (active && showDetails ? 1 : 0);
  const synced = useRef(0);

  useEffect(() => {
    if (depth > synced.current) {
      // Grew: push one entry per new layer. The terminal entry carries the
      // deep-link hash so the URL is shareable and reload-safe.
      for (let d = synced.current + 1; d <= depth; d++) {
        const url =
          d === 1 && active ? `#s=${active.daemonId}:${active.sessionId}` : undefined;
        window.history.pushState({ asmDepth: d }, "", url);
      }
    } else if (depth === 0 && window.location.hash) {
      // Returned home without a system-back (e.g. a takeover cleared the active
      // session): tidy the deep-link hash. Any stray forward entry is harmless
      // — the next back press is simply absorbed.
      window.history.replaceState(null, "", window.location.pathname + window.location.search);
    }
    synced.current = depth;
  }, [depth, active]);

  useEffect(() => {
    const onPop = () => {
      const s = useUiStore.getState();
      if (s.activeSession && s.showDetails) s.setShowDetails(false);
      else if (s.activeSession) s.setActive(null);
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);
}

/**
 * Mobile shell: two screens plus one sheet. The session list is home; a tapped
 * session pushes to a full-screen terminal; Details/Git is a sheet **over** the
 * terminal (so its WebSocket stays mounted underneath). All panels, dialogs,
 * stores, and queries are the same components the desktop shell uses — parity
 * is structural. Touch-target/sheet CSS is phase 2; the key bar is phase 3.
 */
export function MobileShell() {
  const { t } = useTranslation();
  const active = useUiStore((s) => s.activeSession);
  const showUsage = useUiStore((s) => s.showUsage);
  const setShowUsage = useUiStore((s) => s.setShowUsage);
  const showDetails = useUiStore((s) => s.showDetails);
  const setShowDetails = useUiStore((s) => s.setShowDetails);
  const { states, reachable, totalLive, activeState, activeSession, target, live } =
    useActiveSession();

  useMobileHistory();

  // Terminal input handle (set by TerminalView.onReady) that the key bar reads.
  const handleRef = useRef<TerminalHandle | null>(null);
  // Ctrl latch: React state drives the key-bar visual; the ref lets
  // TerminalView read it per keystroke without an effect dependency.
  const [ctrl, setCtrl] = useState<CtrlLatch>("off");
  const ctrlRef = useRef<CtrlLatch>("off");
  ctrlRef.current = ctrl;
  const consumeCtrl = useCallback(() => setCtrl((c) => (c === "armed" ? "off" : c)), []);
  useEffect(() => {
    if (!live) setCtrl("off");
  }, [live]);

  // Track the visual viewport so the key bar stays above the soft keyboard.
  const vh = useVisualViewportHeight();
  const shellStyle = vh != null ? { height: `${vh}px` } : undefined;

  // Every backward move goes through the browser so popstate is the one place
  // that unwinds a layer (see useMobileHistory).
  const back = () => window.history.back();

  // Home screen — the session tree + history, with a compact status header.
  if (!active) {
    return (
      <div className="mobile-shell" style={shellStyle}>
        <header className="mobile-home-header">
          <span className="brand">{t("app.title")}</span>
          <span className="health">
            <span className={"dot " + (reachable > 0 ? "ok" : "bad")} />
            {t("app.daemonSummary", { count: states.length, reachable, live: totalLive })}
          </span>
        </header>
        <div className="mobile-home-body">
          <SessionList />
        </div>
      </div>
    );
  }

  // Terminal screen — full-screen xterm with the details sheet as an overlay.
  return (
    <div className="mobile-shell" style={shellStyle}>
      <header className="mobile-term-header">
        <button className="mobile-back" onClick={back} aria-label={t("mobile.back")} />
        <span className="mobile-term-title mono">
          {activeSession ? (
            <>
              {activeState && daemonLabel(activeState.daemon)} ·{" "}
              {activeSession.agent_plugin_id} · {statusLabel(activeSession.status)}
            </>
          ) : (
            t("app.terminal")
          )}
        </span>
        {activeSession && USAGE_AGENTS.has(activeSession.agent_plugin_id) && (
          <button
            className="btn tiny usage-link"
            onClick={() => setShowUsage(true)}
            title={t("app.viewUsageTitle")}
          >
            {t("app.viewUsage")}
          </button>
        )}
        <button
          className="btn tiny mobile-details-btn"
          onClick={() => setShowDetails(true)}
          aria-label={t("mobile.details")}
          title={t("mobile.details")}
        />
      </header>

      <div className="mobile-term-body">
        {activeSession && target ? (
          <TerminalView
            key={active.daemonId + ":" + activeSession.id}
            target={target}
            sessionId={activeSession.id}
            live={live}
            onReady={(h) => (handleRef.current = h)}
            ctrlRef={ctrlRef}
            onCtrlConsumed={consumeCtrl}
          />
        ) : (
          <div className="empty big">{t("app.emptyTerminal")}</div>
        )}
      </div>

      {live && <TermKeyBar handleRef={handleRef} ctrl={ctrl} setCtrl={setCtrl} />}

      {showDetails && (
        <div className="details-sheet-backdrop" onClick={back}>
          <div
            className="details-sheet"
            role="dialog"
            aria-label={t("rightPanel.header")}
            onClick={(e) => e.stopPropagation()}
          >
            <button className="details-sheet-handle" onClick={back} aria-label={t("common.close")} />
            <RightPanel target={target} session={activeSession} />
          </div>
        </div>
      )}

      {showUsage && activeSession && target && (
        <UsageModal
          target={target}
          sessionId={activeSession.id}
          agent={activeSession.agent_plugin_id}
          onClose={() => setShowUsage(false)}
        />
      )}
    </div>
  );
}
