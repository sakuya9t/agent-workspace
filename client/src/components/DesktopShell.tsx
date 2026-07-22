import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { fitPanels, RESIZER_W, useUiStore } from "../store";
import { daemonLabel } from "../connectionStore";
import { useActiveSession } from "../useActiveSession";
import { USAGE_AGENTS } from "../agents";
import { statusLabel } from "../i18n/labels";
import { TerminalHandle, submitPrompt } from "../terminalTypes";
import { SessionList } from "./SessionList";
import { TerminalView } from "./Terminal";
import { RightPanel } from "./RightPanel";
import { PanelResizer } from "./PanelResizer";
import { UsageModal } from "./UsageModal";
import { DeckModeLink } from "./DeckModeLink";

/**
 * Desktop shell: the classic 3-pane workspace (session tree · terminal ·
 * details) with draggable resizers. Mounted by App for non-phone viewports;
 * the shared dialogs live in App so both shells reuse them.
 */
export function DesktopShell() {
  const { t } = useTranslation();
  const setShowConnection = useUiStore((s) => s.setShowConnection);
  const leftWidth = useUiStore((s) => s.leftWidth);
  const rightWidth = useUiStore((s) => s.rightWidth);
  const setLeftWidth = useUiStore((s) => s.setLeftWidth);
  const setRightWidth = useUiStore((s) => s.setRightWidth);
  const showUsage = useUiStore((s) => s.showUsage);
  const setShowUsage = useUiStore((s) => s.setShowUsage);
  const { states, reachable, totalLive, active, activeState, activeSession, target, live } =
    useActiveSession();
  const terminalHandleRef = useRef<TerminalHandle | null>(null);
  const onTerminalReady = useCallback((handle: TerminalHandle | null) => {
    terminalHandleRef.current = handle;
  }, []);
  const commitChanges = useCallback(() => {
    submitPrompt(terminalHandleRef.current, "commit the changes");
  }, []);

  // Fit the stored side-panel widths to the live viewport so the terminal keeps
  // a usable minimum. Tracks window resizes; the resizers drive off these
  // effective widths so a drag never starts from an off-screen value.
  const [viewportW, setViewportW] = useState(() => window.innerWidth);
  useEffect(() => {
    const onResize = () => setViewportW(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  const { left: effLeft, right: effRight } = fitPanels(leftWidth, rightWidth, viewportW);

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">{t("app.title")}</div>
        <div className="health">
          <span className={"dot " + (reachable > 0 ? "ok" : "bad")} />
          {t("app.daemonSummary", { count: states.length, reachable, live: totalLive })}
          <DeckModeLink />
          <button
            className="btn tiny conn-btn"
            onClick={() => setShowConnection(true)}
            title={t("app.manageTitle")}
          >
            {t("app.manage")}
          </button>
        </div>
      </header>

      <div
        className="workspace"
        style={{
          gridTemplateColumns: `${effLeft}px ${RESIZER_W}px minmax(0, 1fr) ${RESIZER_W}px ${effRight}px`,
        }}
      >
        <SessionList />

        <PanelResizer
          side="left"
          width={effLeft}
          onResize={setLeftWidth}
          label={t("app.resizeLeft")}
        />

        <div className="panel center">
          <div className="panel-header">
            {activeSession ? (
              <>
                <span className="mono">
                  {activeState && daemonLabel(activeState.daemon)} ·{" "}
                  {activeSession.agent_plugin_id} ·{" "}
                  {statusLabel(activeSession.status)}
                </span>
                {USAGE_AGENTS.has(activeSession.agent_plugin_id) && (
                  <button
                    className="btn tiny usage-link"
                    onClick={() => setShowUsage(true)}
                    title={t("app.viewUsageTitle")}
                  >
                    {t("app.viewUsage")}
                  </button>
                )}
              </>
            ) : (
              <span>{t("app.terminal")}</span>
            )}
          </div>
          <div className="panel-body terminal-body">
            {activeSession && target ? (
              <TerminalView
                key={active!.daemonId + ":" + activeSession.id}
                target={target}
                sessionId={activeSession.id}
                live={live}
                onReady={onTerminalReady}
              />
            ) : (
              <div className="empty big">{t("app.emptyTerminal")}</div>
            )}
          </div>
        </div>

        <PanelResizer
          side="right"
          width={effRight}
          onResize={setRightWidth}
          label={t("app.resizeRight")}
        />

        <RightPanel
          target={target}
          session={activeSession}
          onCommitChanges={live ? commitChanges : undefined}
        />
      </div>

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
