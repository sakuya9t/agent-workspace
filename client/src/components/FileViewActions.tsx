import { useTranslation } from "react-i18next";
import { DiffMode } from "./DiffModal";

/**
 * The pair of ways to open one changed file: its diff, or the whole file. Shared
 * by the source-control changeset and the commit-detail file list so both offer
 * the same two choices in the same place.
 *
 * The row around these buttons is itself clickable (opening the diff, the common
 * case), so each button stops the click from reaching it — otherwise the row
 * would re-open in diff mode over the file mode the button just asked for.
 */
export function FileViewActions({ onOpen }: { onOpen: (mode: DiffMode) => void }) {
  const { t } = useTranslation();
  const open = (e: React.MouseEvent, mode: DiffMode) => {
    e.stopPropagation();
    onOpen(mode);
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
    </span>
  );
}
