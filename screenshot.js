#!/usr/bin/env node
// Usage: node screenshot.js <url> <width> <height> <output.png>
const [,, url, w, h, out] = process.argv;
if (!url || !w || !h || !out) {
    process.stderr.write('Usage: screenshot.js <url> <width> <height> <output.png>\n');
    process.exit(1);
}

const { chromium } = require('/usr/lib/node_modules/playwright');
(async () => {
    const browser = await chromium.launch({
        headless: true,
        executablePath: '/home/chs/.cache/ms-playwright/chromium_headless_shell-1208/chrome-headless-shell-linux64/chrome-headless-shell',
    });
    const page = await browser.newPage();
    await page.setViewportSize({ width: parseInt(w), height: parseInt(h) });
    await page.goto(url, { waitUntil: 'load', timeout: 25000 });
    // Extra settle time for dynamic content
    await page.waitForTimeout(2000);
    await page.screenshot({ path: out, type: 'png' });
    await browser.close();
})().catch(e => { process.stderr.write(e.message + '\n'); process.exit(1); });
