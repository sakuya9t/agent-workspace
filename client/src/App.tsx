import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useUiStore } from "./store";
import { targetOf } from "./connectionStore";
import { useDaemonStates } from "./useDaemons";
import { Session } from "./api";
import { statusLabel } from "./i18n/labels";
import { SessionList } from "./components/SessionList";
import { TerminalView } from "./components/Terminal";
import { RightPanel } from "./components/RightPanel";
import { NewSessionDialog } from "./components/NewSessionDialog";
import { NewWorkspaceDialog } from "./components/NewWorkspaceDialog";
import { ConnectionDialog } from "./components/ConnectionDialog";
import { UsageModal } from "./components/UsageModal";

function isLive(s: Session): boolean {
  return s.status === "running" || s.status === "starting";
}

/** Agents that persist usage transcripts the daemon can read (view-usage). */
const USAGE_AGENTS = new Set(["claude", "codex"]);

export function App() {
  const { t } = useTranslation();
  const active = useUiStore((s) => s.activeSession);
  const setShowConnection = useUiStore((s) => s.setShowConnection);
  const states = useDaemonStates();
  const [showUsage, setShowUsage] = useState(false);

  const reachable = states.filter((s) => s.daemon.connected && s.data).length;
  const totalLive = states.reduce(
    (n, s) => n + (s.daemon.connected ? (s.data?.sessions.filter(isLive).length ?? 0) : 0),
    0,
  );

  const activeState = active ? states.find((s) => s.daemon.id === active.daemonId) : undefined;
  const activeSession = activeState?.data?.sessions.find(
    (s) => s.id === active?.sessionId,
  );
  const target = activeState ? targetOf(activeState.daemon) : undefined;
  const live = activeSession ? isLive(activeSession) : false;

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">{t("app.title")}</div>
        <div className="health">
          <span className={"dot " + (reachable > 0 ? "ok" : "bad")} />
          {t("app.daemonSummary", { count: states.length, reachable, live: totalLive })}
          <button
            className="btn tiny conn-btn"
            onClick={() => setShowConnection(true)}
            title={t("app.manageTitle")}
          >
            {t("app.manage")}
          </button>
        </div>
      </header>

      <div className="workspace">
        <SessionList />

        <div className="panel center">
          <div className="panel-header">
            {activeSession ? (
              <>
                <span className="mono">
                  {activeState?.daemon.label} · {activeSession.agent_plugin_id} ·{" "}
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
              />
            ) : (
              <div className="empty big">{t("app.emptyTerminal")}</div>
            )}
          </div>
        </div>

        <RightPanel target={target} session={activeSession} />
      </div>

      {showUsage && activeSession && target && (
        <UsageModal
          target={target}
          sessionId={activeSession.id}
          agent={activeSession.agent_plugin_id}
          onClose={() => setShowUsage(false)}
        />
      )}

      <NewSessionDialog />
      <NewWorkspaceDialog />
      <ConnectionDialog />
    </div>
  );
}
