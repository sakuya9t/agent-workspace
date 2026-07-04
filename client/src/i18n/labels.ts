import i18n from "./index";
import { AttentionState, SessionStatus } from "../api";

/**
 * Enum → display-text lookups. The en values mirror the wire values so the UI
 * reads exactly as before; other locales can override them. Values the daemon
 * may add later (newer daemon than client) fall back to the raw string.
 */
export function statusLabel(s: SessionStatus): string {
  return i18n.t(`status.${s}`, { defaultValue: s });
}

/** A user-stopped session ended deliberately, not by failure — "finished". */
export function endedLabel(s: SessionStatus): string {
  return s === "stopped" ? i18n.t("status.finished") : statusLabel(s);
}

export function attentionLabel(a: AttentionState): string {
  return i18n.t(`attention.${a}`, { defaultValue: a });
}

const ISOLATION = ["worktree", "direct", "plain"] as const;

export function isolationLabel(v: string): string {
  return (ISOLATION as readonly string[]).includes(v)
    ? i18n.t(`isolation.${v as (typeof ISOLATION)[number]}`)
    : v;
}

const INSTANCE_STATUS = ["active", "released"] as const;

export function instanceStatusLabel(v: string): string {
  return (INSTANCE_STATUS as readonly string[]).includes(v)
    ? i18n.t(`instanceStatus.${v as (typeof INSTANCE_STATUS)[number]}`)
    : v;
}
