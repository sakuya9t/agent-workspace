import { useUiStore } from "./store";
import { targetOf } from "./connectionStore";
import { useDaemonStates } from "./useDaemons";
import { Session } from "./api";
import { SessionList } from "./components/SessionList";
import { TerminalView } from "./components/Terminal";
import { RightPanel } from "./components/RightPanel";
import { NewSessionDialog } from "./components/NewSessionDialog";
import { ConnectionDialog } from "./components/ConnectionDialog";

function isLive(s: Session): boolean {
  return s.status === "running" || s.status === "starting";
}

export function App() {
  const active = useUiStore((s) => s.activeSession);
  const setShowConnection = useUiStore((s) => s.setShowConnection);
  const states = useDaemonStates();

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
        <div className="brand">Agent Session Manager</div>
        <div className="health">
          <span className={"dot " + (reachable > 0 ? "ok" : "bad")} />
          {states.length} daemon{states.length === 1 ? "" : "s"} · {reachable} reachable ·{" "}
          {totalLive} live
          <button
            className="btn tiny conn-btn"
            onClick={() => setShowConnection(true)}
            title="Connect / manage daemons"
          >
            manage
          </button>
        </div>
      </header>

      <div className="workspace">
        <SessionList />

        <div className="panel center">
          <div className="panel-header">
            {activeSession ? (
              <span className="mono">
                {activeState?.daemon.label} · {activeSession.agent_plugin_id} ·{" "}
                {activeSession.status}
              </span>
            ) : (
              <span>Terminal</span>
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
              <div className="empty big">
                Select or create a session to open its terminal.
              </div>
            )}
          </div>
        </div>

        <RightPanel target={target} session={activeSession} />
      </div>

      <NewSessionDialog />
      <ConnectionDialog />
    </div>
  );
}
