const URL_ = 'ws://127.0.0.1:47822/';
const ports = new Set();
let ws = null, muted = null, retry = null, ping = null;

const browser = () => {
  const brands = (navigator.userAgentData?.brands || []).map(b => b.brand);
  return ['Microsoft Edge', 'Brave', 'Opera', 'Google Chrome', 'Chromium'].find(b => brands.includes(b)) || 'Navegador';
};
const badge = (text, color) => { chrome.action.setBadgeText({ text }); chrome.action.setBadgeBackgroundColor({ color }); };
const broadcast = msg => { for (const p of ports) p.postMessage(msg); };
const up = msg => ws && ws.readyState === 1 && ws.send(JSON.stringify(msg));

function schedule() {
  clearTimeout(retry);
  if (ports.size) retry = setTimeout(connect, 5000);
}

function connect() {
  if (ws && ws.readyState <= 1) return;
  ws = new WebSocket(URL_);
  ws.onopen = () => {
    ws.send(JSON.stringify({ t: 'hello', browser: browser() }));
    broadcast({ resend: true });
    clearInterval(ping);
    // MV3 kills an idle service worker after 30s; traffic on the socket keeps it alive
    ping = setInterval(() => ws.readyState === 1 && ws.send('{"t":"ping"}'), 20000);
  };
  ws.onmessage = e => {
    const m = JSON.parse(e.data);
    if (m.t === 'state') { muted = m.muted; badge('', '#50fa7b'); broadcast({ muted }); }
    if (m.t === 'cmd') broadcast({ cmd: m.cmd });
    if (m.t === 'denied') badge('!', '#ffb86c');
  };
  ws.onclose = () => { clearInterval(ping); if (ws) badge('off', '#6272a4'); ws = null; schedule(); };
}

chrome.runtime.onConnect.addListener(port => {
  ports.add(port);
  if (muted !== null) port.postMessage({ muted });
  port.onMessage.addListener(m => up(m));
  port.onDisconnect.addListener(() => ports.delete(port));
  connect();
});
chrome.action.onClicked.addListener(connect);
chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);
