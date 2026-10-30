// ponytail: Meet DOM selectors are the weak point; when Meet changes its markup, this is what breaks
const MIC = /microfone|microphone/i;
const CAM = /câmera|camera/i;
const HAND = /levantar a mão|raise hand|abaixar a mão|lower hand/i;
const HAND_UP = /abaixar a mão|lower hand/i;
const LEAVE = /sair da chamada|leave call/i;
let want = null, port = null, last = '';
const applied = new WeakMap();

const label = b => b.getAttribute('aria-label') || '';
const find = test => [...document.querySelectorAll('button, [role="button"]')].find(b => test(label(b)));
const micButton = () => [...document.querySelectorAll('[data-is-muted]')].find(b => MIC.test(label(b)) && !CAM.test(label(b)));
const camButton = () => [...document.querySelectorAll('[data-is-muted]')].find(b => CAM.test(label(b)));
const handButton = () => find(l => HAND.test(l));
const leaveButton = () => find(l => LEAVE.test(l));

function status() {
  const cam = camButton(), hand = handButton();
  return { t: 'status', in_call: !!leaveButton(), cam: cam ? cam.getAttribute('data-is-muted') !== 'true' : null, hand: !!hand && HAND_UP.test(label(hand)) };
}

function report(force) {
  const s = status(), key = JSON.stringify(s);
  if (port && (force || key !== last)) { last = key; port.postMessage(s); }
}

// one-way (app → Meet): apply each wanted state once per button, so a manual click in Meet is never fought
function sync() {
  const b = micButton();
  if (want !== null && b && applied.get(b) !== want) {
    applied.set(b, want);
    if ((b.getAttribute('data-is-muted') === 'true') !== want) b.click();
  }
  report(false);
}

function run(cmd) {
  const b = { camera: camButton, hand: handButton, leave: leaveButton }[cmd]?.();
  if (!b) return port.postMessage({ t: 'result', cmd, ok: false });
  b.click();
  // Meet updates the button a moment after the click; read the new state then
  setTimeout(() => {
    const s = status();
    port.postMessage({ t: 'result', cmd, ok: true, on: cmd === 'camera' ? s.cam : cmd === 'hand' ? s.hand : null });
    report(true);
  }, 400);
}

function open() {
  port = chrome.runtime.connect({ name: 'meet' });
  port.onMessage.addListener(m => {
    if ('muted' in m) { want = m.muted; sync(); }
    if (m.cmd) run(m.cmd);
    if (m.resend) report(true);
  });
  port.onDisconnect.addListener(() => { port = null; setTimeout(open, 1000); });
  report(true);
}

open();
setInterval(sync, 1000);
