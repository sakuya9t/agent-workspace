// Verify remote WebSocket auth: no token -> rejected, ?access_token -> ok.
const lanip = process.argv[2];
const port = process.argv[3];
const token = process.argv[4];
const sid = process.argv[5];

function tryWs(url) {
  return new Promise((resolve) => {
    const ws = new WebSocket(url);
    ws.binaryType = "arraybuffer";
    let got = false;
    ws.onmessage = () => {
      got = true;
      ws.close();
      resolve("open+data");
    };
    ws.onerror = () => resolve("error");
    ws.onclose = () => resolve(got ? "open+data" : "closed-no-data");
    setTimeout(() => {
      try { ws.close(); } catch {}
      resolve(got ? "open+data" : "timeout");
    }, 1500);
  });
}

const base = `ws://${lanip}:${port}/api/sessions/${sid}/stream`;
const noTok = await tryWs(base);
const withTok = await tryWs(`${base}?access_token=${token}`);
console.log("remote WS without token:", noTok);
console.log("remote WS with token   :", withTok);
const pass = noTok !== "open+data" && withTok === "open+data";
console.log(pass ? "RESULT: PASS" : "RESULT: FAIL");
process.exit(pass ? 0 : 1);
