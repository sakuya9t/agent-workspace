import { useTranslation } from "react-i18next";
import { DiffMode } from "./DiffModal";

/**
 * Actions for one file. Both working-tree and commit-detail rows offer its diff
 * and whole-file view; working-tree rows additionally provide discard.
 *
 * The row around these buttons is itself clickable (opening the diff, the common
 * case), so each button stops the click from reaching it — otherwise the row
 * would re-open in diff mode over the file mode the button just asked for.
 */
export function FileViewActions({
  onOpen,
  onDiscard,
  discardDisabled = false,
}: {
  onOpen: (mode: DiffMode) => void;
  /** Present only for working-tree rows; commit history is immutable. */
  onDiscard?: () => void;
  discardDisabled?: boolean;
}) {
  const { t } = useTranslation();
  const open = (e: React.MouseEvent, mode: DiffMode) => {
    e.stopPropagation();
    onOpen(mode);
  };
  const discard = (e: React.MouseEvent) => {
    e.stopPropagation();
    onDiscard?.();
  };
  return (
    <span className="file-actions">
      <button
        className="icon-btn"
        onClick={(e) => open(e, "diff")}
        title={t("rightPanel.viewDiff")}
        aria-label={t("rightPanel.viewDiff")}
      >
        <span className="action-icon action-icon-diff" aria-hidden="true" />
      </button>
      <button
        className="icon-btn"
        onClick={(e) => open(e, "file")}
        title={t("rightPanel.viewWholeFile")}
        aria-label={t("rightPanel.viewWholeFile")}
      >
        <span className="action-icon action-icon-file" aria-hidden="true" />
      </button>
      {onDiscard && (
        <button
          className="icon-btn discard-file-btn"
          disabled={discardDisabled}
          onClick={discard}
          title={t("rightPanel.discardChanges")}
          aria-label={t("rightPanel.discardChanges")}
        >
          <span className="action-icon action-icon-discard" aria-hidden="true" />
        </button>
      )}
    </span>
  );
}
