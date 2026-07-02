import { useQuery } from "@tanstack/react-query";
import { api } from "./api";
import { useUiStore } from "./store";
import { useConnStore } from "./connectionStore";
import { SessionList } from "./components/SessionList";
import { TerminalView } from "./components/Terminal";
import { RightPanel } from "./components/RightPanel";
import { NewSessionDialog } from "./components/NewSessionDialog";
import { ConnectionDialog } from "./components/ConnectionDialog";

export function App() {
  const activeId = useUiStore((s) => s.activeSessionId);
  const setShowConnection = useUiStore((s) => s.setShowConnection);
  const conn = useConnStore();

  const { data: health } = useQuery({
    queryKey: ["health"],
    queryFn: api.health,
    refetchInterval: 5000,
  });

  const { data: sessions } = useQuery({
    queryKey: ["sessions"],
    queryFn: api.listSessions,
    refetchInterval: 1500,
  });

  const active = sessions?.find((s) => s.id === activeId);
  const live = active?.status === "running" || active?.status === "starting";

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">Agent Session Manager</div>
        <div className="health">
          {health ? (
            <>
              <span className="dot ok" /> {health.hostname} · {health.platform} · v
              {health.version} · {health.active_sessions} live
            </>
          ) : (
            <>
              <span className="dot bad" /> daemon unreachable
            </>
          )}
          <button
            className="btn tiny conn-btn"
            onClick={() => setShowConnection(true)}
            title="Change which daemon you're connected to"
          >
            {conn.baseUrl ? `remote: ${hostLabel(conn.label, conn.baseUrl)}` : "local"}
            {conn.baseUrl && conn.token ? " 🔒" : ""}
          </button>
        </div>
      </header>

      <div className="workspace">
        <SessionList />

        <div className="panel center">
          <div className="panel-header">
            {active ? (
              <span className="mono">
                {active.agent_plugin_id} · {active.status}
              </span>
            ) : (
              <span>Terminal</span>
            )}
          </div>
          <div className="panel-body terminal-body">
            {active ? (
              <TerminalView key={active.id} sessionId={active.id} live={!!live} />
            ) : (
              <div className="empty big">
                Select or create a session to open its terminal.
              </div>
            )}
          </div>
        </div>

        <RightPanel session={active} />
      </div>

      <NewSessionDialog />
      <ConnectionDialog />
    </div>
  );
}

function hostLabel(label: string | null, baseUrl: string): string {
  if (label) return label;
  try {
    return new URL(baseUrl).host;
  } catch {
    return baseUrl;
  }
}
