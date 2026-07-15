import { useRef } from "react";
import { useTranslation } from "react-i18next";

interface Props {
  /** Receives the pasted text; the caller writes it to the terminal. */
  onSubmit: (text: string) => void;
  onClose: () => void;
}

/**
 * Paste WITHOUT a clipboard read.
 *
 * `navigator.clipboard.readText()` needs a secure context and the daemon and relay
 * both serve plain HTTP today (see canReadClipboard), so on a phone the key bar's
 * Paste had nothing to read and no-opped in silence — next to a Copy that worked,
 * because copying still has its execCommand fallback.
 *
 * A `paste` EVENT, though, carries its own `clipboardData` in any context and asks
 * no permission: the OS hands the text over precisely because the user chose to
 * paste. So give the user something to paste INTO — a focused textarea — and
 * forward what lands there to the terminal. The gesture stays the platform's own
 * (iOS: tap or long-press → Paste; a hardware keyboard: ⌘V), so nothing here needs
 * to know what it is.
 */
export function PasteSheet({ onSubmit, onClose }: Props) {
  const { t } = useTranslation();
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const submit = (text: string) => {
    if (text) onSubmit(text);
    onClose();
  };

  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="modal paste-sheet">
        <div className="modal-title">{t("pasteSheet.title")}</div>
        {/* autoFocus so the keyboard (and iOS's Paste affordance) comes up on the
            same tap that opened the sheet — which is why the caller must decide to
            open it synchronously, while the gesture is still live. */}
        <textarea
          ref={inputRef}
          className="paste-sheet-input"
          autoFocus
          rows={3}
          placeholder={t("pasteSheet.hint")}
          onPaste={(e) => {
            // Read the text off the EVENT, not the textarea: its value only catches
            // up a tick later, and there is nothing to wait for — send and close.
            const text = e.clipboardData.getData("text");
            if (!text) return;
            e.preventDefault();
            submit(text);
          }}
        />
        <div className="modal-actions">
          <button className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          {/* Anything the paste event misses — dictation, a manual edit, an iOS
              paste that lands as an input rather than a paste — still gets sent. */}
          <button className="btn primary" onClick={() => submit(inputRef.current?.value ?? "")}>
            {t("pasteSheet.send")}
          </button>
        </div>
      </div>
    </div>
  );
}
