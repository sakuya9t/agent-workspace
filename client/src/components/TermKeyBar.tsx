import { Dispatch, MutableRefObject, SetStateAction } from "react";
import { useTranslation } from "react-i18next";
import { copyText, readText } from "../clipboard";
import { CtrlLatch, TerminalHandle } from "../terminalTypes";

interface Props {
  /** Live handle onto the mounted terminal (see TerminalView.onReady). */
  handleRef: MutableRefObject<TerminalHandle | null>;
  ctrl: CtrlLatch;
  setCtrl: Dispatch<SetStateAction<CtrlLatch>>;
}

/**
 * Phone-only, live-sessions-only row docked above the soft keyboard, offering
 * the keys a terminal needs that soft keyboards lack. Buttons send raw bytes
 * through the terminal handle (the same WS path as typed keys); the Ctrl latch
 * is applied to the *next soft-keyboard* key inside TerminalView.
 */
export function TermKeyBar({ handleRef, ctrl, setCtrl }: Props) {
  const { t } = useTranslation();
  const send = (data: string) => () => handleRef.current?.write(data);

  // off → armed (one-shot) → locked (sticky) → off.
  const cycleCtrl = () =>
    setCtrl((c) => (c === "off" ? "armed" : c === "armed" ? "locked" : "off"));

  const onKeyboard = () => handleRef.current?.focus();
  const onPaste = async () => {
    const text = await readText();
    if (text) handleRef.current?.write(text);
  };
  const onCopy = () => {
    const sel = handleRef.current?.getSelection();
    if (sel) void copyText(sel);
  };

  return (
    <div className="term-keybar" role="toolbar" aria-label={t("keybar.label")}>
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
        onClick={cycleCtrl}
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
      <button className="kb kbd" onClick={onKeyboard} aria-label={t("keybar.keyboard")} />
      <button className="kb" onClick={onPaste}>
        {t("keybar.paste")}
      </button>
      <button className="kb" onClick={onCopy}>
        {t("keybar.copy")}
      </button>
    </div>
  );
}
