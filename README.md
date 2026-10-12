# rsp1-fm

SDRplay **RSP1** の IQ を **RTL-TCP** プロトコルで配信するブリッジ `rsp1-rtltcp`。
SDR# など RTL-SDR (TCP) 入力しか持たないアプリから、RSP1 をリアルタイム受信できます。

```
RSP1 (USB) ──SDRplay API──▶ rsp1-rtltcp ──RTL-TCP (CU8)──▶ SDR# / RTL-TCP クライアント
             2.0 MHz IQ                     127.0.0.1:1234
```

## 構成

```
rsp1-fm/
├─ Cargo.toml            … cargo ワークスペース
├─ core/                 … rsp1fm-core (SDRplay API 3.x への FFI)
│   ├─ src/sdrplay.rs    … 構造体・関数バインディング
│   └─ examples/         … api_probe / raw_load_probe (実機・診断用)
└─ rtltcp/               … rsp1-rtltcp (RTL-TCP ブリッジ本体)
    ├─ src/main.rs       … RTL-TCP サーバ + レート変換 + デジタル AGC
    ├─ tools/            … 起動ランチャ・動作確認・診断スクリプト
    └─ README.md         … 詳細な説明
```

## ビルドと実行

Windows / Linux 共通:

```bash
cd rsp1-fm
cargo build --release -p rsp1-rtltcp
./target/release/rsp1-rtltcp --freq 80.0     # Linux
# .\target\release\rsp1-rtltcp.exe --freq 80.0   # Windows
```

SDR# 側は Source を **RTL-SDR (TCP)**、Host=`127.0.0.1`、Port=`1234` にします。
詳しいオプション・設計・OS 別の注意は [rtltcp/README.md](rtltcp/README.md) を参照してください。

## 必要なもの

- Windows（x64）または Linux（Ubuntu など、x86_64）
- **SDRplay API 3.x** インストール済み + API サービスが実行中
  - Windows: `C:\Program Files\SDRplay\API\x64\sdrplay_api.dll` / サービス `SDRplayAPIService`
  - Linux: `/usr/local/lib/libsdrplay_api.so` / サービス `sdrplay`（`sudo systemctl start sdrplay`）
  - 別の場所なら環境変数 `SDRPLAY_API_LIB`（旧 `SDRPLAY_API_DLL` も可）で指定
- SDRplay **RSP1**（RSP1A/RSP2/RSPduo/RSPdx でも概ね動作）
- Rust ツールチェーン（stable）
- Linux: SDRplay API インストーラが入れる udev ルールで RSP1 にアクセスできること
