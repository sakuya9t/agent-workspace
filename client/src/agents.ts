/** Agents that persist usage transcripts the daemon can read (view-usage). */
export const USAGE_AGENTS = new Set(["claude", "codex", "pi"]);

/** Built-in agents that accept natural-language prompts in their terminal. */
export const PROMPT_AGENTS = new Set(["claude", "codex", "opencode", "pi"]);
