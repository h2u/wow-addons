const fs = require('node:fs/promises'),
  path = require('node:path'),
  os = require('node:os'),
  net = require('node:net'),
  { spawn, spawnSync } = require('node:child_process');
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
async function port() {
  const s = net.createServer();
  await new Promise((r) => s.listen(0, '127.0.0.1', r));
  const p = s.address().port;
  await new Promise((r) => s.close(r));
  return p;
}
async function launch(profile) {
  if (process.platform !== 'win32')
    throw new Error('This WebView2 UI driver currently supports Windows only.');
  const binary =
    process.env.WOW_ADDONS_BINARY || path.resolve(__dirname, '../target/debug/wow-addons.exe');
  await fs.access(binary);
  const debugPort = await port();
  const child = spawn(binary, [], {
    cwd: path.dirname(binary),
    env: {
      ...process.env,
      WOW_ADDONS_TEST_DATA: profile,
      WOW_ADDONS_TEST_DEBUG_PORT: String(debugPort),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=' + debugPort,
    },
    windowsHide: true,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '';
  const capture = (chunk) => {
    output = (output + chunk.toString()).slice(-65536);
  };
  child.stdout.on('data', capture);
  child.stderr.on('data', capture);
  let launchError;
  child.on('error', (error) => {
    launchError = error;
  });
  let socket;
  const pending = new Map();
  let next = 0;
  try {
    let tab;
    let lastTargets = [];
    for (let i = 0; i < 600; i++) {
      if (launchError) throw launchError;
      if (child.exitCode !== null)
        throw new Error('Native app exited with ' + child.exitCode + '\n' + output);
      try {
        const tabs = await (
          await fetch('http://127.0.0.1:' + debugPort + '/json/list', {
            signal: AbortSignal.timeout(1000),
          })
        ).json();
        lastTargets = tabs.map((t) => ({ type: t.type, url: t.url }));
        tab = tabs.find((t) => t.type === 'page' && t.url.includes('tauri.localhost'));
        if (tab) break;
      } catch {}
      await pause(100);
    }
    if (!tab)
      throw new Error(
        'Native WebView2 debug endpoint did not start. Port: ' +
          debugPort +
          '; targets: ' +
          JSON.stringify(lastTargets) +
          '\n' +
          output,
      );
    socket = new WebSocket(tab.webSocketDebuggerUrl);
    await new Promise((r, j) => {
      socket.onopen = r;
      socket.onerror = j;
    });
    socket.onmessage = (e) => {
      const m = JSON.parse(e.data),
        p = pending.get(m.id);
      if (p) {
        pending.delete(m.id);
        clearTimeout(p.timer);
        m.error ? p.reject(new Error(m.error.message)) : p.resolve(m.result);
      }
    };
    const cdp = (method, params = {}) =>
      new Promise((resolve, reject) => {
        const id = ++next,
          timer = setTimeout(() => {
            pending.delete(id);
            reject(new Error('Timed out: ' + method));
          }, 90000);
        pending.set(id, { resolve, reject, timer });
        socket.send(JSON.stringify({ id, method, params }));
      });
    const evaluate = async (expression) => {
      const r = await cdp('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
        replMode: true,
      });
      if (r.exceptionDetails)
        throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
      return r.result.value;
    };
    const wait = async (expression) => {
      for (let i = 0; i < 600; i++) {
        if (await evaluate(expression)) return;
        await pause(100);
      }
      throw new Error(
        'Timed out: ' +
          expression +
          ' ' +
          (await evaluate('document.getElementById("error")?.textContent')),
      );
    };
    const close = async () => {
      socket?.close();
      for (const p of pending.values()) {
        clearTimeout(p.timer);
        p.reject(new Error('UI driver closed.'));
      }
      pending.clear();
      if (child.exitCode === null)
        spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], {
          windowsHide: true,
          stdio: 'ignore',
        });
      await pause(300);
    };
    await wait('typeof state!=="undefined" && !!state && !!window.inventory');
    return { child, cdp, evaluate, wait, close };
  } catch (e) {
    const diagnostics = {
      binary,
      profile,
      debugPort,
      exitCode: child.exitCode,
      error: String(e.stack || e),
      output,
    };
    await fs.writeFile(path.join(profile, 'ui-startup.json'), JSON.stringify(diagnostics, null, 2));
    console.error('UI diagnostics: ' + path.join(profile, 'ui-startup.json'));
    socket?.close();
    if (child.exitCode === null)
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], {
        windowsHide: true,
        stdio: 'ignore',
      });
    throw e;
  }
}
async function profile(prefix = 'wow-tauri-test-') {
  return fs.mkdtemp(path.join(os.tmpdir(), prefix));
}
module.exports = { launch, profile, pause };
