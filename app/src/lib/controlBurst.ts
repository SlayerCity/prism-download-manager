const TAPS = 5;
const WINDOW_MS = 1200;
const COOLDOWN_MS = 1400;

type Key = Pick<KeyboardEvent, "key" | "repeat" | "altKey" | "shiftKey" | "metaKey">;

/** Count quick, standalone Control taps without consuming any keyboard events. */
export function createControlBurst() {
  let armed = false;
  let taps: number[] = [];
  let lastBurst = -Infinity;

  function reset() {
    armed = false;
    taps = [];
  }

  return {
    reset,
    keydown(event: Key) {
      if (event.key !== "Control" || event.altKey || event.shiftKey || event.metaKey) {
        reset();
      } else if (!event.repeat) {
        if (armed) reset(); // Pressing both Control keys together isn't a tap.
        else armed = true;
      }
    },
    keyup(event: Key & Pick<KeyboardEvent, "ctrlKey">, now: number) {
      if (event.key !== "Control") return false;
      const standalone = armed && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey;
      armed = false;
      if (!standalone || now - lastBurst < COOLDOWN_MS) {
        reset();
        return false;
      }
      taps = taps.filter((t) => now - t <= WINDOW_MS);
      taps.push(now);
      if (taps.length < TAPS) return false;
      lastBurst = now;
      reset();
      return true;
    },
  };
}
