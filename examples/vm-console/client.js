import RFB from '/novnc/core/rfb.js';

const status = document.getElementById('status');
const token = location.hash.slice(1);
history.replaceState(null, '', location.pathname);
if (!/^[a-f0-9]{64}$/.test(token)) {
  status.textContent = 'Open the link printed by the local console bridge.';
} else {
  const rfb = new RFB(document.getElementById('screen'), `ws://${location.host}/console`, {
    wsProtocols: ['binary', `nanocl-${token}`],
    shared: true,
  });
  rfb.scaleViewport = true;
  rfb.addEventListener('connect', () => { status.textContent = 'Connected'; });
  rfb.addEventListener('disconnect', () => { status.textContent = 'Disconnected. Reopen the bridge link to reconnect.'; });
  rfb.addEventListener('securityfailure', () => { status.textContent = 'Console connection rejected'; });
  document.getElementById('disconnect').onclick = () => rfb.disconnect();
  window.addEventListener('pagehide', () => rfb.disconnect());
}
