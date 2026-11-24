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
  const before = await pill.boundingBox();
  const video = await page.locator('#video').boundingBox();
  await page.mouse.move(before.x + before.width / 2, before.y + before.height / 2);
  await page.mouse.down();
  await page.mouse.move(video.x + 80, video.y + 150, { steps: 10 });
  await page.mouse.up();
  await page.waitForFunction((left) => document.querySelector('[data-prism-video-button]').shadowRoot
    .querySelector('.pill').getBoundingClientRect().left < left - 40, before.x);
  assert.equal(await pill.getAttribute('aria-expanded'), 'false', 'drag must not activate the download menu');
  const after = await pill.boundingBox();
  assert.ok(after.x >= video.x && after.y >= video.y);
  assert.ok(after.x + after.width <= video.x + video.width);
  assert.ok(after.y + after.height <= video.y + video.height);
  await pill.click();
  assert.equal(await pill.getAttribute('aria-expanded'), 'true', 'a normal click must still open the menu');
  await page.keyboard.press('Escape');
  assert.equal(await pill.getAttribute('aria-expanded'), 'false');
  await page.keyboard.press('Enter');
  assert.equal(await pill.getAttribute('aria-expanded'), 'true', 'keyboard activation still works');

  // YouTube homepage hover previews use small, muted, looping players within feed cards.
  const youtube = await browser.newPage();
  await youtube.route('https://www.youtube.com/**', (route) => route.fulfill({
    status: 200,
    contentType: 'text/html',
    body: `<!doctype html><title>Home - YouTube</title><style>
      ytd-rich-item-renderer { display:block; width:320px; height:180px; }
      #unrelated { display:block; width:320px; height:180px; }
    </style><ytd-rich-item-renderer></ytd-rich-item-renderer><video id="unrelated" muted autoplay loop></video>
    <script>document.querySelector('ytd-rich-item-renderer').attachShadow({ mode: 'open' });</script>`,
  }));
  await youtube.goto('https://www.youtube.com/');
  await youtube.addScriptTag({ content: script.text });
  await youtube.evaluate(() => {
    document.querySelector('ytd-rich-item-renderer').shadowRoot.innerHTML =
      '<style>.html5-video-player, video { display:block; width:320px; height:180px; }</style><div class="html5-video-player"><video id="preview" muted autoplay loop></video></div>';
  });
  const previewBox = await youtube.locator('#preview').boundingBox();
  await youtube.mouse.move(previewBox.x + 40, previewBox.y + 40);
  const youtubeShowing = youtube.locator('[data-prism-video-button] .wrap.show');
  await youtubeShowing.waitFor({ state: 'attached' });
  const unrelatedBox = await youtube.locator('#unrelated').boundingBox();
  await youtube.mouse.move(unrelatedBox.x + 40, unrelatedBox.y + 40);
  await youtube.waitForFunction(() => !document.querySelector('[data-prism-video-button] .wrap.show'));
  await youtube.close();
  console.log('Passed: hidden players, fullscreen, compact layout, bounded drag, click/keyboard activation, and YouTube feed hover preview.');
} finally {
  await browser.close();
}
