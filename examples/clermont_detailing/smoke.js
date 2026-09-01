/* Drives the served map in headless Chromium over CDP. Run: node smoke.js <url>
 *
 * Only what is genuinely browser-shaped lives here: the island hydrates, the canvas actually
 * paints, the markers attach, and a pin round-trips through the server to the file under
 * XDG_DATA_HOME. The model's own numbers moved to `cargo t -p service_arb_core`, which checks them
 * against the same Clermont fixture with no browser and no key.
 *
 * Writes pins, so the caller must point XDG_DATA_HOME somewhere disposable. */
const { spawn } = require("node:child_process");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");

const URL_ = process.argv[2] || "http://localhost:8731/";
const PORT = 9333;
const PINS = path.join(process.env.XDG_DATA_HOME || "", "service_arb", "pins-clermont_detailing.json");
const PROMOTED = "Smoke Site";

const chrome = spawn("chromium", [
  "--headless=new", "--disable-gpu", "--no-sandbox", "--window-size=1400,900",
  `--remote-debugging-port=${PORT}`, "about:blank",
], { stdio: "ignore" });

const sleep = ms => new Promise(r => setTimeout(r, ms));
// unbuffered: a hang mid-run should still show how far it got
const log = (...a) => fs.writeSync(1, a.map(x => (typeof x === "string" ? x : JSON.stringify(x))).join(" ") + "\n");
const pins = () => (fs.existsSync(PINS) ? JSON.parse(fs.readFileSync(PINS, "utf8")) : null);

async function main() {
  assert(process.env.XDG_DATA_HOME, "smoke.js writes pins — point XDG_DATA_HOME somewhere disposable");
  fs.rmSync(PINS, { force: true });

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
  // how to answer the next prompt/confirm; the click that raises one cannot be awaited. `null`
  // asserts no dialog is expected at all.
  let answer = null;
  const dialogs = [];
  ws.addEventListener("message", ev => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
    if (m.method === "Runtime.exceptionThrown")
      errors.push(m.params.exceptionDetails.exception?.description || "exception");
    if (m.method === "Page.javascriptDialogOpening") {
      dialogs.push(m.params.type);
      send("Page.handleJavaScriptDialog", { accept: answer !== null, promptText: answer ?? undefined });
    }
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
  const act = glyph => `[...document.querySelectorAll('#report .acts button')].find(b => b.textContent.trim() === '${glyph}')`;
  const ctl = text => `[...document.querySelectorAll('#ctl button')].find(b => b.textContent.includes(${JSON.stringify(text)}))`;
  const markers = () => evaluate("document.querySelectorAll('#map [role=button]').length");
  // pins reach the map through an effect, so a count is only wrong once it has stopped changing
  const expectMarkers = async (n, msg) => {
    let got;
    for (let i = 0; i < 20; i++) { got = await markers(); if (got === n) return; await sleep(500); }
    assert.equal(got, n, msg);
  };
  // the panel is server-rendered empty and filled by the island, so its contents are the only
  // honest signal that the wasm landed and the payload parsed
  const load = async () => {
    await send("Page.navigate", { url: URL_ });
    for (let i = 0; i < 60; i++) {
      if (await evaluate("document.querySelectorAll('#ctl select option').length > 0 && !!document.querySelector('canvas')").catch(() => false)) return;
      await sleep(500);
    }
    throw new Error("the island never settled");
  };
  // a click that raises a dialog blocks the page, so the evaluate must not be awaited
  const clickThrough = (expr, reply = null) => {
    answer = reply;
    dialogs.length = 0;
    send("Runtime.evaluate", { expression: `(${expr}).click()` });
    return sleep(1500);
  };

  await send("Runtime.enable");
  await send("Page.enable");
  await load();

  const s = await evaluate(`(() => {
    const cv = document.querySelector('canvas');
    return {
      layers: document.querySelectorAll('#ctl select option').length,
      tierRows: document.querySelectorAll('#ctl .row').length,
      ticks: [...document.querySelectorAll('#ticks span')].map(t => t.textContent).join(' '),
      note: document.querySelector('#ctl .note').innerText.replace(/\\n/g, ' '),
      markers: document.querySelectorAll('#map [role=button]').length,
      banner: document.querySelector('#banner')?.innerText ?? null,
      canvas: [cv.width, cv.height],
      painted: (() => { const d = cv.getContext('2d').getImageData(0, 0, cv.width, cv.height).data;
        let n = 0; for (let i = 3; i < d.length; i += 4 * 97) if (d[i] > 0) n++; return n; })(),
    };
  })()`);
  log("state:", s);

  assert.equal(s.banner, null, "no banner");
  assert.equal(s.layers, 8, "three live layers plus the study's five");
  assert.equal(s.tierRows, 3, "one row per tier, plus hide-imputed");
  assert(s.note.includes("10446 cells"), `legend counts the grid: ${s.note}`);
  assert.equal(s.markers, 141, "140 competitor markers and the study's one candidate");
  assert(s.painted > 100, "canvas actually painted");

  // a click drops a pin and opens its card — the card is what makes a yellow pin reachable at all
  log("step: click the map");
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x: 700, y: 400, button: "left", buttons: 1, clickCount: 1 });
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: 700, y: 400, button: "left", buttons: 0, clickCount: 1 });
  await sleep(1500);
  const card = await evaluate(`(() => { const el = document.getElementById('report');
    return el && { text: el.innerText.slice(0, 160), acts: [...el.querySelectorAll('.acts button')].map(b => b.textContent.trim()) }; })()`);
  log("---- card ----\n" + (card?.text ?? "(none)"));
  assert(card, "a click opens a card");
  assert.deepEqual(card.acts, ["✓", "⧉", "✕"], "a yellow pin can be kept, copied or dropped");
  await expectMarkers(142, "the clicked pin is on the map");
  assert(card.text.includes("Capture score"), "the card is a site report");

  log("step: promote");
  // ✓ — name it, and it becomes green, lettered, and a line in the file
  await clickThrough(act("✓"), PROMOTED);
  assert.deepEqual(pins()?.added?.map(p => p.name), [PROMOTED], `the promotion is in ${PINS}`);
  assert.deepEqual(pins()?.hidden, [], "nothing hidden yet");
  const promoted = await evaluate(`document.querySelector('#report h4').textContent`);
  assert(promoted.startsWith("B · " + PROMOTED), `the promotion takes the next letter: ${promoted}`);

  log("step: rank");
  // the green set survives both of the buttons that used to wipe it
  await evaluate(`${ctl("Rank top 10 sites")}.click()`);
  await expectMarkers(152, "ranking adds ten yellow pins and keeps the two green ones");
  await evaluate(`${ctl("Clear pins")}.click()`);
  await expectMarkers(142, "clearing takes the yellow pins and leaves the green ones");

  await evaluate(`${ctl("Compare")}.click()`);
  await sleep(800);
  const compare = await evaluate(`document.getElementById('report').innerText.slice(0, 250)`);
  assert(compare.includes("VifNet") && compare.includes(PROMOTED), `the comparison reads the effective set: ${compare}`);
  log("---- candidates ----\n" + compare);

  log("step: reload");
  // it is still there after a restart of the page, because the server kept it
  await load();
  await expectMarkers(142, "the promoted pin comes back green, the yellow ones do not");

  log("step: drop");
  // ✕ on a promoted pin: nothing to confirm, it was never an argument in the study file
  await clickThrough(`document.querySelectorAll('#map [role=button]')[141]`);
  assert.equal((await evaluate(`document.querySelector('#report h4')?.textContent ?? ''`)).startsWith("B · " + PROMOTED), true, "the promoted pin opens its own card");
  await clickThrough(act("✕"));
  assert.deepEqual(dialogs, [], "dropping a pin this session promoted asks nothing");
  assert.deepEqual(pins()?.added, [], "dropping a promoted pin takes it out of the file");
  await expectMarkers(141, "and off the map");

  log("step: hide a study candidate");
  // ✕ on one the study file names is an argument with the document, so it is confirmed
  await clickThrough(`document.querySelectorAll('#map [role=button]')[140]`);
  assert.equal((await evaluate(`document.querySelector('#report h4')?.textContent ?? ''`)).startsWith("A · VifNet"), true, "the study's candidate opens its own card");
  await clickThrough(act("✕"), "");
  assert.deepEqual(dialogs, ["confirm"], "hiding a candidate the study names is confirmed first");
  assert.deepEqual(pins()?.hidden, ["VifNet"], "the study file is untouched; the hiding lives beside it");
  await load();
  await expectMarkers(140, "it stays hidden across a reload");

  // the study file is still the seed: delete the diff and the candidate is back
  fs.rmSync(PINS);
  await load();
  await expectMarkers(141, "deleting the pin file restores what the study declares");

  assert.deepEqual(errors, [], "no uncaught exceptions");
  log("\nOK — all assertions passed");
  chrome.kill();
  process.exit(0);
}

main().catch(e => { console.error("FAIL:", e.message); chrome.kill(); process.exit(1); });
