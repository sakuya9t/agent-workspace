import { MutableRefObject, useState } from "react";
import { useTranslation } from "react-i18next";
import { CtrlLatch, TerminalHandle } from "../terminalTypes";

interface Props {
  /** Live handle onto the mounted terminal (see TerminalView.onReady). */
  handleRef: MutableRefObject<TerminalHandle | null>;
  /** The panel's own Ctrl latch, owned by TerminalView (the desktop shell hands
   *  it none, unlike the phone). Applied to the next soft-keyboard key. */
  ctrl: CtrlLatch;
  onCycleCtrl: () => void;
  onCopy: () => void;
  onPaste: () => void;
  onAttach: () => void;
}

/**
 * iPad-only, live-sessions-only control panel: a single corner toggle that
 * expands into the full set of keys a TUI needs (Esc/Tab/arrows/Ctrl/^C/…) plus
 * Copy/Paste/Attach, then collapses back to the lone toggle so the terminal
 * stays clean. A tablet takes the *desktop* shell (see useIsPhone), so it never
 * gets the phone's docked key bar and has no keyboard for the chords — this is
 * its equivalent. Buttons write raw bytes through the terminal handle (the same
 * WS path as typed keys); the Ctrl latch is applied in TerminalView.
 */
export function TermControlPanel({ handleRef, ctrl, onCycleCtrl, onCopy, onPaste, onAttach }: Props) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const send = (data: string) => () => handleRef.current?.write(data);

  if (!open) {
    return (
      <button
        type="button"
        className="term-panel-toggle"
        onClick={() => setOpen(true)}
        aria-label={t("panel.open")}
        title={t("panel.open")}
      />
    );
  }

  return (
    <div className="term-panel" role="toolbar" aria-label={t("panel.label")}>
      <button className="kb" onClick={send("\x1b")}>
        {t("keybar.esc")}
      </button>
      <button className="kb" onClick={send("\x09")}>
        {t("keybar.tab")}
      </button>
      <button className="kb" onClick={send("\x1b[Z")}>
        {t("keybar.shiftTab")}
      </button>
      <button
        className={"kb" + (ctrl !== "off" ? " on" : "") + (ctrl === "locked" ? " locked" : "")}
        onClick={onCycleCtrl}
        aria-pressed={ctrl !== "off"}
      >
        {t("keybar.ctrl")}
      </button>
      <button className="kb" onClick={send("\x03")}>
        {t("keybar.ctrlC")}
      </button>
      <button className="kb up" onClick={send("\x1b[A")} aria-label={t("keybar.up")} />
      <button className="kb down" onClick={send("\x1b[B")} aria-label={t("keybar.down")} />
      <button className="kb left" onClick={send("\x1b[D")} aria-label={t("keybar.left")} />
      <button className="kb right" onClick={send("\x1b[C")} aria-label={t("keybar.right")} />
      <button
        className="kb kbd"
        onClick={() => handleRef.current?.focus()}
        aria-label={t("keybar.keyboard")}
      />
      {/* Reads the selection, so keep the tap from pulling focus off the terminal. */}
      <button className="kb" onMouseDown={(e) => e.preventDefault()} onClick={onCopy}>
        {t("keybar.copy")}
      </button>
      <button className="kb" onClick={onPaste}>
        {t("keybar.paste")}
      </button>
      <button className="kb attach" onClick={onAttach} aria-label={t("terminal.attachFile")} />
      <button
        type="button"
        className="term-panel-close"
        onClick={() => setOpen(false)}
        aria-label={t("common.close")}
        title={t("common.close")}
      />
    </div>
  );
}
