import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useUiStore } from "./store";
import { useIsPhone } from "./useIsPhone";
import { useActiveSession } from "./useActiveSession";
import { useTabAlert } from "./useTabAlert";
import { DesktopShell } from "./components/DesktopShell";
import { MobileShell } from "./components/MobileShell";
import { NewSessionDialog } from "./components/NewSessionDialog";
import { NewWorkspaceDialog } from "./components/NewWorkspaceDialog";
import { ConnectionDialog } from "./components/ConnectionDialog";

/**
 * Root: pick the shell for the device class, then render the shared dialogs
 * once so both shells reuse them. All server data and UI state live in
 * queries/stores, so crossing the phone↔desktop boundary swaps shells with
 * nothing lost.
 */
export function App() {
  const { t } = useTranslation();
  const isPhone = useIsPhone();

  // Blink the tab title while any session is blocked and waiting on the user, so
  // it's noticeable even when this tab is in the background. Driven from the root
  // so it holds across both shells and regardless of which session is selected.
  const { blocked } = useActiveSession();
  useTabAlert(blocked, t("app.blockedTab", { count: blocked }), t("app.title"));

  // Deep link: honor #s=<daemonId>:<sessionId> on first load so a shared URL
  // opens straight into that session. Works on both shells; the mobile shell
  // additionally pushes this hash as it navigates (see useMobileHistory).
  useEffect(() => {
    const m = /^#s=([^:]+):(.+)$/.exec(window.location.hash);
    if (m) useUiStore.getState().setActive({ daemonId: m[1], sessionId: m[2] });
  }, []);

  return (
    <>
      {isPhone ? <MobileShell /> : <DesktopShell />}
      <NewSessionDialog />
      <NewWorkspaceDialog />
      <ConnectionDialog />
    </>
  );
}
