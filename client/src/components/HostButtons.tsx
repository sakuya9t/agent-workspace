import { useTranslation } from "react-i18next";
import { useUiStore } from "../store";
import { ServersIcon, SlidersIcon } from "./icons";

// The two host-level actions, as the same icon-only buttons wherever they
// appear (desktop top bar, session list header on both shells). The label is
// each button's accessible name and shows as a tip on hover / keyboard focus.

/** Opens the Connections dialog (daemons and relays). */
export function ManageButton() {
  const { t } = useTranslation();
  const setShowConnection = useUiStore((s) => s.setShowConnection);
  return (
    <button
      type="button"
      className="tip-icon-btn manage-btn"
      onClick={() => setShowConnection(true)}
      aria-label={t("app.manage")}
      data-tip={t("app.manage")}
    >
      <ServersIcon />
    </button>
  );
}

/** Opens the startup-settings (Config) dialog. */
export function ConfigButton() {
  const { t } = useTranslation();
  const setShowConfig = useUiStore((s) => s.setShowConfig);
  return (
    <button
      type="button"
      className="tip-icon-btn config-btn"
      onClick={() => setShowConfig(true)}
      aria-label={t("app.config")}
      data-tip={t("app.config")}
    >
      <SlidersIcon />
    </button>
  );
}
