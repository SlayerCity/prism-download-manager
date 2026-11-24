import { describe, expect, it } from 'vitest';
import { normalizeVideoButtonOpacity, VIDEO_BUTTON_OPACITY_KEY } from '../src/shared/settings';

describe('video button opacity', () => {
  it('defaults safely and stores a bounded 5-percent step', () => {
    expect(normalizeVideoButtonOpacity(undefined)).toBe(100);
    expect(normalizeVideoButtonOpacity(63)).toBe(65);
    expect(normalizeVideoButtonOpacity(0)).toBe(20);
    expect(normalizeVideoButtonOpacity(120)).toBe(100);
    expect(normalizeVideoButtonOpacity(Number.NaN)).toBe(100);
    expect(normalizeVideoButtonOpacity(Number.POSITIVE_INFINITY)).toBe(100);
    expect(VIDEO_BUTTON_OPACITY_KEY).toBe('prism.videoButtonOpacity');
  });
});
