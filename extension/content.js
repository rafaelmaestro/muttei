// ponytail: Meet DOM selectors are the weak point; when Meet changes its markup, this is what breaks
const MIC = /microfone|microphone/i;
const CAM = /câmera|camera/i;
let want = null;
const applied = new WeakMap();

function micButton() {
  return [...document.querySelectorAll('[data-is-muted]')].find(b => {
    const label = b.getAttribute('aria-label') || '';
    return MIC.test(label) && !CAM.test(label);
  });
}

// one-way (app → Meet): apply each wanted state once per button, so a manual click in Meet is never fought
function sync() {
  const b = micButton();
  if (want === null || !b || applied.get(b) === want) return;
  applied.set(b, want);
  if ((b.getAttribute('data-is-muted') === 'true') !== want) b.click();
}

function open() {
  const port = chrome.runtime.connect({ name: 'meet' });
  port.onMessage.addListener(m => { want = m.muted; sync(); });
  port.onDisconnect.addListener(() => setTimeout(open, 1000));
}

open();
setInterval(sync, 1000);
