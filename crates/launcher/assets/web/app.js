"use strict";

const $ = (id) => document.getElementById(id);
const listEl = $("list"), playerEl = $("player"), videoEl = $("video");
const titleEl = $("title"), backEl = $("back"), bannerEl = $("banner");

let hls = null;
let curGame = null;          // {id, name}
let curSession = null;       // session id
let inMs = null, outMs = null;

const hevcOk = (window.MediaSource &&
  MediaSource.isTypeSupported('video/mp4; codecs="hvc1.1.6.L150.B0"'));

// ---- routing --------------------------------------------------------------
window.addEventListener("hashchange", route);
window.addEventListener("DOMContentLoaded", route);
backEl.onclick = () => {
  if (curSession && curGame) { location.hash = `#/game/${curGame.id}`; }
  else { location.hash = "#/"; }
};

function route() {
  const h = location.hash.replace(/^#\/?/, "");
  const parts = h.split("/").filter(Boolean);
  stopPlayer();
  if (parts[0] === "game" && parts[1]) { showSessions(parts[1]); }
  else if (parts[0] === "session" && parts[1]) { showPlayer(parts[1]); }
  else { showGames(); }
}

// ---- views ----------------------------------------------------------------
async function showGames() {
  curSession = null; curGame = null;
  titleEl.textContent = "Rokugakun";
  backEl.classList.add("hidden");
  playerEl.classList.add("hidden");
  listEl.classList.remove("hidden");
  listEl.innerHTML = "Loading…";
  const [recent, games] = await Promise.all([
    fetch("/api/sessions").then(r => r.json()).catch(() => []),
    fetch("/api/games").then(r => r.json()).catch(() => []),
  ]);
  listEl.innerHTML = "";

  // Recent recordings — newest first, across all games (still listed after a
  // game is removed from the launcher).
  const rh = document.createElement("h2"); rh.textContent = "Recent recordings";
  listEl.appendChild(rh);
  if (!recent.length) {
    const p = document.createElement("p"); p.className = "empty";
    p.textContent = "No recordings yet."; listEl.appendChild(p);
  } else {
    const grid = document.createElement("div"); grid.className = "grid";
    for (const s of recent.slice(0, 12)) {
      const meta = `${fmtTs(s.started_at)} ・ ${fmtDur(s.duration_ms)} ・ ${(s.total_bytes/1e9).toFixed(2)} GB`;
      const c = card(s.game_name, meta);
      c.onclick = () => { location.hash = `#/session/${s.id}`; };
      grid.appendChild(c);
    }
    listEl.appendChild(grid);
  }

  if (games.length) {
    const gh = document.createElement("h2"); gh.textContent = "Games";
    listEl.appendChild(gh);
    const grid = document.createElement("div"); grid.className = "grid";
    for (const g of games) {
      const c = card(g.name, "View sessions →");
      c.onclick = () => { location.hash = `#/game/${g.id}`; };
      grid.appendChild(c);
    }
    listEl.appendChild(grid);
  }
}

async function showSessions(gid) {
  curSession = null;
  backEl.classList.remove("hidden");
  playerEl.classList.add("hidden");
  listEl.classList.remove("hidden");
  const games = await fetch("/api/games").then(r => r.json()).catch(() => []);
  curGame = games.find(g => g.id === gid) || { id: gid, name: gid };
  titleEl.textContent = curGame.name;
  listEl.innerHTML = "Loading…";
  const ss = await fetch(`/api/games/${gid}/sessions`).then(r => r.json()).catch(() => []);
  if (!ss.length) { listEl.innerHTML = "<p class=\"empty\">No recordings for this game.</p>"; return; }
  listEl.innerHTML = "";
  const grid = document.createElement("div"); grid.className = "grid";
  for (const s of ss) {
    const meta = `${fmtTs(s.started_at)} ・ ${fmtDur(s.duration_ms)} ・ ${(s.total_bytes/1e9).toFixed(2)} GB ・ ${s.segment_count} files`;
    const c = card("Session", meta);
    c.onclick = () => { location.hash = `#/session/${s.id}`; };
    grid.appendChild(c);
  }
  listEl.appendChild(grid);
}

async function showPlayer(sid) {
  curSession = sid;
  backEl.classList.remove("hidden");
  listEl.classList.add("hidden");
  playerEl.classList.remove("hidden");
  inMs = outMs = null; updateInOut();

  const useH264 = !hevcOk;
  const m3u8 = `${useH264 ? "/hls264" : "/hls"}/session/${encodeURIComponent(sid)}.m3u8`;
  bannerEl.classList.remove("hidden");
  bannerEl.textContent = useH264
    ? "This device can't decode HEVC directly — transcoding to H.264 (the first load takes a while)…"
    : "Preparing the stream (only the first load takes a moment)…";

  // Poll until the server has produced the playlist (re-segmentation may take a moment).
  await waitForPlaylist(m3u8);
  bannerEl.classList.add("hidden");
  attach(m3u8);
}

// ---- playback -------------------------------------------------------------
async function waitForPlaylist(url) {
  for (let i = 0; i < 150; i++) { // up to ~30s
    try {
      const r = await fetch(url, { cache: "no-store" });
      if (r.ok) return;
    } catch (_) {}
    await sleep(200);
  }
}

function attach(m3u8) {
  if (Hls.isSupported()) {
    hls = new Hls({ enableWorker: true, lowLatencyMode: false });
    hls.loadSource(m3u8);
    hls.attachMedia(videoEl);
    hls.on(Hls.Events.ERROR, (_e, d) => {
      if (d.fatal) bannerEl.classList.remove("hidden"),
        bannerEl.textContent = "Playback error: " + d.details;
    });
  } else if (videoEl.canPlayType("application/vnd.apple.mpegurl")) {
    videoEl.src = m3u8; // Safari native
  } else {
    bannerEl.classList.remove("hidden");
    bannerEl.textContent = "This browser cannot play HLS.";
  }
}

function stopPlayer() {
  if (hls) { hls.destroy(); hls = null; }
  videoEl.removeAttribute("src"); videoEl.load();
}

// double-click to toggle fullscreen
videoEl.addEventListener("dblclick", () => {
  if (document.fullscreenElement) document.exitFullscreen();
  else videoEl.requestFullscreen().catch(() => {});
});

// ---- screenshot (client-side canvas) --------------------------------------
$("snap").onclick = () => {
  if (!videoEl.videoWidth) return;
  const c = document.createElement("canvas");
  c.width = videoEl.videoWidth; c.height = videoEl.videoHeight;
  c.getContext("2d").drawImage(videoEl, 0, 0);
  c.toBlob((b) => {
    const a = document.createElement("a");
    a.href = URL.createObjectURL(b);
    a.download = `shot_${Date.now()}.png`;
    a.click();
  }, "image/png");
};

// ---- clip -----------------------------------------------------------------
$("setin").onclick = () => { inMs = Math.floor(videoEl.currentTime * 1000); updateInOut(); };
$("setout").onclick = () => { outMs = Math.floor(videoEl.currentTime * 1000); updateInOut(); };
function updateInOut() {
  $("inout").textContent = `IN ${inMs==null?"--":fmtDur(inMs)} / OUT ${outMs==null?"--":fmtDur(outMs)}`;
}
$("clip").onclick = async () => {
  if (inMs == null || outMs == null || outMs <= inMs) { $("clipmsg").textContent = "Set IN and OUT first"; return; }
  $("clipmsg").textContent = "Clipping…";
  const body = { session_id: curSession, start_ms: inMs, end_ms: outMs, mode: $("reenc").checked ? "reencode" : "copy" };
  const res = await fetch("/api/clip", { method: "POST", body: JSON.stringify(body) }).then(r => r.json());
  if (!res.job) { $("clipmsg").textContent = "Failed"; return; }
  for (let i = 0; i < 1800; i++) {
    const st = await fetch(`/api/clip/${res.job}`).then(r => r.json());
    if (st.status === "done") {
      $("clipmsg").innerHTML = `Done: <a class="dl" href="${st.url}" download>Download</a>`;
      return;
    }
    if (st.status === "failed") { $("clipmsg").textContent = "Failed: " + (st.error||""); return; }
    await sleep(1000);
  }
  $("clipmsg").textContent = "Timed out";
};

// ---- helpers --------------------------------------------------------------
function card(name, meta) {
  const d = document.createElement("div");
  d.className = "card";
  d.innerHTML = `<span class="name"></span><span class="meta"></span>`;
  d.querySelector(".name").textContent = name;
  d.querySelector(".meta").textContent = meta;
  return d;
}
function fmtTs(ms) {
  const d = new Date(ms);
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}/${p(d.getMonth()+1)}/${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
function fmtDur(ms) {
  const s = Math.max(0, Math.floor(ms/1000));
  const h = Math.floor(s/3600), m = Math.floor((s%3600)/60), ss = s%60;
  const p = (n) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${p(m)}:${p(ss)}` : `${m}:${p(ss)}`;
}
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
