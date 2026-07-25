import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AttentionState } from "../api";
import { attentionLabel } from "../i18n/labels";
import { isBusy } from "../status";

interface Props {
  /** Session label as the row showed it — a mis-click must be recognizable. */
  name: string;
  /** Working or blocked: a turn is in flight, so stopping needs force. */
  busy: boolean;
  attention: AttentionState;
  pending: boolean;
  error: string | null;
  onCancel: () => void;
  onConfirm: (force: boolean) => void;
}

/**
 * Confirmation for stopping a session, replacing the old native `confirm()`
 * because the protected case needs a control the browser dialog can't carry.
 *
 * A session whose agent is *working* or *blocked on a prompt* has a turn in
 * flight; stopping it there throws that work away, and a session list is a wall
 * of small buttons. So for those the Stop button is dead until "force stop" is
 * ticked — the deliberate second act, not a second OK. Idle sessions get no
 * checkbox at all and confirm in one click, which is what keeps the checkbox
 * meaningful when it does appear.
 *
 * The daemon enforces the same rule (`stop_session` → 409), so this is the
 * affordance, not the protection: if the row was stale and the daemon says the
 * session is busy, the parent flips `busy` on and the checkbox appears here.
 */
export function StopSessionDialog({
  name,
  busy,
  attention,
  pending,
  error,
  onCancel,
  onConfirm,
}: Props) {
  const { t } = useTranslation();
  const [force, setForce] = useState(false);
  const blocked = busy && !force;

  return (
    <div className="modal-backdrop" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">{t("stopDialog.title", { name })}</div>

        {busy && (
          <div className="warn">
            {/* Name the state when we know it. When `busy` came from the
                daemon's 409 instead of the row, the row's own state is stale by
                definition, so don't quote it back as fact. */}
            {isBusy(attention)
              ? t("stopDialog.busyWarning", { state: attentionLabel(attention) })
              : t("stopDialog.busyWarningStale")}
          </div>
        )}

        <div className="dim small">{t("stopDialog.body")}</div>

        {busy && (
          <label className="checkbox danger">
            <input
              type="checkbox"
              checked={force}
              onChange={(e) => setForce(e.target.checked)}
            />
            <span>{t("stopDialog.forceLabel")}</span>
          </label>
        )}

        {error && <div className="error">{error}</div>}

        <div className="modal-actions">
          <button className="btn" onClick={onCancel}>
            {t("common.cancel")}
          </button>
          <button
            className="btn danger"
            disabled={blocked || pending}
            title={blocked ? t("stopDialog.forceRequired") : undefined}
            onClick={() => onConfirm(force)}
          >
            {pending
              ? t("stopDialog.stopping")
              : busy
                ? t("stopDialog.forceStop")
                : t("stopDialog.stop")}
          </button>
        </div>
      </div>
    </div>
  );
}
