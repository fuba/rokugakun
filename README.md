# ロクガくん（gamerokugakun）

**OBS いらずの Windows ゲーム自動録画ランチャー。** ゲームを起動するだけで、そのウィンドウとそのアプリの音声だけを自動で録画し続けます。すべてローカル完結・純 Rust 製です。

## 特徴

- 🎮 **ゲームを登録して「録画して起動」**、または**自動録画**（対象アプリの起動を検知して自動でアタッチ）
- 🪟 **対象ウィンドウだけ録画**（Windows.Graphics.Capture）。非フルスクリーン時はタイトルバーを除いたクライアント領域のみ。黄色い録画枠は出ません（かわりに画面右上に小さな赤い点が点滅。録画には写りません）
- 🔊 **対象プロセスの音声だけ録音**（WASAPI process loopback）— Discord や BGM は混ざりません
- ⚡ **HEVC (NVENC) + AAC** をハードウェアエンコードし、**MPEG-TS セグメント**（既定 1GB / 10分で切替、キーフレーム境界）として保存。muxer は依存ゼロの純 Rust 実装
- 💾 **容量上限で自動ローテーション**（古いセグメントから削除）。SQLite で録画を管理
- 📺 **ビューワ内蔵**: セッションをまたいで一気通貫再生（ffplay 連結再生）
- 🌐 **ブラウザビューワ + HLS サーバー内蔵**: シークバー / ダブルクリック全画面 / 切り抜き（NVENC 再エンコード or 高速コピー）/ スクリーンショット。LAN に公開されるのでスマホからも視聴できます（HEVC 非対応ブラウザには H.264 へ自動フォールバック）
- ⚙️ 解像度 / fps / ビットレート / レート制御などを全体・ゲーム別に設定可能。ウィンドウが設定より小さい場合は自動フィット

## 動作環境

- Windows 11（Windows.Graphics.Capture / process loopback / `SetIsBorderRequired` を使用）
- ハードウェア HEVC エンコーダを持つ GPU（NVIDIA NVENC で検証済み。Media Foundation 経由なので AMD/Intel の HEVC HW エンコーダでも動く想定）
- **ffmpeg / ffplay**（任意・推奨）: 内蔵ビューワの再生、ブラウザ視聴（HLS 再構成）、切り抜きに使用します。`launcher.exe` と同じフォルダ、scoop（`~/scoop/apps/ffmpeg`）、または PATH から自動検出します。録画そのものには不要です

## 使い方

[Releases](../../releases) から zip をダウンロードして展開し、`rokugakun.exe` をダブルクリックするだけです。

1. 「📁 ファイルを選択...」でゲームの exe / ショートカットを登録（または「⏵ 起動中アプリから登録...」で自動録画を ON に）
2. 「▶ 録画して起動」を押すとゲームが起動し、ウィンドウを検出して録画が始まります
3. ゲームを終了すると録画も自動で止まります
4. 「録画を見る」→「🌐 ブラウザで見る」でシークバー付きのリッチなビューワが開きます

録画ファイルは既定で `%USERPROFILE%\Videos\GameRecordings` に保存されます（変更可）。設定・DB・ログは `%LOCALAPPDATA%\GameRecorder` にあります。

### CLI

```
rokugakun.exe selftest [秒]   # ffplay のテスト映像を録画するセルフテスト
rokugakun.exe serve [秒]      # ビューワサーバーのみ起動
rokugakun.exe list-apps      # 録画対象として検出できる起動中アプリ一覧
```
（ソースからビルドした場合の実行ファイル名は `launcher.exe` です）

## ソースからビルド

Rust (stable, `x86_64-pc-windows-msvc`) と MSVC Build Tools が必要です。

```
cargo build --release -p launcher
# => target/release/launcher.exe
```

```
cargo test --workspace
```

## 構成

| crate | 内容 |
|---|---|
| `crates/core` (`rec-core`) | 設定 / SQLite ストア / 容量ローテーション / ログ |
| `crates/ts-mux` | 依存ゼロの純 Rust MPEG-TS muxer |
| `crates/recorder` | キャプチャ（WGC/D3D11）・エンコード（MF HEVC/AAC）・mux パイプライン |
| `crates/launcher` | egui GUI + 内蔵 Web ビューワ / HLS サーバー |

Web ビューワは [hls.js](https://github.com/video-dev/hls.js)（Apache-2.0）を同梱しています。

## ライセンス

[MIT](LICENSE)
