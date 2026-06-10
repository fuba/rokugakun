以下は、指定された MVP を前提にした軽量な設計案です。
方針は **OBS なし・ローカル完結・ゲーム起動前に録画パイプラインを先に立ち上げる** です。

# Windows ゲーム録画ランチャー MVP 設計

## 1. 目的

Windows 上で、ユーザーがランチャーからゲームを起動すると同時に、そのゲームウィンドウと対象プロセスの音声だけを録画する。

録画は HEVC + AAC を MPEG-TS セグメントとして保存し、SQLite manifest で管理する。保存容量が上限を超えた場合は、古いセグメントから削除する。

## 2. 基本方針

重要な設計判断は以下です。

```text
- 単一巨大ファイルにはしない
- MPEG-TS セグメント単位で保存する
- 古いデータは segment ファイル単位で削除する
- ゲーム起動前に録画準備を完了する
- 初回だけ対象ウィンドウ選択を許容する
- 音声は対象プロセス単位で分離する
- 映像は HEVC に固定する
- AV1 は MVP では扱わない
```

MVP では「完全自動の Game Pass ゲーム検出」までは狙わない。
ゲーム起動コマンド、対象ウィンドウの識別ルール、音声対象プロセスの推定を登録し、次回以降に再利用する。

## 3. 全体構成

```text
+-------------------------+
| Launcher UI             |
|-------------------------|
| - ゲーム一覧            |
| - 録画プリセット        |
| - 容量上限              |
| - 保存先                |
| - 起動ボタン            |
+------------+------------+
             |
             v
+-------------------------+
| Launcher Service        |
|-------------------------|
| - ゲーム起動            |
| - ウィンドウ検出        |
| - プロセス検出          |
| - Recorder Core 起動    |
| - セッション管理        |
+------------+------------+
             |
             v
+-------------------------+
| Recorder Core           |
|-------------------------|
| - Window Capture        |
| - Audio Capture         |
| - HEVC Encode           |
| - AAC Encode            |
| - MPEG-TS Mux           |
| - Segment Writer        |
| - Retention Manager     |
+------------+------------+
             |
             v
+-------------------------+
| Storage                 |
|-------------------------|
| - SQLite manifest       |
| - .ts segments          |
| - thumbnails/logs       |
+-------------------------+
```

プロセス分離は以下のどちらか。

```text
案A: launcher.exe と recorder.exe を分ける
案B: launcher.exe 内に recorder core を持つ
```

MVP では **案A** を推奨する。

理由:

```text
- 録画コアがクラッシュしても UI を巻き込まない
- C++ recorder と UI 実装を分離しやすい
- 将来、UI を Go/Wails・Tauri・C# などに変えられる
- 録画コアを管理者権限で動かす必要が出ても分離できる
```

## 4. プロセス構成

```text
launcher.exe
  - UI
  - ゲーム登録管理
  - 録画プリセット管理
  - recorder.exe 起動
  - ゲーム起動
  - IPC 制御

recorder.exe
  - Windows.Graphics.Capture
  - Application Loopback Audio
  - HEVC encoder
  - AAC encoder
  - MPEG-TS segment writer
  - SQLite manifest 更新
  - 容量削除

game.exe
  - 実際のゲーム
```

IPC は MVP では **Named Pipe** で十分。

```text
\\.\pipe\game-recorder-control
```

メッセージは JSON Lines でよい。

```json
{"type":"start","session_id":"...","window":{"hwnd":"..."},"audio":{"pid":1234}}
{"type":"stop"}
{"type":"status"}
{"type":"mark_protected","segment_id":123}
```

## 5. 起動シーケンス

「起動した瞬間から録画できる」を満たすには、ゲーム起動より前に録画パイプラインを準備する。

ただし、Windows.Graphics.Capture でウィンドウ単位録画するには対象ウィンドウが必要なので、完全には矛盾がある。
そのため MVP では次の2段階にする。

### 初回起動

```text
1. ユーザーがゲームを登録する
2. 起動コマンドを登録する
3. ランチャーがゲームを起動する
4. ゲームウィンドウが出たら一覧から対象を選ばせる
5. hwnd / exe / title / class / AUMID / process name を保存
6. 以後の自動録画に使う
```

初回は「起動した瞬間から完全録画」は諦める。
これは Windows.Graphics.Capture のウィンドウ対象が存在しないと始められないため。

### 2回目以降

```text
1. ユーザーがランチャーでゲームを選ぶ
2. recorder.exe を起動する
3. 音声キャプチャ準備を開始する
4. 映像キャプチャは pending 状態にする
5. ゲームを起動する
6. ウィンドウ検出ルールで対象 hwnd を探す
7. 見つかった瞬間に Windows.Graphics.Capture を開始する
8. 同時に HEVC/AAC/MPEG-TS 書き込みを開始する
9. ゲーム終了検知で録画停止する
```

厳密には、ゲームプロセス生成からウィンドウ出現までの間は録画できない。
しかし「ユーザーがゲームを操作可能になる瞬間」には録画開始できる設計にする。

## 6. ゲーム登録モデル

SQLite にゲーム情報を持つ。

```sql
CREATE TABLE games (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    launch_command TEXT NOT NULL,
    launch_workdir TEXT,
    launch_args TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

ウィンドウ識別ルール。

```sql
CREATE TABLE game_window_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id TEXT NOT NULL,
    exe_path TEXT,
    process_name TEXT,
    window_title_pattern TEXT,
    window_class TEXT,
    app_user_model_id TEXT,
    preferred_monitor_index INTEGER,
    last_hwnd INTEGER,
    confidence INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(game_id) REFERENCES games(id)
);
```

Game Pass では exe path が安定しない場合があるため、複数情報を併用する。

優先順位:

```text
1. AppUserModelID
2. process image path
3. process name
4. window class
5. title regex
6. 最後に選択された hwnd の特徴
```

## 7. セッションモデル

```sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    game_id TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    codec_video TEXT NOT NULL,
    codec_audio TEXT NOT NULL,
    container TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    fps_num INTEGER,
    fps_den INTEGER,
    bitrate_video INTEGER,
    bitrate_audio INTEGER,
    storage_root TEXT NOT NULL,
    status TEXT NOT NULL,
    FOREIGN KEY(game_id) REFERENCES games(id)
);
```

セグメント。

```sql
CREATE TABLE segments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    path TEXT NOT NULL,
    index_no INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    duration_ms INTEGER,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    start_pts INTEGER,
    end_pts INTEGER,
    protected INTEGER NOT NULL DEFAULT 0,
    closed INTEGER NOT NULL DEFAULT 0,
    deleted INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(session_id) REFERENCES sessions(id)
);
```

容量管理用に index を張る。

```sql
CREATE INDEX idx_segments_gc
ON segments(deleted, protected, closed, started_at);
```

## 8. 録画プリセット

MVP では HEVC 固定。
NVIDIA / AMD の違いは encoder backend として抽象化する。

```json
{
  "id": "hevc_1440p60_high",
  "name": "HEVC 1440p60 High",
  "video": {
    "codec": "hevc",
    "backend": "auto",
    "width": 2560,
    "height": 1440,
    "fps": 60,
    "bitrate_mbps": 35,
    "keyframe_interval_sec": 2,
    "b_frames": 0,
    "rate_control": "cbr"
  },
  "audio": {
    "codec": "aac",
    "sample_rate": 48000,
    "channels": 2,
    "bitrate_kbps": 192
  },
  "segment": {
    "max_size_mb": 1024,
    "max_duration_sec": 600
  },
  "retention": {
    "max_total_gb": 300
  }
}
```

MVP では CBR 推奨。

理由:

```text
- セグメントサイズ予測がしやすい
- 容量上限から保持時間を見積もりやすい
- 実装が単純
```

品質優先なら VBR でもよいが、保持時間予測が少し荒くなる。

## 9. Window Capture

### API

```text
Windows.Graphics.Capture
Direct3D11
```

### 入力

```text
HWND
または GraphicsCaptureItem
```

### フレーム取得

```text
Direct3D11CaptureFramePool
GraphicsCaptureSession
Direct3D11CaptureFrame
ID3D11Texture2D
```

録画コア内部では、キャプチャフレームを D3D11 texture として受け取る。

```cpp
struct VideoFrame {
    Microsoft::WRL::ComPtr<ID3D11Texture2D> texture;
    int width;
    int height;
    int64_t qpc_time;
    int64_t pts_100ns;
};
```

### フレームタイミング

基本方針:

```text
- キャプチャ API から来たフレーム時刻を基準にする
- エンコーダには一定 fps として渡す
- ドロップ/重複で 60fps に整える
```

MVP では可変 fps にしない。

```text
target fps = 60
frame interval = 16.666ms
```

キャプチャが遅れた場合:

```text
- 軽微な遅れ: 前フレーム複製
- 大きな遅れ: フレームドロップ
```

## 10. Audio Capture

### API

```text
Application Loopback Capture
WASAPI
```

対象は原則としてゲームプロセス。

入力:

```text
target process id
include process tree = true
```

Game Pass では実際の音声プロセスが起動コマンドの子ではない可能性がある。
そのため、MVP では次の順に決める。

```text
1. 対象ウィンドウの GetWindowThreadProcessId から PID を取得
2. その PID の process tree を音声対象にする
3. 音声が検出できない場合、ユーザーに音声対象プロセスを選ばせる
4. 最後の fallback として system loopback
```

MVP 要件では音声分離必須なので、system loopback は明示的な emergency fallback 扱い。

音声フレーム。

```cpp
struct AudioFrame {
    std::vector<float> samples; // interleaved float32
    uint32_t sample_rate;
    uint16_t channels;
    int64_t qpc_time;
    int64_t pts_100ns;
};
```

内部処理は float32 で受け、AAC encoder 前に必要な形式へ変換する。

## 11. 同期設計

映像と音声は共通クロックに合わせる。

```text
基準: QueryPerformanceCounter
単位: 100ns または 90kHz
```

内部では 100ns tick を使う。

```cpp
int64_t qpc_to_100ns(int64_t qpc) {
    return (qpc - qpc_base) * 10'000'000 / qpc_frequency;
}
```

MPEG-TS へ書くときに 90kHz PTS に変換する。

```cpp
int64_t pts90k = pts100ns * 90000 / 10000000;
```

セッション開始時:

```text
session_base_qpc = now
video_base_pts = 0
audio_base_pts = 0
```

同期方針:

```text
- 映像は 60fps grid に乗せる
- 音声は連続サンプル数から PTS を進める
- muxer は PTS 順に packet を出す
- PCR は video PID を基準にする
```

## 12. HEVC Encoder

### 抽象インターフェイス

```cpp
class IVideoEncoder {
public:
    virtual ~IVideoEncoder() = default;

    virtual bool open(const VideoEncoderConfig& config) = 0;
    virtual bool encode(const VideoFrame& frame, std::vector<EncodedPacket>& out) = 0;
    virtual bool flush(std::vector<EncodedPacket>& out) = 0;
    virtual void close() = 0;
};
```

出力 packet。

```cpp
struct EncodedPacket {
    std::vector<uint8_t> data;
    int64_t pts90k;
    int64_t dts90k;
    bool keyframe;
    StreamType stream_type;
};
```

### backend

```text
NVIDIA: NVENC HEVC
AMD: AMF HEVC
fallback: Media Foundation HEVC
```

MVP で無理に NVIDIA/AMD を完全抽象化しようとすると重い。
現実的にはこうする。

```text
v0.1: NVENC HEVC
v0.2: AMF HEVC
v0.3: Media Foundation fallback
```

ただし設計上は interface で切れるようにしておく。

### GOP

```text
keyframe interval = 2秒
fps 60なら GOP = 120 frames
B-frame = 0
```

B-frame を使わない理由:

```text
- DTS/PTS 管理が単純
- 低遅延
- segment 切り替えが楽
```

画質効率は落ちるが、MVP では正しさを優先する。

## 13. AAC Encoder

選択肢:

```text
- Media Foundation AAC encoder
- libavcodec AAC encoder
```

MVP では **Media Foundation AAC encoder** か **libavcodec** のどちらか。
TS mux まで libavformat に寄せるなら、AAC も libavcodec に寄せると実装は楽。

ただし Windows ネイティブ完結を重視するなら Media Foundation。

AAC packet は MPEG-TS に載せるため、ADTS 付きにするか、muxer 側で ADTS/PES 化する。

MVP では:

```text
AAC-LC
48kHz
2ch
192kbps
ADTS framing
```

## 14. MPEG-TS Segment Muxer

### 基本

出力は `.ts` セグメント。

```text
seg_000001.ts
seg_000002.ts
seg_000003.ts
```

各 segment は単独再生可能にする。

segment 先頭に必ず出すもの:

```text
- PAT
- PMT
- HEVC codec configuration near first keyframe
- AAC stream
```

### PID

固定でよい。

```text
PAT PID: 0x0000
PMT PID: 0x1000
Video PID: 0x0100
Audio PID: 0x0101
PCR PID: Video PID
```

### segment 切替条件

```text
if current_segment.size >= max_size
    request_rotate = true

if current_segment.duration >= max_duration
    request_rotate = true

if request_rotate && next_video_packet.keyframe
    close current segment
    open next segment
```

つまり、サイズが 1GB を超えた瞬間に即切断しない。
**次の keyframe まで待って切る。**

### 書き込み単位

```cpp
class SegmentMuxer {
public:
    bool open_segment(const SegmentInfo& info);
    bool write_video(const EncodedPacket& packet);
    bool write_audio(const EncodedPacket& packet);
    bool rotate_if_needed(const EncodedPacket& next_video_packet);
    bool close_segment();
};
```

### 自前 muxer か libavformat か

MVP では **libavformat 推奨**。

理由:

```text
- HEVC in TS の細部を自前で持たなくてよい
- PAT/PMT/PCR/PES のバグを避けられる
- AAC との同期を既存 muxer に任せられる
```

ただし設計上は muxer interface を切る。

```cpp
class IMuxer {
public:
    virtual bool open(const MuxerConfig& config) = 0;
    virtual bool write_packet(const EncodedPacket& packet) = 0;
    virtual bool close() = 0;
};
```

将来、TS muxer を自前化できる。

## 15. Segment Writer

Muxer は「現在のファイルに書く」だけ。
Segment Writer がローテーションと DB 更新を担当する。

```cpp
class SegmentWriter {
public:
    bool start_session(const SessionConfig& config);
    bool write_packet(const EncodedPacket& packet);
    bool stop_session();
private:
    void open_new_segment();
    void close_current_segment();
    bool should_rotate() const;
};
```

ファイル名。

```text
{game_slug}_{session_start}_{segment_index}.ts
```

例:

```text
Forza_20260608_221500_000001.ts
Forza_20260608_221500_000002.ts
```

一時ファイルとして書き、close 時に rename する。

```text
seg_000001.ts.writing
→ seg_000001.ts
```

録画中クラッシュ時に `.writing` が残る。
次回起動時に recovery 処理を行う。

## 16. SQLite manifest

SQLite は録画の索引であり、真実のソースはファイルシステムと合わせて扱う。

起動時に整合性チェックする。

```text
- DB にあるがファイルがない segment → deleted 扱い
- ファイルがあるが DB にない segment → orphan として登録または隔離
- .writing が残っている segment → incomplete として扱う
```

セッション開始:

```sql
INSERT INTO sessions (...)
```

セグメント開始:

```sql
INSERT INTO segments (..., closed=0, deleted=0)
```

セグメント close:

```sql
UPDATE segments
SET ended_at=?, duration_ms=?, size_bytes=?, closed=1
WHERE id=?
```

削除:

```sql
UPDATE segments
SET deleted=1
WHERE id=?
```

物理削除と DB 更新は原則として transaction で扱いたいが、ファイル削除は DB transaction に含められない。
MVP では以下の順序にする。

```text
1. segment を deleting 状態にする
2. ファイル削除
3. deleted=1 にする
```

追加カラム:

```sql
ALTER TABLE segments ADD COLUMN deleting INTEGER NOT NULL DEFAULT 0;
```

## 17. 容量ローテーション

対象は storage root 全体。

```text
D:\GameRecordings
```

ゲーム別上限は MVP では不要。
全体上限のみ。

削除対象:

```text
- closed = 1
- protected = 0
- deleted = 0
- deleting = 0
- 現在録画中セッションの active segment ではない
```

削除順:

```text
started_at ASC
```

ロジック:

```cpp
void RetentionManager::cleanup() {
    auto total = calc_total_size();

    while (total > max_total_bytes) {
        auto seg = find_oldest_deletable_segment();
        if (!seg) break;

        mark_deleting(seg.id);
        if (delete_file(seg.path)) {
            mark_deleted(seg.id);
            total -= seg.size_bytes;
        } else {
            clear_deleting_or_mark_error(seg.id);
            break;
        }
    }
}
```

実行タイミング:

```text
- segment close 後
- セッション開始時
- アプリ起動時
```

録画中に現在の segment は消さない。

## 18. クリップ保護

MVP でも `protected` は入れておく。

UI で「このセッションを保護」または「直近 N 分を保護」を実装できる。

直近 N 分保護:

```sql
UPDATE segments
SET protected = 1
WHERE session_id = ?
  AND ended_at >= ?
  AND deleted = 0;
```

## 19. ランチャー UI

MVP 画面。

```text
[ゲーム一覧]

Forza Horizon 5
  プリセット: HEVC 1440p60 High
  保存上限: 300GB
  [録画して起動]

Starfield
  プリセット: HEVC 4K60
  保存上限: 500GB
  [録画して起動]
```

録画中画面。

```text
録画中: Forza Horizon 5
経過時間: 00:42:10
現在 segment: 000013
保存容量: 184.2GB / 300GB
推定保持時間: 6h 12m
音声: ForzaHorizon5.exe
映像: Forza Horizon 5 window
[録画停止]
[直近5分を保護]
[このセッションを保護]
```

初回ウィンドウ選択画面。

```text
対象ウィンドウを選択:

- Forza Horizon 5 [ForzaHorizon5.exe]
- Xbox [XboxPcApp.exe]
- Microsoft Store [WinStore.App.exe]
```

## 20. ゲーム起動直後録画の実装

厳密な順序。

```text
1. User presses "録画して起動"
2. launcher creates session record
3. launcher starts recorder.exe in pending mode
4. recorder initializes:
   - D3D device
   - encoder
   - audio subsystem
   - muxer not opened yet
5. launcher starts game command
6. launcher monitors windows
7. when target window appears:
   - send hwnd to recorder
8. recorder starts Windows.Graphics.Capture
9. first video frame arrives
10. audio target pid is resolved
11. Application Loopback starts
12. first keyframe is encoded
13. segment_000001.ts.writing opened
14. PAT/PMT + video/audio packets written
```

この設計では「ゲームプロセス起動直後」ではなく「ゲームウィンドウ出現直後」から録画になる。

ゲーム起動コマンドの直後から音声だけ開始することはできるが、映像がないため TS に入れるには扱いが面倒になる。
MVP では映像フレーム到着を session media start とする。

## 21. Game Pass 対応

Game Pass では launch command が特殊になる可能性がある。

MVP では以下のどれかを登録可能にする。

```text
- 通常 exe
- .lnk
- shell:AppsFolder の AppUserModelID
- URI
- 任意コマンド
```

ゲーム起動は `ShellExecuteEx` を使う。

```cpp
ShellExecuteExW(&sei);
```

プロセス ID が取れない起動方式もあるため、launcher は起動後にウィンドウ検出を行う。

ウィンドウ検出。

```text
EnumWindows
GetWindowThreadProcessId
GetWindowText
GetClassName
QueryFullProcessImageName
Package identity / AUMID
```

## 22. エラー処理

### 映像キャプチャ失敗

```text
- 対象ウィンドウがない
- Capture API が拒否
- 黒画面
- サイズ 0
- D3D device lost
```

MVP 対応:

```text
- ユーザーに再選択させる
- recorder を再初期化
- セッションは継続
```

### 音声キャプチャ失敗

```text
- 対象 PID が音を出していない
- Application Loopback が対象を掴めない
- デバイス変更
```

MVP 対応:

```text
- 音声対象プロセスの再選択
- 失敗時は録画を止める
```

音声分離必須なので、勝手に system loopback に落とさない。

### encoder 失敗

```text
- NVENC 初期化失敗
- AMF 初期化失敗
- 解像度非対応
- driver 問題
```

MVP 対応:

```text
- 他 backend へ fallback
- それも失敗したら録画開始しない
```

### segment 書き込み失敗

```text
- ディスク full
- permission denied
- 保存先消失
```

MVP 対応:

```text
- 録画停止
- session status = error
- UI に表示
```

## 23. スレッド設計

```text
Main/UI thread
  - launcher UI

Launcher monitor thread
  - game process/window detection

Recorder control thread
  - IPC
  - state machine

Video capture thread
  - Windows.Graphics.Capture frame arrival

Audio capture thread
  - WASAPI/Application Loopback

Encode thread
  - HEVC encode

Audio encode thread
  - AAC encode

Mux/write thread
  - packet ordering
  - MPEG-TS write
  - segment rotation

Retention thread
  - old segment deletion
```

MVP では mux/write thread を単一にする。
video/audio encoder から出た packet は timestamp 付き queue に入れる。

```cpp
ConcurrentQueue<EncodedPacket> packet_queue;
```

mux thread が PTS 順に取り出して書く。

ただし映像と音声が完全に PTS 順で来るとは限らないので、短い reorder buffer を持つ。

```text
mux_delay = 500ms
```

## 24. Recorder state machine

```text
Idle
  ↓
Preparing
  ↓
WaitingForWindow
  ↓
Capturing
  ↓
Stopping
  ↓
Stopped

Error
```

状態遷移。

```text
Idle -> Preparing
  recorder 起動

Preparing -> WaitingForWindow
  encoder/audio/D3D 初期化完了

WaitingForWindow -> Capturing
  hwnd 取得、capture 開始、最初の frame 到達

Capturing -> Stopping
  game 終了 or user stop

Stopping -> Stopped
  encoder flush、segment close、DB 更新

Any -> Error
  致命的エラー
```

## 25. ディレクトリ構成

```text
C:\Users\<user>\AppData\Local\GameRecorder\
  config.json
  recorder.db
  logs\
  presets\
  temp\

D:\GameRecordings\
  ForzaHorizon5\
    20260608_221500\
      session.json
      seg_000001.ts
      seg_000002.ts
      seg_000003.ts
```

DB は AppData 側、動画は録画保存先。
session.json は DB が壊れたときの最低限の復旧用。

## 26. C++ モジュール構成

```text
src/
  launcher/
    main.cpp
    game_registry.cpp
    window_detector.cpp
    process_util.cpp
    ipc_client.cpp

  recorder/
    main.cpp
    recorder_service.cpp
    state_machine.cpp
    ipc_server.cpp

  capture/
    window_capture_wgc.cpp
    d3d_device.cpp

  audio/
    app_loopback_capture.cpp
    audio_resampler.cpp

  encode/
    video_encoder.h
    nvenc_hevc_encoder.cpp
    amf_hevc_encoder.cpp
    aac_encoder_mf.cpp

  mux/
    muxer.h
    mpegts_muxer_libav.cpp
    segment_writer.cpp

  storage/
    sqlite_store.cpp
    retention_manager.cpp
    file_util.cpp

  common/
    config.cpp
    logging.cpp
    timebase.cpp
    errors.cpp
```

## 27. 主要データ構造

```cpp
struct RecordingPreset {
    std::string id;
    int width;
    int height;
    int fps_num;
    int fps_den;
    int video_bitrate;
    int audio_bitrate;
    int keyframe_interval_sec;
    int segment_max_size_bytes;
    int segment_max_duration_sec;
    int64_t retention_max_bytes;
    EncoderBackend backend;
};
```

```cpp
struct GameConfig {
    std::string id;
    std::wstring name;
    std::wstring launch_command;
    std::wstring launch_args;
    std::wstring working_directory;
    WindowRule window_rule;
};
```

```cpp
struct WindowRule {
    std::wstring process_name;
    std::wstring exe_path;
    std::wstring title_regex;
    std::wstring class_name;
    std::wstring app_user_model_id;
};
```

```cpp
struct SessionConfig {
    std::string session_id;
    std::string game_id;
    RecordingPreset preset;
    std::filesystem::path output_dir;
};
```

## 28. MVP で切り捨てるもの

MVP に入れないもの。

```text
- AV1
- HDR 正規対応
- Present hook
- デスクトップ全体録画
- 複数音声トラック
- マイク録音
- 自動黒画面検出
- クラウドアップロード
- 高度な編集 UI
- Steam / Xbox / Playnite ライブラリ自動取り込み
- セッションタイムライン UI
```

HDR は特に後回しにするべき。
まず SDR / borderless window / HEVC / AAC / TS segment に絞る。

## 29. リスク

### Windows.Graphics.Capture の初回許可

対象ウィンドウ取得にユーザー操作が必要になる場合がある。
MVP では初回セットアップで許容する。

### Game Pass のプロセス識別

Game Pass ゲームは起動コマンドと実体プロセスが一致しない場合がある。
対象ウィンドウから PID を取る方針にする。

### 音声分離の不安定さ

Application Loopback は対象 PID の指定が重要。
ウィンドウ PID が実際の音声レンダリング PID と違う場合、初回に音声対象プロセスを選ばせる必要がある。

### HEVC encoder 差異

NVENC と AMF で設定項目がかなり違う。
抽象化しすぎるより、MVP は backend ごとにプリセットを分ける。

```text
hevc_nvenc_1440p60
hevc_amf_1440p60
```

### TS mux の細部

HEVC in MPEG-TS は可能だが、PMT descriptor、VPS/SPS/PPS、keyframe segment 開始を適切に扱う必要がある。
最初は libavformat を使うべき。

## 30. 実装順序

### Step 1: 保存・DB・ローテーション

```text
- SQLite schema
- segment レコード作成
- ダミーファイル生成
- 容量上限で古い segment 削除
```

### Step 2: ゲーム起動・ウィンドウ検出

```text
- ゲーム登録
- ShellExecuteEx 起動
- EnumWindows
- 対象ウィンドウ選択
- ルール保存
```

### Step 3: Windows.Graphics.Capture

```text
- 対象 hwnd から frame 取得
- D3D11 texture を受ける
- frame rate 調整
```

### Step 4: HEVC encode

```text
- まず NVENC HEVC
- D3D11 texture 入力
- Annex B HEVC bitstream 出力
- keyframe interval 固定
```

### Step 5: AAC encode

```text
- Application Loopback は後回しでもよい
- 先に sine wave で AAC encode
- 次に system loopback
- 最後に application loopback
```

ただし最終 MVP では application loopback 必須。

### Step 6: MPEG-TS mux

```text
- libavformat で HEVC + AAC -> TS
- segment rotate
- keyframe 境界 close
```

### Step 7: 統合

```text
- launcher から recorder 起動
- ゲーム起動
- window detect
- record start
- game exit detect
- stop
- retention
```

## 31. 最小 API

launcher → recorder:

```json
{"type":"prepare","session_id":"...","preset_id":"hevc_1440p60_high","output_dir":"D:\\GameRecordings\\Forza\\..."}
```

```json
{"type":"attach_window","hwnd":123456}
```

```json
{"type":"set_audio_process","pid":9876,"include_tree":true}
```

```json
{"type":"start"}
```

```json
{"type":"stop"}
```

recorder → launcher:

```json
{"type":"status","state":"waiting_for_window"}
```

```json
{"type":"status","state":"capturing","duration_ms":123000,"size_bytes":1048576000}
```

```json
{"type":"segment_closed","path":"...seg_000012.ts","size_bytes":1073741824}
```

```json
{"type":"error","code":"AUDIO_TARGET_NOT_FOUND","message":"..."}
```

## 32. MVP の最終仕様

MVP の完成条件は以下。

```text
- ランチャーにゲームを登録できる
- ランチャーからゲームを起動できる
- 初回に録画対象ウィンドウを選べる
- 次回以降は自動で対象ウィンドウを捕まえる
- 対象ウィンドウだけを録画できる
- 対象プロセス音声だけを録音できる
- HEVC + AAC で .ts segment を生成できる
- 1GB または10分で segment を切れる
- 容量上限を超えたら古い closed segment を削除できる
- protected segment は削除されない
- ゲーム終了時に録画を停止できる
- 異常終了後に .writing segment を検出できる
```

この MVP で、OBS なし・Medal.tv なし・ローカル完結のゲーム録画ランチャーとして成立する。
最大の難所は mux ではなく、**Windows.Graphics.Capture の対象復元** と **Application Loopback の対象 PID 解決**。この2つを初回手動選択で逃がす設計にしておけば、実装リスクはかなり下がる。
