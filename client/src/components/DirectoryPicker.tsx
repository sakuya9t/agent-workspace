import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { Target } from "../connectionStore";

interface Props {
  target: Target;
  initialPath?: string;
  title?: string;
  onPick: (path: string) => void;
  onClose: () => void;
}

/**
 * Browses the daemon host's filesystem so the user can pick a directory
 * without typing the full path. The working directory lives on the server,
 * so this uses the daemon's /api/fs/list rather than a native file dialog.
 */
export function DirectoryPicker({ target, initialPath, title, onPick, onClose }: Props) {
  const { t } = useTranslation();
  // `path` empty means "let the daemon default to home".
  const [path, setPath] = useState(initialPath ?? "");
  const [showHidden, setShowHidden] = useState(false);
  const [manual, setManual] = useState(initialPath ?? "");
  // Single-clicked entry in the list; takes precedence over the listed
  // directory when confirming, so "c" clicked inside /a/b picks /a/b/c.
  const [selected, setSelected] = useState<string | null>(null);

  const navigate = (p: string) => {
    setSelected(null);
    setPath(p);
  };

  const { data, error, isFetching } = useQuery({
    queryKey: ["fs", target.baseUrl, path, showHidden],
    queryFn: () => api.fsList(target, path, showHidden),
    retry: false,
  });

  // Keep the editable path box in sync with wherever we navigated.
  useEffect(() => {
    if (data?.path) setManual(data.path);
  }, [data?.path]);

  const current = data?.path ?? path;
  const chosen = selected ?? current;

  return (
    <div
      className="modal-backdrop"
      onClick={(e) => {
        // Nested inside the new-session dialog's backdrop; don't let the click
        // bubble up and close the parent dialog too.
        e.stopPropagation();
        onClose();
      }}
    >
      <div className="modal picker-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">
          <span>{title ?? t("directoryPicker.defaultTitle")}</span>
          <button className="btn tiny" onClick={onClose}>
            {t("common.close")}
          </button>
        </div>

        <div className="picker-path-row">
          <input
            className="input mono"
            value={manual}
            spellCheck={false}
            onChange={(e) => {
              setSelected(null);
              setManual(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") navigate(manual);
            }}
            placeholder={t("directoryPicker.pathPlaceholder")}
          />
          <button className="btn" onClick={() => navigate(manual)}>
            {t("directoryPicker.goBtn")}
          </button>
        </div>

        <div className="picker-toolbar">
          <button
            className="btn tiny"
            disabled={!data?.parent}
            onClick={() => data?.parent && navigate(data.parent)}
          >
            {t("directoryPicker.upBtn")}
          </button>
          <button className="btn tiny" onClick={() => navigate("")}>
            {t("directoryPicker.homeBtn")}
          </button>
          <label className="checkbox small">
            <input
              type="checkbox"
              checked={showHidden}
              onChange={(e) => setShowHidden(e.target.checked)}
            />
            {t("directoryPicker.hidden")}
          </label>
          {isFetching && <span className="dim small">{t("directoryPicker.loading")}</span>}
        </div>

        {error && <div className="error">{String(error)}</div>}

        <div className="picker-list">
          {data?.entries.length === 0 && (
            <div className="dim small">{t("directoryPicker.noSubdirs")}</div>
          )}
          {data?.entries.map((e) => (
            <div
              key={e.path}
              className={`picker-entry${selected === e.path ? " selected" : ""}`}
              onDoubleClick={() => navigate(e.path)}
              onClick={() => {
                setSelected(e.path);
                setManual(e.path);
              }}
              title={t("directoryPicker.entryTitle")}
            >
              <span className="picker-icon">{e.is_git ? "◆" : "▸"}</span>
              <span className="mono">{e.name}</span>
              {e.is_git && <span className="git-tag">{t("common.git")}</span>}
            </div>
          ))}
        </div>

        <div className="modal-actions">
          <button className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            className="btn primary"
            disabled={!chosen}
            onClick={() => onPick(chosen)}
          >
            {t("directoryPicker.useFolder")}
          </button>
        </div>
      </div>
    </div>
  );
}
