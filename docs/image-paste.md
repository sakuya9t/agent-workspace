# Image / screenshot paste into sessions

Status: **implemented** (daemon endpoint + web client incl. 📎 button),
2026-07-05. Verified by `scripts/paste-test.mjs` (11 checks, daemon + WS), a
headless-Chrome click-through of the 📎 button against the real bundle (upload +
placeholder echo), and a CLI probe of the agent side (below).

2026-07-12: clipboard-image paste was **macOS-only** until now — no key on
Windows/Linux could deliver an image to the page. Fixed by taking Ctrl-V from
xterm (see *Which key actually pastes*, below), and covered by the personas in
`scripts/copy-paste-test.mjs` so it can't silently regress again.

## Goal

Let a user paste (or drag-drop) a screenshot straight into a live session and
have the session's agent ingest it. Target UX:

> Focus the terminal → paste an image → the input line shows a placeholder like
> `[pasted image .asm/pastes/ab12….png]` → press Enter → the agent loads the
> image. Works on any network (loopback, LAN, SSH-forward, relay).

## Why the mechanism is "file on the node + path in the prompt"

A session's agent (Claude Code, Codex, …) runs behind a **remote PTY**. It has
no access to the browser's clipboard, and Claude Code repaints its TUI on every
keystroke, which defeats inline-image / escape-sequence injection (sixel, Kitty
graphics, OSC 1337). The only mechanism that works across all agents and all
network paths is the one a terminal drag-and-drop already uses:

1. put the image **file** somewhere the agent can read, and
2. put its **path** into the prompt as text.

This was confirmed empirically against the installed `claude` with a solid
magenta PNG (filename carries no colour hint, so only a model that sees the
pixels can name it):

| prompt form | result |
| --- | --- |
| bare path `/tmp/x.png` | `Magenta` ✓ |
| path inline with prose | `Magenta` ✓ |
| relative path from cwd `./x.png` | `Magenta` ✓ |
| `[pasted image /tmp/x.png]` (our placeholder) | `Magenta` ✓ |

So the placeholder we inject is detected and the image is loaded on submit.
Codex has an equivalent (`--image` is launch-only, so mid-session it is the same
path-in-prompt route); a plain `shell` just gets the path typed — a harmless
fallback.

## Design

### Client (`client/src/…`)

- **Capture** — `components/Terminal.tsx` offers three entry points, all sharing
  one `uploadAndInject`: a `paste` (capture-phase) listener, `drop`/`dragover`
  listeners on the terminal mount, and an explicit **📎 attach button** + hidden
  `<input type=file accept=image/*>` (the primary affordance on touch devices,
  where clipboard-image paste is unreliable). On an `image/*` item paste/drop
  `preventDefault()`s (so xterm never sees binary garbage); a plain-text paste
  falls through to xterm untouched.
- **Which key actually pastes** — only the browser's **native** paste carries the
  image (the file item lives in the `paste` event's `clipboardData`), so a paste
  gesture is useful to us only if it reaches the browser *uncancelled*. That is
  why image paste originally worked on macOS and **nowhere else** — on Windows /
  Linux both gestures dead-ended, and the terminal has no ⌘ key to fall back on:

  | gesture | `paste` event | `clipboardData` | why |
  | --- | --- | --- | --- |
  | ⌘-V (macOS) | fires | `file:image/png` | xterm doesn't touch ⌘-V |
  | Ctrl-V (was) | **never fires** | — | xterm maps it to `^V` and `preventDefault()`s, so Chrome skips its paste command |
  | Ctrl-Shift-V | fires | **empty** | it is Chrome's *paste-as-plain-text*; the image is stripped by the browser |

  So `Terminal.tsx`'s key handler now claims **Ctrl-V on Windows/Linux and hands
  it straight back to the browser** (returns `false` to xterm, *without* a
  `preventDefault` — xterm must not send `^V`, but Chrome must still run its
  paste command). The cost is `^V` (readline quoted-insert, vim visual-block) on
  those platforms — the same trade Windows Terminal and VS Code's terminal make;
  vim's own `Ctrl-Q` is the way back. macOS is untouched: ⌘-V still does it, and
  `^V` still reaches the app there.
- **Upload then inject (in that order)** — `api.pasteImage` POSTs the raw Blob;
  only after the upload resolves does the client send the placeholder over the
  **existing** WS input frame (`{t:"i", d:"[pasted image <relpath>] "}`). Because
  the file is confirmed on the node before the path appears, a slow or dropped
  link never leaves a dangling reference in the prompt — this is what satisfies
  "works in all network conditions".
- **Feedback** — a small non-intrusive overlay (`Uploading image…` / an error
  for 4 s) rendered as a React sibling of the xterm mount, never written into
  the terminal (which a TUI would repaint over).
- **Transport** — `postBlob` reuses `req`'s `Bearer` + `X-ASM-Relay-Key` auth
  but sends the Blob as-is (`application/octet-stream` / the Blob's type). No
  multipart.

### Daemon (`crates/daemon/src/api/paste.rs`)

`POST /api/sessions/:id/paste`, raw image body:

- Auth is the router's standard bearer/loopback gate (the route lives under
  `/api`, so it inherits `require_auth` with no extra wiring).
- Validates: session exists; body non-empty; `≤ 5 MiB` (matches Claude Code's
  per-image limit — the route raises the transport `DefaultBodyLimit` a little
  above this so an oversize upload gets a clean `413`, not a truncated read);
  **magic-byte sniff** for PNG / JPEG / GIF / WebP (the client `Content-Type` is
  never trusted).
- Writes to `<session.working_directory>/.asm/pastes/<uuid>.<ext>` — always
  reachable from the agent's cwd, even under a filesystem sandbox. The
  destination is derived entirely from the server-side session record, so the
  client cannot influence the path (**no traversal**).
- Best-effort writes `<cwd>/.asm/.gitignore` = `*` so pastes never pollute git
  status, without touching tracked files or git config (works for any worktree
  layout).
- Returns `{ ok, path, relative_path, filename }`. The client injects
  `relative_path` (tidier; the agent runs in cwd).

## Security notes

- Endpoint is authed like every other `/api` route.
- Path is server-derived; magic-byte + size validated; extension from the
  sniffed type, not the filename.
- The image now traverses the relay in plaintext, same as all traffic today —
  end-to-end TLS is still the pending **SEC-1** item.

## Verifying (don't redo the discovery)

Two committed proofs, plus a one-off agent check:

1. **Daemon + WS** — `node scripts/paste-test.mjs 127.0.0.1:<port> <cwd>`
   (needs a running daemon). Covers upload → file on disk → `.gitignore` →
   the shell reading the file at the returned path → 415/413/400 rejects.
2. **Browser (📎 button)** — `scripts/attach-button-test.mjs` drives the real
   bundle in headless Chrome (button → picker → upload → placeholder echoes into
   the terminal). Its header has the full recipe. General CDP technique: the
   `ui-repro-headless-chrome` note.
2b. **Browser (clipboard image)** — `scripts/copy-paste-test.mjs` seeds a real
   PNG onto Chrome's clipboard and presses the actual paste keys: T1 (Linux UA,
   which is also Windows' key model) Ctrl-V and T2 (Mac UA) ⌘-V both have to
   upload it. Two harness details make this faithful rather than decorative: the
   edit command rides on the keydown (`commands: ["paste"]`) exactly as a real
   browser delivers it, so a `preventDefault` from xterm suppresses the paste
   just like in the wild; and the image is painted on a canvas, because Chrome
   sanitizes clipboard images by decoding + re-encoding them (a hand-rolled byte
   blob like testenv's `TINY_PNG` is rejected by `clipboard.write`).
3. **Agent ingest** — the load-bearing claim (agent loads a path in the prompt)
   was proven with `claude -p "… /tmp/x.png …"` against a solid-colour PNG.

Gotchas worth remembering:
- **Port 4600 is normally the real running daemon.** Start throwaway/test
  daemons on another port, or you'll silently hit (and mutate) the live one.
- Serve the built client to a browser by pointing the daemon at it:
  `ASM_STATIC_DIR=$PWD/client/dist`. The default `local` daemon (`baseUrl=""`)
  is same-origin, so loopback trust means no token in the browser.
- The session tree is **expanded by default** (`collapsed` starts empty), so a
  `.session-row` is directly clickable — no expand step needed.

## Reusable seam

`TerminalView` now exposes its live socket via `wsRef` and a shared
`uploadAndInject`, reached from the effect's listeners through `uploadRef`. The
planned **MOB** terminal key-bar (which also needs to send programmatic input —
Esc/Tab/^C/paste) can reuse the same `wsRef`-based input path instead of
inventing another handle.

## Follow-ups (not done)

- **Cleanup policy** — prune `.asm/pastes/` on session archive and/or a rolling
  count/size cap (today pastes accumulate until the workspace is cleaned).
- **Per-agent capability hint** — e.g. show the 📎 affordance only for
  image-capable agents, and optionally use `codex --image` semantics.
- **Camera capture on mobile** — the file input omits `capture`, so the OS lets
  the user choose gallery *or* camera; a dedicated "take photo" path could be
  added if wanted.
