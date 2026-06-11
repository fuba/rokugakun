"use strict";

const $ = (id) => document.getElementById(id);
const listEl = $("list"), playerEl = $("player"), videoEl = $("video");
const titleEl = $("title"), backEl = $("back"), bannerEl = $("banner");
const stageEl = $("stage");

let hls = null;
let curGame = null;          // {id, name}
let curSession = null;       // session id
let curLabel = "clip";       // label used for screenshot/clip filenames
let clipMode = false;
let inMs = null, outMs = null;
let scrubbing = null;        // "seek" | "in" | "out" | null

const hevcOk = (window.MediaSource &&
  MediaSource.isTypeSupported('video/mp4; codecs="hvc1.1.6.L150.B0"'));

// ---- routing --------------------------------------------------------------
window.addEventListener("hashchange", route);
window.addEventListener("DOMContentLoaded", () => { wirePlayer(); route(); });
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

// ---- list views -----------------------------------------------------------
async function showGames() {
  curSession = null; curGame = null;
  titleEl.textContent = "rokugakun";
  backEl.classList.add("hidden");
  playerEl.classList.add("hidden");
  listEl.classList.remove("hidden");
  listEl.innerHTML = "Loading…";
  const [recent, games] = await Promise.all([
    fetch("/api/sessions").then(r => r.json()).catch(() => []),
    fetch("/api/games").then(r => r.json()).catch(() => []),
  ]);
  listEl.innerHTML = "";

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
  resetClip();
  setClipMode(false);
  $("shotmsg").textContent = "";
  stageEl.classList.add("show-controls"); // visible until playback starts

  // Resolve a friendly label (game name) for screenshot/clip filenames.
  curLabel = (curGame && curGame.name) || "clip";
  fetch("/api/sessions").then(r => r.json()).then(list => {
    const m = (list || []).find(s => s.id === sid);
    if (m) { curLabel = m.game_name; titleEl.textContent = m.game_name; }
  }).catch(() => {});

  const useH264 = !hevcOk;
  const m3u8 = `${useH264 ? "/hls264" : "/hls"}/session/${encodeURIComponent(sid)}.m3u8`;
  bannerEl.classList.remove("hidden");
  bannerEl.textContent = useH264
    ? "This device can't decode HEVC directly — transcoding to H.264 (the first load takes a while)…"
    : "Preparing the stream (only the first load takes a moment)…";

  await waitForPlaylist(m3u8);
  bannerEl.classList.add("hidden");
  attach(m3u8);
}

// ---- stream ---------------------------------------------------------------
async function waitForPlaylist(url) {
  for (let i = 0; i < 150; i++) { // up to ~30s
    try { const r = await fetch(url, { cache: "no-store" }); if (r.ok) return; } catch (_) {}
    await sleep(200);
  }
}

function attach(m3u8) {
  if (Hls.isSupported()) {
    hls = new Hls({ enableWorker: true, lowLatencyMode: false });
    hls.loadSource(m3u8);
    hls.attachMedia(videoEl);
    hls.on(Hls.Events.ERROR, (_e, d) => {
      if (d.fatal) { bannerEl.classList.remove("hidden"); bannerEl.textContent = "Playback error: " + d.details; }
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

// ---- custom player controls ----------------------------------------------
function wirePlayer() {
  const seek = $("seek");

  $("play").onclick = togglePlay;
  videoEl.addEventListener("click", togglePlay);
  videoEl.addEventListener("dblclick", toggleFullscreen);
  $("full").onclick = toggleFullscreen;

  videoEl.addEventListener("play", () => { $("play").textContent = "⏸"; stageEl.classList.remove("show-controls"); });
  videoEl.addEventListener("pause", () => { $("play").textContent = "▶"; stageEl.classList.add("show-controls"); });
  videoEl.addEventListener("timeupdate", renderProgress);
  videoEl.addEventListener("progress", renderProgress);
  videoEl.addEventListener("durationchange", renderProgress);
  videoEl.addEventListener("loadedmetadata", renderProgress);

  $("mute").onclick = () => { videoEl.muted = !videoEl.muted; reflectVolume(); };
  $("vol").oninput = (e) => { videoEl.muted = false; videoEl.volume = parseFloat(e.target.value); reflectVolume(); };

  // scrub / clip-handle dragging on the shared timeline
  seek.addEventListener("pointerdown", (e) => {
    const t = e.target;
    if (t === $("handleIn")) scrubbing = "in";
    else if (t === $("handleOut")) scrubbing = "out";
    else scrubbing = "seek";
    seek.setPointerCapture(e.pointerId);
    onScrub(e);
  });
  seek.addEventListener("pointermove", (e) => { if (scrubbing) onScrub(e); });
  seek.addEventListener("pointerup", (e) => { scrubbing = null; try { seek.releasePointerCapture(e.pointerId); } catch (_) {} });

  // clip controls
  $("clipToggle").onclick = () => setClipMode(!clipMode);
  $("setin").onclick = () => { inMs = Math.floor(videoEl.currentTime * 1000); clampClip(); renderClip(); };
  $("setout").onclick = () => { outMs = Math.floor(videoEl.currentTime * 1000); clampClip(); renderClip(); };
  $("playclip").onclick = () => { if (inMs != null) { videoEl.currentTime = inMs / 1000; videoEl.play(); } };
  $("export").onclick = exportClip;

  // screenshot -> POST to the app daemon (saved in the app-configured folder)
  $("snap").onclick = saveScreenshot;

  // keyboard shortcuts
  document.addEventListener("keydown", (e) => {
    if (playerEl.classList.contains("hidden")) return;
    if (e.target.tagName === "INPUT") return;
    switch (e.key) {
      case " ": case "k": e.preventDefault(); togglePlay(); break;
      case "f": toggleFullscreen(); break;
      case "m": videoEl.muted = !videoEl.muted; reflectVolume(); break;
      case "c": setClipMode(!clipMode); break;
      case "s": saveScreenshot(); break;
      case "i": inMs = Math.floor(videoEl.currentTime * 1000); clampClip(); renderClip(); break;
      case "o": outMs = Math.floor(videoEl.currentTime * 1000); clampClip(); renderClip(); break;
      case "ArrowLeft": videoEl.currentTime = Math.max(0, videoEl.currentTime - 5); break;
      case "ArrowRight": videoEl.currentTime += 5; break;
    }
  });

  reflectVolume();
}

function togglePlay() { if (videoEl.paused) videoEl.play(); else videoEl.pause(); }
function toggleFullscreen() {
  if (document.fullscreenElement) document.exitFullscreen();
  else stageEl.requestFullscreen().catch(() => {});
}
function reflectVolume() {
  $("mute").textContent = (videoEl.muted || videoEl.volume === 0) ? "🔇" : "🔊";
  $("vol").value = videoEl.muted ? 0 : videoEl.volume;
}

function dur() { return videoEl.duration && isFinite(videoEl.duration) ? videoEl.duration : 0; }

function onScrub(e) {
  const seek = $("seek");
  const rect = seek.getBoundingClientRect();
  const frac = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
  const ms = frac * dur() * 1000;
  if (scrubbing === "seek") { videoEl.currentTime = ms / 1000; }
  else if (scrubbing === "in") { inMs = ms; clampClip(); }
  else if (scrubbing === "out") { outMs = ms; clampClip(); }
  renderProgress(); renderClip();
}

function renderProgress() {
  const d = dur();
  const cur = videoEl.currentTime || 0;
  const pf = d ? (cur / d) * 100 : 0;
  $("played").style.width = pf + "%";
  $("thumb").style.left = pf + "%";
  try {
    if (videoEl.buffered.length) {
      const end = videoEl.buffered.end(videoEl.buffered.length - 1);
      $("buffered").style.width = (d ? (end / d) * 100 : 0) + "%";
    }
  } catch (_) {}
  $("time").textContent = `${fmtClock(cur)} / ${fmtClock(d)}`;
}

// ---- clip mode ------------------------------------------------------------
function setClipMode(on) {
  clipMode = on;
  $("clipToggle").classList.toggle("active", on);
  $("clipPanel").classList.toggle("hidden", !on);
  $("clipRange").classList.toggle("hidden", !on);
  $("handleIn").classList.toggle("hidden", !on);
  $("handleOut").classList.toggle("hidden", !on);
  if (on && inMs == null && outMs == null) {
    // default selection: a 15s window around the current time
    const cur = videoEl.currentTime * 1000;
    inMs = Math.max(0, cur - 5000);
    outMs = cur + 10000;
    clampClip();
  }
  renderClip();
}

function resetClip() { inMs = null; outMs = null; $("clipmsg").textContent = ""; }

function clampClip() {
  const dMs = dur() * 1000;
  if (inMs != null) inMs = Math.max(0, Math.min(inMs, dMs || inMs));
  if (outMs != null) outMs = Math.max(0, Math.min(outMs, dMs || outMs));
  if (inMs != null && outMs != null && outMs < inMs) { const t = inMs; inMs = outMs; outMs = t; }
}

function renderClip() {
  if (!clipMode) return;
  const d = dur() * 1000;
  const inf = (inMs != null && d) ? (inMs / d) * 100 : 0;
  const outf = (outMs != null && d) ? (outMs / d) * 100 : 100;
  $("clipRange").style.left = inf + "%";
  $("clipRange").style.width = Math.max(0, outf - inf) + "%";
  $("handleIn").style.left = inf + "%";
  $("handleOut").style.left = outf + "%";
  const len = (inMs != null && outMs != null) ? Math.max(0, outMs - inMs) : 0;
  $("clipInfo").textContent =
    `IN ${inMs == null ? "--" : fmtClock(inMs/1000)} · OUT ${outMs == null ? "--" : fmtClock(outMs/1000)} · length ${fmtClock(len/1000)}`;
}

async function exportClip() {
  if (inMs == null || outMs == null || outMs <= inMs) { $("clipmsg").textContent = "Set IN and OUT first"; return; }
  const bar = $("clipbar"), msg = $("clipmsg");
  const reenc = $("reenc").checked;
  bar.classList.remove("hidden"); bar.value = 0;
  msg.textContent = reenc ? "Exporting… 0%" : "Exporting…";
  const body = {
    session_id: curSession, start_ms: Math.floor(inMs), end_ms: Math.floor(outMs),
    mode: reenc ? "reencode" : "copy", title: curLabel,
  };
  const res = await fetch("/api/clip", { method: "POST", body: JSON.stringify(body) }).then(r => r.json()).catch(() => ({}));
  if (!res.job) { msg.textContent = "Failed to start"; bar.classList.add("hidden"); return; }
  for (let i = 0; i < 3600; i++) {
    const st = await fetch(`/api/clip/${res.job}`).then(r => r.json()).catch(() => ({}));
    if (st.status === "running") {
      const pct = Math.round((st.progress || 0) * 100);
      bar.value = pct; msg.textContent = `Exporting… ${pct}%`;
    } else if (st.status === "done") {
      bar.value = 100; bar.classList.add("hidden");
      const name = st.name || "clip.mp4";
      msg.innerHTML = `Saved <code>${name}</code> — <a class="dl" href="${st.url}" download="${name}">download</a>`;
      return;
    } else if (st.status === "failed") {
      bar.classList.add("hidden"); msg.textContent = "Failed: " + (st.error || ""); return;
    }
    await sleep(500);
  }
  bar.classList.add("hidden"); msg.textContent = "Timed out";
}

// ---- screenshot (sent to the app daemon, saved in the configured folder) --
async function saveScreenshot() {
  if (!videoEl.videoWidth) { $("shotmsg").textContent = "No frame to capture yet."; return; }
  $("shotmsg").textContent = "Saving screenshot…";
  const c = document.createElement("canvas");
  c.width = videoEl.videoWidth; c.height = videoEl.videoHeight;
  c.getContext("2d").drawImage(videoEl, 0, 0);
  c.toBlob(async (blob) => {
    if (!blob) { $("shotmsg").textContent = "Capture failed."; return; }
    try {
      const r = await fetch(`/api/screenshot?name=${encodeURIComponent(curLabel)}`, {
        method: "POST", headers: { "Content-Type": "image/png" }, body: blob,
      });
      const j = await r.json();
      $("shotmsg").textContent = j.ok ? `Screenshot saved: ${j.path}` : "Save failed.";
    } catch (e) {
      $("shotmsg").textContent = "Save failed: " + e;
    }
  }, "image/png");
}

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
function fmtClock(sec) {
  sec = Math.max(0, Math.floor(sec || 0));
  const h = Math.floor(sec/3600), m = Math.floor((sec%3600)/60), s = sec%60;
  const p = (n) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${p(m)}:${p(s)}` : `${m}:${p(s)}`;
}
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
