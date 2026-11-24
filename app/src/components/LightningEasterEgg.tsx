import { useEffect, useState } from "react";
import { createControlBurst } from "../lib/controlBurst";
import "./lightning.css";

const EFFECT_MS = 720;
const BOLTS = [
  "M-12 118 128 142 168 204 268 180 249 245 380 231 351 305 503 282 474 351 635 332 671 385 780 358 757 419 895 404 1014 475 M249 245 194 275 207 300 101 314 M351 305 300 354 324 399 230 446 M635 332 620 265 701 249 720 195 820 169 M780 358 830 298 904 314 961 273",
  "M115 -10 152 67 239 76 216 121 268 180 M503 282 545 208 610 200 588 156 700 124 717 83 839 67 869 -12 M588 156 507 160 473 104 429 111 403 63 M474 351 425 403 453 450 382 482 403 525 297 553 264 633",
  "M-10 493 115 504 159 462 230 446 M757 419 713 470 745 486 682 536 711 565 650 621 M895 404 931 365 1011 370",
];

/** A decorative overlay in the focused main window only; it never captures input. */
export function LightningEasterEgg({ reduced }: { reduced: boolean }) {
  const [active, setActive] = useState(false);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const gesture = createControlBurst();
    let timer: number | undefined;
    const reset = () => gesture.reset();
    const hide = () => {
      window.clearTimeout(timer);
      setActive(false);
    };
    const onPreferenceChange = () => {
      reset();
      if (media.matches) hide();
    };
    const onUp = (event: KeyboardEvent) => {
      if (!gesture.keyup(event, performance.now()) || reduced || media.matches) return;
      setActive(true);
      timer = window.setTimeout(hide, EFFECT_MS);
    };
    if (reduced) hide();
    window.addEventListener("keydown", gesture.keydown, true);
    window.addEventListener("keyup", onUp, true);
    window.addEventListener("blur", reset);
    media.addEventListener("change", onPreferenceChange);
    return () => {
      window.removeEventListener("keydown", gesture.keydown, true);
      window.removeEventListener("keyup", onUp, true);
      window.removeEventListener("blur", reset);
      media.removeEventListener("change", onPreferenceChange);
      window.clearTimeout(timer);
    };
  }, [reduced]);

  if (!active || reduced) return null;
  return (
    <svg className="prism-lightning" viewBox="0 0 1000 620" preserveAspectRatio="none" aria-hidden="true" focusable="false">
      {BOLTS.map((d, i) => (
        <g key={d} className={`prism-lightning__bolt prism-lightning__bolt--${i}`}>
          <path className="prism-lightning__halo" d={d} vectorEffect="non-scaling-stroke" />
          <path className="prism-lightning__rim" d={d} vectorEffect="non-scaling-stroke" />
          <path className="prism-lightning__core" d={d} vectorEffect="non-scaling-stroke" />
        </g>
      ))}
    </svg>
  );
}
