// Small display formatters shared across components.

/**
 * Human-readable file size. Used wherever we report a size limit to the user, so
 * the terminal's attach path and the Details panel's upload path phrase the same
 * 10 MiB cap the same way.
 */
export const formatBytes = (n: number) => `${(n / (1024 * 1024)).toFixed(1)} MB`;
