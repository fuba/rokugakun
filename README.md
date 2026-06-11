# rokugakun

**An OBS-free game auto-recording launcher for Windows.** Launch a game and it
automatically records just that window and just that app's audio — all locally,
written in pure Rust.

## Features

- 🎮 **Register a game and "Record & Launch"**, or **auto-record** (rokugakun
  watches for the app and starts recording the moment it launches).
- 🪟 **Records only the target window** (Windows.Graphics.Capture). For
  non-fullscreen apps it captures the client area only, excluding the title bar.
  No yellow capture border — instead a small red dot blinks at the top-right of
  the screen (it is never part of the recording).
- 🔊 **Records only the target process's audio** (WASAPI process loopback), so
  Discord, music, and notifications never bleed in.
- ⚡ **Hardware HEVC (NVENC via Media Foundation) + AAC**, written to **MPEG-TS
  segments** (1 GB / 10 min by default, split on keyframe boundaries). The muxer
  is a dependency-free pure-Rust implementation.
- 💾 **Capacity-capped retention** — the oldest segments are deleted once the
  storage cap is reached. An SQLite manifest tracks everything.
- 📺 **Built-in viewer** — play across sessions seamlessly (via ffplay's concat
  protocol).
- 🌐 **Built-in web viewer + HLS server** — a fully custom player with a seek
  bar, fullscreen, volume, **clip mode** (set IN/OUT on the timeline, export via
  NVENC re-encode or fast copy), and **screenshots saved server-side** into a
  folder you configure in the app. The server binds to your LAN, so you can
  watch from your phone (HEVC-incapable browsers fall back to H.264
  automatically).
- ⚙️ Resolution / fps / bitrate / rate control and more, configurable globally
  or per game. When a window is smaller than the preset, the output auto-fits.

## Requirements

- Windows 11 (uses Windows.Graphics.Capture, process loopback, and
  `SetIsBorderRequired`).
- A GPU with a hardware HEVC encoder (tested on NVIDIA NVENC; because it goes
  through Media Foundation, AMD/Intel hardware HEVC encoders should work too).
- **ffmpeg / ffplay** (optional but recommended): used for viewer playback,
  browser streaming (HLS re-segmentation), and clip export. rokugakun finds them
  next to `rokugakun.exe`, in scoop (`~/scoop/apps/ffmpeg`), or on `PATH`.
  Recording itself does not need them.

## Getting started

Download the zip from [Releases](../../releases), extract it, and double-click
`rokugakun.exe`.

1. Click **Choose file…** to register a game's exe / shortcut (or **From running
   apps…** to register one with auto-record turned on).
2. Click **Record & Launch** to start the game; rokugakun detects its window and
   begins recording.
3. Quit the game and the recording stops automatically.
4. Open the **Recordings** tab, or click **Web Viewer** for the rich
   browser-based player (seek bar, clip, screenshots).

Recordings are written to `%USERPROFILE%\Videos\GameRecordings` by default
(changeable in Settings). Screenshots taken in the web viewer are saved to
`%USERPROFILE%\Pictures\Rokugakun` by default (also configurable). App
config, the database, and logs live under `%LOCALAPPDATA%\GameRecorder`.

### CLI

```
rokugakun.exe selftest [secs]   # record an ffplay test pattern end-to-end
rokugakun.exe serve [secs]      # run only the web/HLS viewer server
rokugakun.exe list-apps         # list running apps detectable as targets
```

(When built from source the executable is named `launcher.exe`.)

## Building from source

Requires Rust (stable, `x86_64-pc-windows-msvc`) and the MSVC Build Tools.

```
cargo build --release -p launcher
# => target/release/launcher.exe
```

```
cargo test --workspace
```

## Layout

| crate | contents |
|---|---|
| `crates/core` (`rec-core`) | config / SQLite store / capacity retention / logging |
| `crates/ts-mux` | dependency-free pure-Rust MPEG-TS muxer |
| `crates/recorder` | capture (WGC/D3D11), encode (MF HEVC/AAC), mux pipeline |
| `crates/launcher` | egui GUI + embedded web viewer / HLS server |

The web viewer bundles [hls.js](https://github.com/video-dev/hls.js)
(Apache-2.0).

## License

rokugakun is released under the [MIT License](LICENSE).

It builds on open-source Rust crates and bundles a few web assets, all under
permissive licenses (MIT, Apache-2.0, BSD, ISC, Zlib, etc.) — there is no
GPL/LGPL or other copyleft code in the distributed binary. See
[THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) for the full attribution list
and license texts.

FFmpeg is **not** bundled — rokugakun calls it as an external program if it is
present — so FFmpeg's own license does not apply to rokugakun. HEVC/H.264 codec
patent licensing for your use is your responsibility.
