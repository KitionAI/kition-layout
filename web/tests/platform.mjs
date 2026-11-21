// Playwright's WebKit calls itself Safari on a Mac on every OS: its user agent names a Mac, so
// Playwright sets navigator.platform to "MacIntel" on Linux too. The editor takes Cmd for its
// shortcuts there while Playwright's ControlOrMeta presses Control. Without that override WebKit
// reports the machine's own platform, as Chromium does, so the shortcuts a test presses are the
// ones the editor expects. Each test that opens a browser imports this first.
process.env.PLAYWRIGHT_NO_UA_PLATFORM = '1'
