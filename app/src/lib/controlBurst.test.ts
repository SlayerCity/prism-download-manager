import { describe, expect, it } from "vitest";
import { createControlBurst } from "./controlBurst";

const control = { key: "Control", repeat: false, altKey: false, shiftKey: false, metaKey: false, ctrlKey: false };
const controlDown = { ...control, ctrlKey: true };

function setup() {
  const gesture = createControlBurst();
  const tap = (at: number) => {
    gesture.keydown(controlDown);
    return gesture.keyup(control, at);
  };
  return { gesture, tap };
}

describe("Control lightning gesture", () => {
  it("triggers on the fifth tap and allows another strike after the cooldown", () => {
    const { tap } = setup();
    expect([0, 200, 400, 600, 800].map(tap)).toEqual([false, false, false, false, true]);
    expect([900, 1000, 1100, 1200, 1300].map(tap)).toEqual([false, false, false, false, false]);
    expect([2300, 2400, 2500, 2600, 2700].map(tap)).toEqual([false, false, false, false, true]);
  });

  it("does not add slow taps to a later burst", () => {
    const { tap } = setup();
    expect([0, 400, 800, 1200, 1600].map(tap)).toEqual([false, false, false, false, false]);
    expect(tap(1700)).toBe(false);
    expect(tap(1800)).toBe(true);
  });

  it("does not count Ctrl shortcuts, extra modifiers, or a lone keyup", () => {
    const { gesture, tap } = setup();
    [0, 100, 200, 300].forEach(tap);
    gesture.keydown(controlDown);
    const letter = { ...control, key: "f", ctrlKey: true };
    gesture.keydown(letter);
    expect(gesture.keyup(letter, 380)).toBe(false);
    expect(gesture.keyup(control, 400)).toBe(false);
    expect(gesture.keyup(control, 450)).toBe(false);
    [500, 600, 700, 800].forEach(tap);
    gesture.keydown({ ...control, shiftKey: true });
    expect(gesture.keyup({ ...control, shiftKey: true }, 900)).toBe(false);
    expect(tap(1000)).toBe(false);
  });

  it("ignores key repeat and cancels a partial sequence after focus is lost", () => {
    const { gesture, tap } = setup();
    gesture.keydown(control);
    for (let i = 0; i < 10; i++) gesture.keydown({ ...control, repeat: true });
    expect(gesture.keyup(control, 100)).toBe(false);
    expect([200, 300, 400].map(tap)).toEqual([false, false, false]);
    gesture.reset();
    expect([500, 600, 700, 800, 900].map(tap)).toEqual([false, false, false, false, true]);
  });

  it("does not count holding both Control keys together as two presses", () => {
    const { gesture, tap } = setup();
    [0, 100, 200, 300].forEach(tap);
    gesture.keydown(control);
    gesture.keydown(control);
    expect(gesture.keyup({ ...control, ctrlKey: true }, 400)).toBe(false);
    expect(gesture.keyup(control, 410)).toBe(false);
    expect(tap(500)).toBe(false);
  });
});
