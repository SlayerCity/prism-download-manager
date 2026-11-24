/**
 * Captured downloads can trigger native-host launch (up to 8 s) followed by an app response
 * (up to 20 s). Keep the browser's fail-open timeout just beyond that combined budget.
 */
export const CAPTURE_HANDOFF_TIMEOUT_MS = 29_000;
