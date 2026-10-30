// ponytail: Meet DOM selectors are the weak point; when Meet changes its markup, this is what breaks
const MIC = /microfone|microphone/i;
const CAM = /câmera|camera/i;
const HAND = /levantar a mão|raise hand|abaixar a mão|lower hand/i;
const HAND_UP = /abaixar a mão|lower hand/i;
const LEAVE = /sair da chamada|leave call/i;
let want = null, port = null, last = '';
const applied = new WeakMap();

// toggles are labelled with the action they perform: "Ativar microfone" means it is muted now
const TOGGLE = /^\s*((des)?ativar|turn (on|off))\b/i;
const OFF_NOW = /^\s*(ativar|turn on)\b/i;

const label = b => b.getAttribute('aria-label') || b.getAttribute('data-tooltip') || '';
const buttons = () => [...document.querySelectorAll('button, [role="button"]')];
const find = test => buttons().find(b => test(label(b)));
const toggle = (re, not) => {
  const ok = b => re.test(label(b)) && !(not && not.test(label(b)));
  return [...document.querySelectorAll('[data-is-muted]')].find(ok) || buttons().find(b => ok(b) && TOGGLE.test(label(b)));
};
const isOff = b => b.hasAttribute('data-is-muted') ? b.getAttribute('data-is-muted') === 'true' : OFF_NOW.test(label(b));
const micButton = () => toggle(MIC, CAM);
const camButton = () => toggle(CAM);
const handButton = () => find(l => HAND.test(l));
const leaveButton = () => find(l => LEAVE.test(l));

function status() {
  const cam = camButton(), hand = handButton();
  return { t: 'status', in_call: !!leaveButton(), cam: cam ? !isOff(cam) : null, hand: !!hand && HAND_UP.test(label(hand)) };
}

function report(force) {
  const s = status(), key = JSON.stringify(s);
  if (port && (force || key !== last)) { last = key; port.postMessage(s); }
}

// one-way (app → Meet): apply each wanted state once per button, so a manual click in Meet is never fought
let seen;
function sync() {
  const b = micButton();
  const what = b ? `${label(b)} · data-is-muted=${b.getAttribute('data-is-muted')}` : 'botão de microfone não encontrado';
  if (what !== seen) { seen = what; console.info('[muttei]', what, '· estado do app:', want === null ? 'desconhecido' : want ? 'mudo' : 'no ar'); }
  if (want !== null && b && applied.get(b) !== want) {
    applied.set(b, want);
    if (isOff(b) !== want) b.click();
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
