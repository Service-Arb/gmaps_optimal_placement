/* Drives the built map in headless Chromium over CDP and asserts the model
   actually produces sane numbers. Run: node smoke.js <url> */
const { spawn } = require("node:child_process");
const assert = require("node:assert");

const URL_ = process.argv[2] || "http://localhost:8731/map.html";
const PORT = 9333;

const chrome = spawn("chromium", [
  "--headless=new", "--disable-gpu", "--no-sandbox", "--window-size=1400,900",
  `--remote-debugging-port=${PORT}`, "about:blank",
], { stdio: "ignore" });

const sleep = ms => new Promise(r => setTimeout(r, ms));

async function main() {
  let targets;
  for (let i = 0; i < 40; i++) {
    try { targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json(); break; }
    catch { await sleep(250); }
  }
  const ws = new WebSocket(targets.find(t => t.type === "page").webSocketDebuggerUrl);
  await new Promise(r => ws.addEventListener("open", r));

  let id = 0;
  const pending = new Map();
  const errors = [];
  ws.addEventListener("message", ev => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
    if (m.method === "Runtime.exceptionThrown")
      errors.push(m.params.exceptionDetails.exception?.description || "exception");
  });
  const send = (method, params = {}) => new Promise(res => {
    const i = ++id; pending.set(i, res);
    ws.send(JSON.stringify({ id: i, method, params }));
  });
  const evaluate = async expr => {
    const r = await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
    if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception.description);
    return r.result.result.value;
  };

  await send("Runtime.enable");
  await send("Page.enable");
  await send("Page.navigate", { url: URL_ });
  for (let i = 0; i < 60; i++) {
    // top-level let/const live in the global lexical scope, not on window
    if (await evaluate("typeof overlay !== 'undefined' && !!overlay && !!overlay.getProjection()").catch(() => false)) break;
    await sleep(500);
  }

  const s = await evaluate(`(() => {
    const live = markers.filter(m => m.getMap());
    const nz = i => { let n = 0; for (const v of i) if (v > 0) n++; return n; };
    return {
      cells: N, comps: COMPS.length, liveMarkers: live.length,
      shown: shown.reduce((a, b) => a + b, 0),
      pop: layerByName('Population').arr().reduce((a, b) => a + b, 0),
      pressNZ: nz(press), demandNZ: nz(demand),
      canvas: [cv.width, cv.height],
      painted: (() => { const d = ctx.getImageData(0, 0, cv.width, cv.height).data;
        let n = 0; for (let i = 3; i < d.length; i += 4 * 97) if (d[i] > 0) n++; return n; })(),
    };
  })()`);
  console.log("state:", s);

  assert.equal(s.comps, 140, "competitor count");
  assert.equal(s.liveMarkers, 140, "all competitor markers attached to the map");
  assert(s.cells > 9000, "grid loaded");
  assert(s.shown > 5000, `${s.shown} cells coloured`);
  assert(Math.abs(s.pop - 454985) < 50, `population total ${s.pop}`);
  assert(s.pressNZ > 3000, `competitor pressure reaches ${s.pressNZ} cells`);
  assert(s.painted > 100, "canvas actually painted");

  // pressure must fall when washes are dropped, and rise with a wider catchment
  const sens = await evaluate(`(() => {
    const sum = a => [...a].reduce((x, y) => x + y, 0);
    const wash = TIERS.find(t => t.name === 'wash');
    const base = sum(press);
    wash.show = false; refresh(); const noWash = sum(press);
    wash.show = true; lambda = 4000; refresh(); const wide = sum(press);
    lambda = D.lambda_m; refresh();
    return { base, noWash, wide, restored: sum(press) };
  })()`);
  console.log("sensitivity:", sens);
  assert(sens.noWash < sens.base, "dropping washes must lower pressure");
  assert(sens.wide > sens.base, "wider λ must raise pressure");
  assert(Math.abs(sens.restored - sens.base) < 1e-6, "recompute is deterministic");

  // configured candidates are pinned at load and comparable against each other
  const cands = await evaluate(`(() => {
    compareCandidates();
    return { n: D.candidates.length, pins: pins.length, report: document.getElementById('report').innerText.slice(0, 300) };
  })()`);
  assert.equal(cands.pins, cands.n, "one pin per [[candidate]]");
  assert(cands.report.includes("VifNet"), "candidate comparison names the candidate");
  console.log("---- candidates ----\n" + cands.report);

  // site scoring + ranking
  const rank = await evaluate(`(() => {
    rankSites();
    const rows = [...document.querySelectorAll('#report tr')].map(r => r.textContent.trim());
    siteReport(45.7797, 3.0863);
    return { rows, pins: pins.length, report: document.getElementById('report').innerText.slice(0, 400) };
  })()`);
  assert.equal(rank.pins, 11, "10 ranked pins + 1 clicked pin");
  console.log("top sites:", rank.rows.join(" | "));
  console.log("---- click report ----\n" + rank.report);

  // every layer must colour cells without throwing
  const layers = await evaluate(`LAYERS.map((l, i) => {
    layer = LAYERS[i]; colorise();
    return l.name + '=' + shown.reduce((a, b) => a + b, 0);
  }).join(', ')`);
  console.log("layers:", layers);

  assert.deepEqual(errors, [], "no uncaught exceptions");
  console.log("\nOK — all assertions passed");
  chrome.kill();
  process.exit(0);
}

main().catch(e => { console.error("FAIL:", e.message); chrome.kill(); process.exit(1); });
