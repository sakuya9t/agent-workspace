# Agent Deck

`/deck` is a button-first control surface for phones and Stream Deck-shaped
screens. It deliberately does not replace the terminal: it shows live sessions
that are working or blocked, collapses idle sessions into one count, and opens
the terminal when a prompt cannot be represented safely as buttons.

The web UI supports 2×4, 4×4, and 4×6 grids. The selected layout is stored in
`asm.deckLayout`. Pagination is part of the button grid, so changing the number
of keys does not change the session or approval model.

## Controller API

A hardware controller can use the same authenticated daemon API as the web UI:

- `GET /api/sessions` — filter live sessions to `attention_state=activity` or a
  needs-user state (`likely_blocked`, `approval_needed`, `error`). Count the
  remaining live sessions as idle.
- `GET /api/sessions/:id/deck` — returns `{ "prompt": null }` when the visible
  terminal state is not safely button-addressable, otherwise:

  ```json
  {
    "prompt": {
      "revision": "8903fc2a136d32d1",
      "question": "Would you like to run the following command?",
      "detail": "Reason: Run the test suite.\n$ cargo test --workspace",
      "options": [
        { "id": 1, "label": "Yes, proceed" },
        { "id": 2, "label": "Yes, and don't ask again" },
        { "id": 3, "label": "No" }
      ],
      "selected": 1
    }
  }
  ```

- `POST /api/sessions/:id/deck/respond` with
  `{ "revision": "…", "option_id": 2 }` — re-reads the live terminal, rejects
  a stale revision with HTTP 409, and sends the required arrow keys plus Enter.
  It does not claim the session's WebSocket attachment, so a terminal viewer is
  not disconnected.

The daemon owns terminal/provider parsing and keystroke generation. A physical
device only maps keys to option ids; it never needs to parse ANSI or know how a
particular agent renders approvals.

Web keys expose stable `data-deck-action` values (`session:*`,
`approval:option:*`, `page:*`, and summary/navigation actions). These are useful
for automation and mirror the action names a hardware adapter should use.
