# rsp1-rtltcp

SDRplay **RSP1** の IQ を **RTL-TCP** プロトコルで配信するブリッジ。

SDR# には SDRplay 用の入力が無いため、RSP1 をそのまま使えません。このサーバが
SDRplay API から IQ を取り出し、**CU8（unsigned 8bit I/Q）** に変換して
RTL-TCP サーバとして流します。クライアント側は内蔵の「RTL-SDR (TCP)」ソースで
`127.0.0.1:1234` に繋ぐだけです。

```
RSP1 ──(SDRplay API, 2MHz ゼロIF)──▶ rsp1-rtltcp ──(RTL-TCP / CU8)──▶ SDR# ほか
```

## ビルド

```powershell
cd E:\github\rsp1-fm
cargo build --release -p rsp1-rtltcp
# -> target\release\rsp1-rtltcp.exe
```

> 低整合性ラベルが継承されるフォルダでは `target` に書き込めないことがあります。
> その場合は書き込み可能な場所を `CARGO_TARGET_DIR` に指定してください:
> ```powershell
> $env:CARGO_TARGET_DIR='E:\path\to\rtltcp-target'
> cargo build --release -p rsp1-rtltcp
> # -> E:\path\to\rtltcp-target\release\rsp1-rtltcp.exe
> ```

## 実行

```powershell
# いちばん簡単（低整合性ラベルの影響を避けてコピー実行。後述の「低整合性」を参照）
rtltcp\tools\run-rtltcp.cmd --freq 80.0

# 既定 (127.0.0.1:1234, 100 MHz, 2048000 Hz, AGC)
.\target\release\rsp1-rtltcp.exe

# 例: JR-D 下り帯を 2.048MHz で
.\target\release\rsp1-rtltcp.exe --freq 352.603125 --rate 2048000

# デバイス一覧だけ
.\target\release\rsp1-rtltcp.exe --list
```

SDR# 側:

- Source = **RTL-SDR (TCP)**
- Host = `127.0.0.1` / Port = `1234`
- SDR# が接続時に周波数・サンプルレート・ゲインを送ってくるので、それに追従します。

## オプション

| オプション | 既定 | 説明 |
|---|---|---|
| `--port <番号>` | 1234 | 待ち受けポート |
| `--bind <アドレス>` | 127.0.0.1 | 待ち受けアドレス |
| `--freq <Hz\|MHz>` | 100 MHz | 初期中心周波数（小数点付きなら MHz） |
| `--rate <Hz>` | 2048000 | 初期サンプルレート |
| `--gain <dB>` | 30 | マニュアル時のゲイン |
| `--no-agc` | | 最初からマニュアルゲイン |
| `--no-digital-agc` | | 8bit 化前のデジタル AGC を切る |
| `--release-on-idle` | | クライアント切断時に RSP1 を解放（SDRuno と交互に使う場合） |
| `--list` | | 接続中の RSP を一覧表示して終了 |

## 重要: 低整合性 (Low Integrity) で起動すると必ず失敗します

SDRplay API サービスは **中整合性 (Medium Integrity)** の共有メモリ
`Global\sdrSrvComShMem` でクライアントと通信します。Windows の整合性ラベル規則により、
**低整合性のプロセスはこの共有メモリを開けません**（`OpenFileMapping` が
`ERROR_ACCESS_DENIED`）。このとき SDRplay API は次を返します:

```
sdrplay_api.cpp / sdrplay_api_Open:151 Could not open file mapping object
```

さらに **exe に `Mandatory Label\Low Mandatory Level` が付いていると、起動元が
中整合性でもプロセスは低整合性になります**。サンドボックス環境（DSH など）は、生成した
ファイルに低整合性ラベルを継承させることがあります。確認:

```powershell
icacls <exe>     # "Mandatory Label\Low Mandatory Level" が出たら該当
```

このツールは起動時に整合性を判定し、低整合性なら理由と対処を表示します:

```
[警告] このプロセスは低整合性 (Low Integrity, RID 0x1000) で動作しています。
```

対処（いずれか）:

1. **通常のターミナルから実行する** — サンドボックス外でビルド/実行すればラベルは付かない
2. **exe を別フォルダへコピーしてから実行する** — `tools\run-rtltcp.cmd` が
   `%LOCALAPPDATA%\rsp1-rtltcp\` へコピーして起動します
3. 管理者権限で `icacls "<exe>" /setintegritylevel Medium`

> フォルダに `Low Mandatory Level:(OI)(CI)` が継承設定されていると、その中でビルドした
> exe には低整合性ラベルが付きます。上記 1〜3 のいずれかでラベルを外してください。

## 設計メモ

- デバイスは常に **2.0 MHz / IF 1.536 MHz** で開き、間引きはしません。
  クライアントが要求したレートへは**間引き（移動平均）＋線形補間**で合わせるため、
  レート変更でストリームを張り直す必要がありません。
- 8bit を有効に使うため、既定で**ゆっくりしたデジタル AGC** を通します
  （RMS 目標＋ピーク上限でクリップも回避）。
- 最初の接続で RSP1 を開き、その後は開いたままにします（SDR# の接続/切断が即反映）。
  未接続の間は IQ を捨てるだけです。`--release-on-idle` で解放に切り替え。
- SDRplay API サービスは、直前まで使っていたプロセスの解放に数十秒かかることが
  あります。その間 `Open` が失敗しても **1 秒間隔で自動再試行**します。

## 動作確認

同梱の確認クライアントで、SDR# 無しでも検証できます。

```powershell
python rtltcp\tools\rtltcp_check.py --freq 80.0 --rate 2048000 --seconds 2 --agc
```

ヘッダ（`RTL0`, tuner, ゲイン表）・コマンド・IQ の流量とスペクトルを表示します。

実測例（RSP1 互換機 / 80 MHz）: `RTL0`, tuner=5, gain_count=29、
2048000 Hz で定常受信、ノイズフロア約 -97 dB、信号ピーク約 -61 dB。

診断ツール:

| ツール | 用途 |
|---|---|
| `tools\run-rtltcp.cmd` | 低整合性ラベルを避けて exe をコピー実行するランチャー |
| `tools\sdrplay_api_probe.py` | API のバージョンと Open の成否・GetLastError を表示 |
| `tools\sdrplay_deep_probe.py` | プロセス整合性と共有メモリのアクセス権を表示 |
| `core\examples\api_probe.rs` | Rust 側から Open と GetLastError を表示 |
| `core\examples\raw_load_probe.rs` | LoadLibraryW 直呼びでトークンとマッピングを確認 |
