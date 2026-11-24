import { describe, expect, it } from 'vitest';
import { CAPTURE_HANDOFF_TIMEOUT_MS } from '../src/shared/timeouts';

describe('capture hand-off timing', () => {
  it('covers the native host launch and app-response budgets before failing open', () => {
    expect(CAPTURE_HANDOFF_TIMEOUT_MS).toBeGreaterThan(28_000);
    expect(CAPTURE_HANDOFF_TIMEOUT_MS).toBeLessThan(30_000);
  });
});
