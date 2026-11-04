# File attachments (and image / screenshot paste) into sessions

Status: **implemented** (daemon endpoint + web client incl. 📎 button),
2026-07-05. Verified by `scripts/paste-test.mjs` (22 checks, daemon + WS), a
headless-Chrome click-through of the 📎 button against the real bundle (upload +
placeholder echo), and a CLI probe of the agent side (below).

2026-07-12: clipboard-image paste was **macOS-only** until now — no key on
Windows/Linux could deliver an image to the page. Fixed by taking Ctrl-V from
xterm (see *Which key actually pastes*, below), and covered by the personas in
`scripts/copy-paste-test.mjs` so it can't silently regress again.

2026-07-21: added a second, complementary verb — **workspace upload**, a Details-panel
button that copies files to `<cwd>/uploads/<name>` under their real names so an
agent can find them by listing a directory rather than being handed a path. See
*Workspace upload*, below; the attachment behaviour described here is unchanged.

2026-07-12: **widened from images to any file type, ≤ 10 MiB.** An agent gets as
much out of a PDF, a zip, or a CSV as it does out of a screenshot, and the
transport was never image-specific — only the validation was. The magic-byte
allowlist is gone; **size is now the only bound**. See *From images to any file*
below for what that changed and why it's safe here.

## Goal

Let a user paste, drag-drop, or pick **any file** straight into a live session
and have the session's agent ingest it. Target UX:

> Focus the terminal → paste an image (or hit 📎 and pick `spec.pdf`) → the input
> line shows a placeholder like `[pasted image .asm/pastes/shot-ab12….png]` or
> `[attached file .asm/pastes/spec-ab12….pdf]` → press Enter → the agent loads
> it. Works on any network (loopback, LAN, SSH-forward, relay).

## From images to any file

The mechanism below (file on the node + path in the prompt) was never
image-specific — an agent reads a PDF or unzips an archive from a path exactly
as it loads a PNG from one. Only the *validation* was image-specific, so that is
all that had to go:

| | before | now |
| --- | --- | --- |
| type check | magic-byte allowlist (PNG/JPEG/GIF/WebP), else `415` | none — any bytes are stored |
| size cap | 5 MiB (Claude Code's per-image limit) | **10 MiB** |
| picker | `<input accept="image/*">` | no `accept` — every file is offerable |
| drop / paste | first `image/*` item; a dropped PDF was a **silent no-op** | first file item, whatever it is |
| stored name | `<uuid>.<sniffed-ext>` | `<stem>-<uuid8>.<ext>` — keeps the user's name |
| placeholder | `[pasted image <path>]` | same for images; `[attached file <path>]` otherwise |
| oversize UX | uploaded in full, then a `413` | client pre-checks the size and fails instantly |

Two things are worth calling out:

- **Why dropping the allowlist is fine here.** The endpoint writes bytes to a
  file and never executes, parses, or serves them; it is authed like every other
  `/api` route; and ASM is a LAN-local tool where the user attaching the file is
  the user who owns the workspace. A type allowlist was buying nothing against
  that threat model while costing the whole PDF/zip use case. What actually
  bounds the endpoint is the size cap, which is enforced server-side and cannot
  be talked out of.
- **The client now names the file, so the name is now untrusted input.** That is
  the one new surface, and it is handled in `safe_stem_ext`: the name is reduced
  to a single path component (basename only, `[A-Za-z0-9._-]`, no leading dots,
  truncated), then made unique with a uuid. The *directory* is still derived
  entirely from the session record, so `../../../../tmp/evil.pdf` lands at
  `.asm/pastes/evil-3f2a1b9c.pdf` — sanitised, not rejected. Unit-tested in
  `paste.rs`, and asserted end-to-end in `paste-test.mjs`.

The 5 MiB cap used to be justified as "Claude Code's per-image limit". That
reasoning only ever applied to images; 10 MiB is a round number that comfortably
holds a paper, a source tarball, or a log bundle. An image over Claude's own
limit is now something the agent rejects on read, rather than something ASM
refuses to store — the right place for that policy.

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
  `<input type=file>` with **no `accept`** (the primary affordance on touch
  devices, where clipboard-image paste is unreliable). Any *file* item
  paste/drop `preventDefault()`s (so xterm never sees binary garbage); a
  plain-text paste — whose clipboard items are all `kind: "string"` — falls
  through to xterm untouched. `dragover` always accepted any file kind; before
  the widening `drop` then quietly discarded everything that wasn't an image,
  which is the worst kind of no-op: the cursor said yes and nothing happened.
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
- **Size pre-check** — the client rejects anything over `MAX_ATTACHMENT_BYTES`
  (10 MiB, mirrored from the daemon) *before* uploading, so a too-big file fails
  instantly and legibly instead of after a long upload ending in a `413`. The
  daemon still enforces the real limit; this is UX, not security.
- **Upload then inject (in that order)** — `api.uploadAttachment` POSTs the raw
  Blob with the filename as a `?name=` query param; only after the upload
  resolves does the client send the placeholder over the **existing** WS input
  frame (`{t:"i", d:"[attached file <relpath>] "}`, or `[pasted image …]` for an
  `image/*` blob). Because the file is confirmed on the node before the path
  appears, a slow or dropped link never leaves a dangling reference in the
  prompt — this is what satisfies "works in all network conditions".
- **Feedback** — a small non-intrusive overlay (`Uploading spec.pdf…` / an error
  for 4 s) rendered as a React sibling of the xterm mount, never written into
  the terminal (which a TUI would repaint over).
- **Transport** — `postBlob` reuses `req`'s `Bearer` + `X-ASM-Relay-Key` auth
  but sends the Blob as-is (`application/octet-stream` / the Blob's type). No
  multipart.

### Daemon (`crates/daemon/src/api/paste.rs`)

`POST /api/sessions/:id/paste?name=<filename>`, raw body of any type:

- Auth is the router's standard bearer/loopback gate (the route lives under
  `/api`, so it inherits `require_auth` with no extra wiring).
- Validates: session exists; body non-empty; `≤ 10 MiB` (`MAX_PASTE_BYTES` — the
  route raises the transport `DefaultBodyLimit` a little above this so an
  oversize upload gets a clean `413`, not a truncated read). **No type check**:
  any bytes are storable.
- Writes to `<session.working_directory>/.asm/pastes/<stem>-<uuid8>.<ext>` —
  always reachable from the agent's cwd, even under a filesystem sandbox. The
  **directory** is derived entirely from the server-side session record; only the
  leaf name comes from the client, and `safe_stem_ext` reduces it to one safe
  path component (**no traversal**).
- `ext` comes from the client's filename when it has one, else from the
  magic-byte sniff (a clipboard image is a bare blob with no name), else `bin`.
  So `sniff_image_ext` survives as a *fallback*, not a gate — and stays shared
  with the diff panel's preview endpoint, which still needs a real image check.
- Best-effort writes `<cwd>/.asm/.gitignore` = `*` so attachments never pollute
  git status, without touching tracked files or git config (works for any
  worktree layout).
- Returns `{ ok, path, relative_path, filename }`. The client injects
  `relative_path` (tidier; the agent runs in cwd).

## Security notes

- Endpoint is authed like every other `/api` route.
- The directory is server-derived; the client-supplied filename is sanitised to
  a single path component and made unique with a uuid, so it can neither escape
  `.asm/pastes/` nor clobber an earlier attachment.
- Size (10 MiB) is the only bound on content, deliberately — see *From images to
  any file*. The bytes are written to a file and never executed, parsed, or
  served back.
- The file traverses the relay in plaintext, same as all traffic today. Per the
  2026-07-12 decision, the LAN journey is plaintext **by design** — encryption
  for the off-LAN path is the relay's job (R5), not a daemon-terminated TLS
  layer. See `docs/security-followups.md`.

## Verifying (don't redo the discovery)

Two committed proofs, plus a one-off agent check:

1. **Daemon + WS** — `node scripts/paste-test.mjs` (sandboxed by default; spawns
   its own daemon). Covers upload → file on disk → `.gitignore` → the shell
   reading the file at the returned path; PDF/ZIP/text attachments keeping their
   stem and extension; a traversal filename collapsing into `.asm/pastes/`; and
   the 413/400 rejects. The unnamed-PNG case exercises the magic-byte fallback.
2. **Browser (📎 button)** — `scripts/attach-button-test.mjs` drives the real
   bundle in headless Chrome (button → picker → upload → placeholder echoes into
   the terminal) for **both** a PNG (`[pasted image …]`) and a PDF
   (`[attached file …]`), and asserts the input carries no `accept` filter. Its
   header has the full recipe. General CDP technique: the
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

## Workspace upload (2026-07-21)

Status: **implemented** (daemon endpoint + Details-panel button and drop zone).
Verified by `scripts/workspace-upload-test.mjs` (22 checks) and
`scripts/workspace-upload-ui-test.mjs` (15 checks, headless Chrome).

The attachment path above answers "let the agent *see* this file, once". It does
not answer "put this file **in my workspace**" — and that gap was real: the only
way to get a file onto the node was to paste it into a live terminal and let the
client inject a `.asm/pastes/<stem>-<uuid8>.<ext>` path. A user who wants to hand
a session a dataset, a spec, or a log bundle and then *talk about it by name* had
nowhere to put it.

`POST /api/sessions/:id/upload?name=<filename>[&force=true]` is that second verb.
Same transport, same 10 MiB cap, same `safe_stem_ext` sanitiser — three
deliberate differences:

| | `/paste` (attachment) | `/upload` (workspace) |
| --- | --- | --- |
| lands in | `.asm/pastes/` (self-ignoring) | `uploads/` — **visible, not git-ignored** |
| stored name | `<stem>-<uuid8>.<ext>` | the client's name, **verbatim** |
| collision | impossible, by construction | `409` → client confirms → `force=true` |
| how the agent finds it | path injected into the prompt | `ls uploads/` |

### Why each difference

- **Visible, not ignored.** An uploaded file is working material the user may
  well want to commit; an attachment is a one-shot reference that should never
  reach the repo. So `uploads/` deliberately does *not* get the `.asm/` treatment
  and does show up in `git status`.
- **The exact name, no uuid.** A predictable path is the entire feature — it is
  what lets the user say "read `uploads/spec.pdf`" or the agent discover the file
  by listing the directory. That makes `safe_stem_ext` load-bearing here in a way
  it isn't for a paste, since the uuid is no longer there as a second line of
  defence. Note one wrinkle the paste path never had: an extension-less name is a
  *real* name, so `Makefile` must not become `Makefile.bin` — the sniffed-extension
  fallback applies only when there is no usable name at all.
- **409, not overwrite and not uniquify.** Keeping the name means collisions are
  now possible, and `uploads/` sits inside the user's checkout — a silent
  overwrite could destroy source. Silently uniquifying would be worse than
  useless: it hands back a name the user didn't ask for, which is exactly the
  predictability the feature exists to provide. So the daemon refuses and the
  client turns the `409` into a "Replace `spec.pdf`?" prompt, reusing the
  confirm-and-retry-with-force idiom already used for branch delete and archive.
  A *directory* in the way is a `400` instead, because no confirm makes that
  retry succeed — offering the prompt would just loop.

### Security notes

Everything from the paste path carries over (server-derived directory,
sanitised single-component leaf name, size as the only content bound, authed
like every other `/api` route), plus one new consideration the paste path was
immune to:

- **A forced replace unlinks before writing.** `fs::write` follows symlinks, and
  an agent running in this very session could have left `uploads/spec.pdf` as a
  symlink to anywhere the daemon user can reach. Removing the entry first means
  a replace can only ever create a regular file at that path. The existence
  check uses `symlink_metadata` rather than `exists` for the same reason — a
  dangling symlink reports as absent to `exists` but still occupies the name.
  Asserted end-to-end in `workspace-upload-test.mjs` ("the symlink target was
  NOT written through").

### Client

The button lives in the Details panel directly under the **Directory** field, so
the layout answers "where does this go?" without help text. The whole panel body
is the drop target — a button-height strip is a miserable thing to aim a dragged
file at — and lights up with an inset dashed outline while a file drag is over
it (`dragenter`/`dragleave` are depth-counted, since they fire for every child
the pointer crosses). The picker carries `multiple` and no `accept`; uploads run
**sequentially** so the progress line names one file and, more importantly, so
two replace prompts can never race for the user's attention.

## Reusable seam

`TerminalView` now exposes its live socket via `wsRef` and a shared
`uploadAndInject`, reached from the effect's listeners through `uploadRef`. The
planned **MOB** terminal key-bar (which also needs to send programmatic input —
Esc/Tab/^C/paste) can reuse the same `wsRef`-based input path instead of
inventing another handle.

## Follow-ups (not done)

- **Cleanup policy** — prune `.asm/pastes/` on session archive and/or a rolling
  count/size cap (today attachments accumulate until the workspace is cleaned).
  Now slightly more pressing: a 10 MiB zip is a bigger squatter than a
  screenshot. Tracked as **IMG-2**.
- **Multiple files at once** — the picker has no `multiple`, and paste/drop take
  only the first file item. Dropping a folder or a multi-select still attaches
  one file, silently. A loop over `dataTransfer.files` + one placeholder per
  upload would do it.
- **Per-agent capability hint** — e.g. show the 📎 affordance only for
  file-capable agents, and optionally use `codex --image` semantics.
- **Camera capture on mobile** — the file input omits `capture`, so the OS lets
  the user choose gallery *or* camera; a dedicated "take photo" path could be
  added if wanted.
