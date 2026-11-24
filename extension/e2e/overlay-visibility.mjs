// Focused browser regression checks; no native host needed.
// Run from extension/: node e2e/overlay-visibility.mjs
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { chromium } from 'playwright';

const { outputFiles: [script] } = await build({
  stdin: {
    contents: "import { VideoButton } from './src/content/video-button'; const button = new VideoButton(); button.setEnabled(true); window.setPrismCompactForTest = (value) => button.setCompact(value);",
    resolveDir: fileURLToPath(new URL('..', import.meta.url)),
  },
  bundle: true,
  write: false,
  format: 'iife',
});
const browser = await chromium.launch({ channel: 'chromium', executablePath: process.env.PRISM_E2E_CHROME_PATH });
try {
  const page = await browser.newPage();
  await page.setContent(`
    <style>video { display: block; background: #123; }</style>
    <div id="player"><button id="fullscreen" onclick="this.parentElement.requestFullscreen()">Fullscreen</button>
      <video id="video" width="400" height="240"></video>
    </div>
    <div style="opacity: 0"><video id="transparent" width="400" height="240"></video></div>
    <video id="hidden" width="400" height="240" style="visibility: hidden"></video>
  `);
  await page.addScriptTag({ content: script.text });
  const showing = page.locator('[data-prism-video-button] .wrap.show');
  const moveOver = async (selector) => {
    const box = await page.locator(selector).boundingBox();
    await page.mouse.move(box.x + 30, box.y + 30);
    // Let the IntersectionObserver and the pointer handler run before negative assertions.
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  };
  for (const selector of ['#transparent', '#hidden']) {
    await moveOver(selector);
    assert.equal(await showing.count(), 0, `${selector} must not show an overlay`);
  }
  await moveOver('#video');
  await showing.waitFor({ state: 'attached' });
  await page.click('#fullscreen');
  await page.waitForFunction(() => document.fullscreenElement?.id === 'player');
  await showing.waitFor({ state: 'detached' });
  await moveOver('#video');
  assert.equal(await showing.count(), 0, 'pointer movement must not recreate the fullscreen overlay');
  await page.evaluate(() => document.exitFullscreen());
  await page.waitForFunction(() => !document.fullscreenElement);
  await moveOver('#video');
  await showing.waitFor({ state: 'attached' });
  const pill = showing.getByRole('button', { name: 'Download this video with Prism' });
  const normalWidth = await pill.evaluate((el) => el.offsetWidth);
  await page.evaluate(() => window.setPrismCompactForTest(true));
  assert.equal(await pill.evaluate((el) => el.offsetWidth), 32);
  assert.ok(normalWidth > 32);
  await page.evaluate(() => window.setPrismCompactForTest(false));
  assert.equal(await pill.evaluate((el) => el.offsetWidth), normalWidth);
  console.log('Passed: hidden players, fullscreen entry/exit, and accessible compact layout.');
} finally {
  await browser.close();
}
