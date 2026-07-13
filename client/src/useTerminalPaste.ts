import { MutableRefObject, useState } from "react";
import { canReadClipboard, readText } from "./clipboard";
import { TerminalHandle } from "./terminalTypes";

/**
 * Paste-into-the-terminal button behaviour, shared by the phone key bar and the
 * terminal's touch overlay. Both need the same two-path dance, and it is subtle
 * enough that a second copy would rot.
 *
 * Reading the clipboard needs a secure context, which a phone or tablet talking
 * to the daemon or the relay over plain HTTP does not have — so DON'T await the
 * answer: decide synchronously, or the user gesture is spent by the time the
 * sheet's textarea asks iOS for the keyboard. Where the read IS available it
 * stays the one-tap path; the sheet also catches the empty / dismissed read
 * (Safari asks for confirmation on every one).
 *
 * The caller renders the button and the {@link PasteSheet} itself, so each site
 * keeps its own markup and the sheet stays out of the toolbar's stacking
 * context.
 */
export function useTerminalPaste(handleRef: MutableRefObject<TerminalHandle | null>) {
  const [pasteSheet, setPasteSheet] = useState(false);

  const onPaste = () => {
    if (!canReadClipboard()) {
      setPasteSheet(true);
      return;
    }
    void readText().then((text) => {
      if (text) handleRef.current?.write(text);
      else setPasteSheet(true);
    });
  };

  return {
    onPaste,
    /** Whether to render the no-clipboard-read fallback sheet. */
    pasteSheet,
    /** Feed the sheet's text to the terminal over the normal input path. */
    submitPaste: (text: string) => handleRef.current?.write(text),
    closePasteSheet: () => setPasteSheet(false),
  };
}
