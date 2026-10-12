//! RSP1 (SDRplay API 3.x) の IQ を RTL-TCP プロトコルで配信するブリッジ。
//!
//! SDR# (SDR# Studio) には SDRplay 用のフロントエンドが無いため、RSP1 をそのまま
//! 使うことはできない。このプログラムは SDRplay API から IQ を読み出し、
//! CU8 (unsigned 8bit I/Q) に変換して RTL-TCP サーバとして配信する。
//! SDR# 側は内蔵の「RTL-SDR (TCP)」ソースで 127.0.0.1:1234 に繋ぐだけでよい。
//!
//! 使い方:
//!   rsp1-rtltcp.exe --freq 352603125 --rate 2048000 --gain 30
//!   (SDR# > Source: RTL-SDR (TCP), Host: 127.0.0.1, Port: 1234)
//!
//! 設計:
//! - デバイスは常に 2.0 MHz / IF 1.536 MHz で開く (SDRplay の間引きは使わない)。
//!   クライアントが要求したレートへは、間引き (移動平均) + 線形補間で合わせるので、
//!   レート変更でストリームを張り直す必要がない。
//! - 8bit を有効に使うため、既定でゆっくりしたデジタル AGC を通す。
//! - 最初の接続で RSP1 を開き、その後は開いたままにする (SDR# の接続/切断がすぐ効く)。
//!   未接続の間は IQ を捨てるだけ。SDRuno と交互に使う場合は --release-on-idle。
//! - SDRplay API サービスは、直前まで使っていたプロセスの解放に数十秒かかることがあり、
//!   その間は Open が失敗する。開けなかった場合は 1 秒間隔で自動再試行する。

use rsp1fm_core::sdrplay::{
    Api, CallbackFnsT, DeviceParamsT, DeviceT, StreamCbParamsT, AGC_50HZ, AGC_DISABLE, API_SUCCESS,
    BW_1_536, IF_ZERO, SDRPLAY_MAX_DEVICES, TUNER_A, UPDATE_CTRL_AGC, UPDATE_DEV_PPM,
    UPDATE_NONE, UPDATE_TUNER_FRF, UPDATE_TUNER_GR,
};
use std::ffi::c_void;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const USAGE: &str = "\
rsp1-rtltcp - RSP1 (SDRplay) の IQ を RTL-TCP で配信し、SDR# の「RTL-SDR (TCP)」で受信させる

使い方:
  rsp1-rtltcp.exe [オプション]

オプション:
  --port <番号>     待ち受けポート (既定 1234)
  --bind <アドレス> 待ち受けアドレス (既定 127.0.0.1)
  --freq <Hz|MHz>   初期の中心周波数。小数点付きなら MHz 扱い (既定 100 MHz)
  --rate <Hz>       初期のサンプルレート (既定 2048000)
  --gain <dB>       マニュアル時のゲイン (既定 30)
  --no-agc          最初からマニュアルゲインで開始 (既定は AGC)
  --no-digital-agc  8bit 化のデジタル AGC を切る (既定は有効)
  --release-on-idle クライアントが切れたら RSP1 を解放する
                    (既定は解放しない = SDR# の接続/切断がすぐ効く。SDRuno と
                     交互に使う場合はこのオプションを付ける)
  --list            接続中の RSP を一覧表示して終了
  -h, --help        このヘルプ

SDR# 側:
  Source = RTL-SDR (TCP) / Host = 127.0.0.1 / Port = <--port の値>
  SDR# が接続時に周波数・サンプルレート・ゲインを送ってくるので、それに追従する。
";

// RSP1 の実サンプルレート (これで開き、要求レートへはデジタルで変換する)
const DEV_RATE: f64 = 2_000_000.0;
const DEV_BW: i32 = BW_1_536;

// ---- RTL-TCP プロトコル ----
const RTLTCP_MAGIC: &[u8; 4] = b"RTL0";
/// SDR# にゲインつまみを出させるためのチューナ種別 (5 = R820T)。
const TUNER_TYPE_R820T: u32 = 5;
/// R820T のゲイン表 (絶対ゲイン dB)。index 指定 (0x0d) のときに使う。
const GAIN_TABLE_DB: [f32; 29] = [
    0.0, 0.9, 1.4, 2.7, 3.7, 7.7, 8.7, 12.5, 14.4, 15.7, 16.6, 19.7, 20.7, 22.9, 25.4, 28.0, 29.7,
    32.8, 33.8, 36.4, 37.2, 38.6, 40.2, 42.1, 43.4, 43.9, 44.5, 45.0, 45.6,
];

// ---- 設定 ----

struct Config {
    port: u16,
    bind: String,
    freq: f64,
    rate: f64,
    gain_db: f32,
    agc: bool,
    digital_agc: bool,
    release_on_idle: bool,
    list: bool,
}

impl Config {
    fn parse(args: impl Iterator<Item = String>) -> Result<Config, String> {
        let mut cfg = Config {
            port: 1234,
            bind: "127.0.0.1".into(),
            freq: 100_000_000.0,
            rate: 2_048_000.0,
            gain_db: 30.0,
            agc: true,
            digital_agc: true,
            release_on_idle: false,
            list: false,
        };
        let mut it = args;
        while let Some(a) = it.next() {
            let mut val = |name: &str| -> Result<String, String> {
                it.next().ok_or_else(|| format!("{name} の値がありません"))
            };
            match a.as_str() {
                "--port" => cfg.port = val("--port")?.parse().map_err(|e| format!("--port: {e}"))?,
                "--bind" => cfg.bind = val("--bind")?,
                "--freq" => cfg.freq = parse_freq(&val("--freq")?)?,
                "--rate" => {
                    cfg.rate = val("--rate")?.parse().map_err(|e| format!("--rate: {e}"))?
                }
                "--gain" => {
                    cfg.gain_db = val("--gain")?.parse().map_err(|e| format!("--gain: {e}"))?
                }
                "--no-agc" => cfg.agc = false,
                "--agc" => cfg.agc = true,
                "--no-digital-agc" => cfg.digital_agc = false,
                "--release-on-idle" => cfg.release_on_idle = true,
                "--list" => cfg.list = true,
                "-h" | "--help" => {
                    print!("{USAGE}");
                    std::process::exit(0);
                }
                other => return Err(format!("不明なオプション: {other}")),
            }
        }
        Ok(cfg)
    }
}

fn parse_freq(s: &str) -> Result<f64, String> {
    if s.contains('.') {
        let mhz: f64 = s.parse().map_err(|e| format!("--freq: {e}"))?;
        Ok(mhz * 1e6)
    } else {
        s.parse().map_err(|e| format!("--freq: {e}"))
    }
}

fn normalize_rate(req: f64) -> f64 {
    if req.is_finite() && (200_000.0..=10_000_000.0).contains(&req) {
        req
    } else {
        2_048_000.0
    }
}

// ---- レート変換 (間引き + 線形補間) ----

struct Resampler {
    /// 間引き段数 (移動平均の長さ)
    decim: usize,
    acc_i: f32,
    acc_q: f32,
    acc_n: usize,
    /// 間引き後のサンプル 1 個あたりの出力サンプル数
    step: f64,
    pos: f64,
    has_prev: bool,
    pi: f32,
    pq: f32,
}

impl Resampler {
    fn new(out_rate: f64) -> Self {
        let mut r = Resampler {
            decim: 1,
            acc_i: 0.0,
            acc_q: 0.0,
            acc_n: 0,
            step: 1.0,
            pos: 0.0,
            has_prev: false,
            pi: 0.0,
            pq: 0.0,
        };
        r.configure(out_rate);
        r
    }

    fn configure(&mut self, out_rate: f64) {
        let out = normalize_rate(out_rate);
        self.decim = if out > 1_000_000.0 {
            1
        } else if out > 500_000.0 {
            2
        } else if out > 250_000.0 {
            4
        } else {
            8
        };
        let in_rate = DEV_RATE / self.decim as f64;
        self.step = (in_rate / out).clamp(0.2, 4.0);
    }

    fn reset_state(&mut self) {
        self.acc_i = 0.0;
        self.acc_q = 0.0;
        self.acc_n = 0;
        self.pos = 0.0;
        self.has_prev = false;
    }

    /// i16 の I/Q を間引き + 補間して out (i16 相当の f32, I/Q 交互) へ書き出す。
    fn process(&mut self, i: &[i16], q: &[i16], out: &mut Vec<f32>) {
        for k in 0..i.len().min(q.len()) {
            self.acc_i += i[k] as f32;
            self.acc_q += q[k] as f32;
            self.acc_n += 1;
            if self.acc_n < self.decim {
                continue;
            }
            let ci = self.acc_i / self.decim as f32;
            let cq = self.acc_q / self.decim as f32;
            self.acc_i = 0.0;
            self.acc_q = 0.0;
            self.acc_n = 0;

            if !self.has_prev {
                self.pi = ci;
                self.pq = cq;
                self.has_prev = true;
                self.pos = 0.0;
                continue;
            }
            while self.pos < 1.0 {
                let t = self.pos.clamp(0.0, 1.0) as f32;
                out.push(self.pi + (ci - self.pi) * t);
                out.push(self.pq + (cq - self.pq) * t);
                self.pos += self.step;
            }
            self.pos -= 1.0;
            self.pi = ci;
            self.pq = cq;
        }
    }
}

/// 8bit 化の前にレベルを整える、ゆっくりしたデジタル AGC。
/// (RTL-SDR のアナログ AGC 相当。クリップも防ぐ)
struct DigitalAgc {
    gain: f32,
    updates: u32,
}

impl DigitalAgc {
    /// 目標 RMS (i16 のスケールで約 -18 dBFS)
    const TARGET_RMS: f32 = 4000.0;
    /// 瞬間ピークの上限 (i16 のスケールで約 -2.7 dBFS)
    const PEAK_LIMIT: f32 = 24000.0;
    const GAIN_MIN: f32 = 0.1;
    const GAIN_MAX: f32 = 4096.0;

    fn new() -> Self {
        DigitalAgc { gain: 1.0, updates: 0 }
    }

    fn update(&mut self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        let mut sum = 0.0f64;
        let mut peak = 0.0f32;
        for &v in samples {
            sum += (v as f64) * (v as f64);
            let a = v.abs();
            if a > peak {
                peak = a;
            }
        }
        let rms = (sum / samples.len() as f64).sqrt() as f32;
        // RMS 目標とピーク上限の両方を満たす絶対ゲイン
        let want = (Self::TARGET_RMS / rms.max(1.0))
            .min(Self::PEAK_LIMIT / peak.max(1.0))
            .clamp(Self::GAIN_MIN, Self::GAIN_MAX);
        // 最初だけ速く寄せ、その後はゆっくり (ポンピングを避ける)
        let alpha = if self.updates < 400 { 0.005 } else { 0.001 };
        self.updates = self.updates.saturating_add(1);
        self.gain = (self.gain + (want - self.gain) * alpha).clamp(Self::GAIN_MIN, Self::GAIN_MAX);

        // 瞬間的なクリップだけは即座に避ける (ファストアタック / 遅いリリース)
        let hard = 28000.0 / peak.max(1.0);
        if self.gain > hard {
            self.gain = hard.max(Self::GAIN_MIN);
        }
    }
}

fn to_u8(v: f32) -> u8 {
    ((v / 256.0) + 128.0).round().clamp(0.0, 255.0) as u8
}

/// RTL-TCP のゲイン (絶対 dB) を SDRplay のゲイン低減量 (20=最大 .. 59=最小) に写す。
fn gain_db_to_gr(gain_db: f32) -> i32 {
    (59.0 - gain_db * 0.855).round().clamp(20.0, 59.0) as i32
}

// ---- ストリームの受け皿 ----

struct Sink {
    tx: Sender<Vec<u8>>,
    pending: Arc<AtomicUsize>,
    limit: usize,
    rs: Resampler,
    agc: Option<DigitalAgc>,
    scratch: Vec<f32>,
}

impl Sink {
    fn new(
        tx: Sender<Vec<u8>>,
        pending: Arc<AtomicUsize>,
        out_rate: f64,
        digital_agc: bool,
    ) -> Sink {
        Sink {
            tx,
            pending,
            limit: (out_rate as usize / 2).max(64 * 1024),
            rs: Resampler::new(out_rate),
            agc: if digital_agc { Some(DigitalAgc::new()) } else { None },
            scratch: Vec::new(),
        }
    }

    fn set_rate(&mut self, out_rate: f64) {
        self.rs.configure(out_rate);
        self.rs.reset_state();
        self.limit = (out_rate as usize / 2).max(64 * 1024);
    }

    /// コールバックから呼ばれる。CU8 のパケットを作って送る。
    fn process(&mut self, i: &[i16], q: &[i16]) {
        // クライアントが遅いときは古いデータを捨てる (遅延を溜めない)
        if self.pending.load(Ordering::Relaxed) > self.limit {
            return;
        }
        self.scratch.clear();
        self.rs.process(i, q, &mut self.scratch);
        if self.scratch.is_empty() {
            return;
        }
        let gain = match self.agc.as_mut() {
            Some(a) => {
                a.update(&self.scratch);
                a.gain
            }
            None => 1.0,
        };
        let mut buf: Vec<u8> = Vec::with_capacity(self.scratch.len());
        for &v in &self.scratch {
            buf.push(to_u8(v * gain));
        }
        let len = buf.len();
        if self.tx.send(buf).is_ok() {
            self.pending.fetch_add(len, Ordering::Relaxed);
        }
    }
}

struct StreamState {
    sink: Mutex<Option<Sink>>,
}

unsafe extern "C" fn stream_cb(
    xi: *mut i16,
    xq: *mut i16,
    _p: *mut StreamCbParamsT,
    num: u32,
    _reset: u32,
    ctx: *mut c_void,
) {
    if ctx.is_null() || xi.is_null() || xq.is_null() {
        return;
    }
    let state = &*(ctx as *const StreamState);
    let n = num as usize;
    let i = std::slice::from_raw_parts(xi, n);
    let q = std::slice::from_raw_parts(xq, n);

    let mut guard = match state.sink.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if let Some(sink) = guard.as_mut() {
        sink.process(i, q);
    }
}

unsafe extern "C" fn event_cb(_event: i32, _tuner: i32, _params: *mut c_void, _ctx: *mut c_void) {}

// ---- ラジオ側 (SDRplay) ----

#[derive(Clone, Copy)]
struct Desired {
    out_rate: f64,
    freq: f64,
    gain_db: f32,
    agc: bool,
    ppm: f64,
}

struct Running {
    api: Api,
    dev: DeviceT,
    params: *mut DeviceParamsT,
    ctx_ptr: *mut c_void,
}

enum Cmd {
    Apply(Desired),
    /// クライアントが切れた (release_on_idle のときだけ実際に解放する)
    Close,
    /// プロセス終了 (SDRplay をきちんと閉じてから終わる)
    Quit,
}

struct Radio {
    state: Arc<StreamState>,
    running: Option<Running>,
    applied: Option<Desired>,
    /// 最後に要求された状態 (未接続でも保持し、開け次第それで開始する)
    desired: Option<Desired>,
    release_on_idle: bool,
    fail_streak: u32,
    last_fail_log: Option<std::time::Instant>,
}

impl Radio {
    fn new(state: Arc<StreamState>, release_on_idle: bool) -> Self {
        Radio {
            state,
            running: None,
            applied: None,
            desired: None,
            release_on_idle,
            fail_streak: 0,
            last_fail_log: None,
        }
    }

    /// クライアントからの希望状態を受ける。
    fn on_apply(&mut self, d: Desired) {
        self.desired = Some(d);
        self.pump();
    }

    /// クライアントが切れた。
    fn on_close(&mut self) {
        if self.release_on_idle {
            self.stop();
        }
        // 既定では開いたままにする (SDR# の接続/切断をすぐ反映させるため)
    }

    /// 希望状態へ寄せる。開けていないときは次回以降も再試行する。
    /// デバイスは常に 2.0 MHz 固定なので、レート変更で張り直す必要はない。
    fn pump(&mut self) {
        let d = match self.desired {
            Some(d) => d,
            None => return,
        };
        if self.running.is_none() {
            match self.start(d) {
                Ok(()) => {
                    self.applied = Some(d);
                    self.fail_streak = 0;
                    println!(
                        "[rsp1] 受信開始: {:.6} MHz / 出力 {} Hz / {} / {:.1} dB (gRdB {})",
                        d.freq / 1e6,
                        d.out_rate,
                        if d.agc { "AGC" } else { "マニュアル" },
                        d.gain_db,
                        gain_db_to_gr(d.gain_db)
                    );
                }
                Err(e) => {
                    self.applied = None;
                    self.fail_streak = self.fail_streak.saturating_add(1);
                    let now = std::time::Instant::now();
                    let due = match self.last_fail_log {
                        None => true,
                        Some(t) => now.duration_since(t).as_secs() >= 10,
                    };
                    if due {
                        self.last_fail_log = Some(now);
                        eprintln!(
                            "[rsp1] 受信を開始できません ({} 回目): {e}",
                            self.fail_streak
                        );
                        eprintln!(
                            "[rsp1] SDRplay API サービスは、直前まで使っていたクライアントの解放に\
                             時間がかかることがあります (数十秒〜1分)。自動で再試行します。\
                             改善しない場合は C:\\Program Files\\SDRplay\\RestartService.bat で\
                             サービスを再起動してください。"
                        );
                    }
                }
            }
            return;
        }
        let a = match self.applied {
            Some(a) => a,
            None => return,
        };
        if a.freq != d.freq {
            self.update_freq(d.freq);
            println!("[rsp1] 中心周波数 -> {:.6} MHz", d.freq / 1e6);
        }
        if a.ppm != d.ppm {
            self.update_ppm(d.ppm);
        }
        if a.gain_db != d.gain_db || a.agc != d.agc {
            self.update_gain(d.gain_db, d.agc);
        }
        self.applied = Some(d);
    }

    fn start(&mut self, d: Desired) -> Result<(), String> {
        let api = Api::load()?;
        // 直前までデバイスを使っていたプロセスの解放が終わっていないと Open が失敗する。
        // 呼び出し側 (radio_thread) が数秒間隔で再試行するので、ここでは 1 回だけ試す。
        let r = unsafe { (api.open)() };
        if r != API_SUCCESS {
            return Err(format!("sdrplay_api_Open: {}", api.err_str(r)));
        }
        match open_stream(&api, d, &self.state) {
            Ok((dev, params, ctx_ptr)) => {
                self.running = Some(Running { api, dev, params, ctx_ptr });
                Ok(())
            }
            Err(e) => {
                unsafe {
                    let _ = (api.close)();
                }
                Err(e)
            }
        }
    }

    fn stop(&mut self) {
        if let Some(r) = self.running.take() {
            unsafe {
                let _ = (r.api.uninit)(r.dev.dev);
                let _ = Arc::from_raw(r.ctx_ptr as *const StreamState);
                let mut dev = r.dev;
                let _ = (r.api.release_device)(&mut dev);
                let _ = (r.api.close)();
            }
            self.applied = None;
        }
    }

    fn update_freq(&mut self, f: f64) {
        if let Some(r) = self.running.as_mut() {
            let p = r.params;
            let dev = r.dev.dev;
            let api = &r.api;
            unsafe {
                if let Some(dp) = p.as_mut() {
                    if !dp.rx_channel_a.is_null() {
                        (*(*dp).rx_channel_a).tuner_params.rf_freq.rf_hz = f;
                    }
                }
                let _ = (api.update)(dev, TUNER_A, UPDATE_TUNER_FRF, UPDATE_NONE);
            }
        }
    }

    fn update_ppm(&mut self, ppm: f64) {
        if let Some(r) = self.running.as_mut() {
            let p = r.params;
            let dev = r.dev.dev;
            let api = &r.api;
            unsafe {
                if let Some(dp) = p.as_mut() {
                    if !dp.dev_params.is_null() {
                        (*(*dp).dev_params).ppm = ppm;
                    }
                }
                let _ = (api.update)(dev, TUNER_A, UPDATE_DEV_PPM, UPDATE_NONE);
            }
        }
    }

    fn update_gain(&mut self, gain_db: f32, agc: bool) {
        if let Some(r) = self.running.as_mut() {
            let p = r.params;
            let dev = r.dev.dev;
            let api = &r.api;
            unsafe {
                if let Some(dp) = p.as_mut() {
                    if !dp.rx_channel_a.is_null() {
                        let rxa = &mut *dp.rx_channel_a;
                        if agc {
                            rxa.ctrl_params.agc.enable = AGC_50HZ;
                        } else {
                            rxa.ctrl_params.agc.enable = AGC_DISABLE;
                            rxa.tuner_params.gain.g_rdb = gain_db_to_gr(gain_db);
                        }
                    }
                }
                let _ = (api.update)(dev, TUNER_A, UPDATE_CTRL_AGC, UPDATE_NONE);
                let _ = (api.update)(dev, TUNER_A, UPDATE_TUNER_GR, UPDATE_NONE);
            }
        }
    }
}

fn open_stream(
    api: &Api,
    d: Desired,
    state: &Arc<StreamState>,
) -> Result<(DeviceT, *mut DeviceParamsT, *mut c_void), String> {
    unsafe {
        let _ = (api.lock_device_api)();
        let mut devs = [DeviceT::zero(); SDRPLAY_MAX_DEVICES];
        let mut n = 0u32;
        let gd = (api.get_devices)(devs.as_mut_ptr(), &mut n, SDRPLAY_MAX_DEVICES as u32);
        if gd != API_SUCCESS {
            let _ = (api.unlock_device_api)();
            return Err(format!("sdrplay_api_GetDevices に失敗: {}", api.err_str(gd)));
        }
        if n == 0 {
            let _ = (api.unlock_device_api)();
            return Err("RSP が見つかりません (USB 接続と SDRplay API サービスを確認)".into());
        }
        let mut dev = devs[0];
        let sel = (api.select_device)(&mut dev);
        let _ = (api.unlock_device_api)();
        if sel != API_SUCCESS {
            return Err(format!("sdrplay_api_SelectDevice に失敗: {}", api.err_str(sel)));
        }

        let mut params: *mut DeviceParamsT = std::ptr::null_mut();
        let gp = (api.get_device_params)(dev.dev, &mut params);
        if gp != API_SUCCESS || params.is_null() {
            let mut dev2 = dev;
            let _ = (api.release_device)(&mut dev2);
            return Err("sdrplay_api_GetDeviceParams に失敗".into());
        }

        {
            let dp = &mut *params;
            if dp.dev_params.is_null() || dp.rx_channel_a.is_null() {
                let mut dev2 = dev;
                let _ = (api.release_device)(&mut dev2);
                return Err("デバイスパラメータが null です".into());
            }
            // 常に 2.0 MHz / IF 1.536 MHz で開く (間引きは使わない)
            (*dp.dev_params).fs_freq.fs_hz = DEV_RATE;
            (*dp.dev_params).ppm = d.ppm;

            let rxa = &mut *dp.rx_channel_a;
            rxa.tuner_params.bw_type = DEV_BW;
            rxa.tuner_params.if_type = IF_ZERO;
            rxa.tuner_params.rf_freq.rf_hz = d.freq;
            rxa.tuner_params.gain.g_rdb = gain_db_to_gr(d.gain_db);
            rxa.tuner_params.gain.lna_state = 0;
            rxa.tuner_params.gain.min_gr = 20;
            rxa.ctrl_params.dc_offset.dc_enable = 1;
            rxa.ctrl_params.dc_offset.iq_enable = 1;
            rxa.ctrl_params.decimation.enable = 0;
            rxa.ctrl_params.decimation.decimation_factor = 1;
            rxa.ctrl_params.decimation.wide_band_signal = 0;
            rxa.ctrl_params.agc.enable = if d.agc { AGC_50HZ } else { AGC_DISABLE };
        }

        let ctx_ptr = Arc::into_raw(state.clone()) as *mut c_void;
        let mut cbs = CallbackFnsT { stream_a: Some(stream_cb), stream_b: None, event: Some(event_cb) };
        let ir = (api.init)(dev.dev, &mut cbs, ctx_ptr);
        if ir != API_SUCCESS {
            let _ = Arc::from_raw(ctx_ptr as *const StreamState);
            let mut dev2 = dev;
            let _ = (api.release_device)(&mut dev2);
            return Err(format!("ストリーム開始に失敗: {}", api.err_str(ir)));
        }
        Ok((dev, params, ctx_ptr))
    }
}

/// Ctrl+C / ウィンドウを閉じたときに、SDRplay をきちんと閉じてから終了するための送り先。
static CMD_TX: Mutex<Option<Sender<Cmd>>> = Mutex::new(None);

#[link(name = "kernel32")]
extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}

/// CTRL_C_EVENT(0) / CTRL_CLOSE_EVENT(2) でラジオ側に後始末をさせてから終了する。
// ---- 低整合性 (Low Integrity) の検出 ----
//
// サンドボックス環境 (DSH など) が exe に "Mandatory Label\Low" を付けると、
// その exe から起動したプロセスは低整合性 (Low Integrity) になる。
// SDRplay API サービスは Global\sdrSrvComShMem (中整合性) の共有メモリを開くため、
// 低整合性では必ず sdrplay_api_Open が "Could not open file mapping object" で失敗する。
const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_INTEGRITY_LEVEL: u32 = 25;
const SECURITY_MANDATORY_MEDIUM_RID: u32 = 0x2000;

#[repr(C)]
struct SidAndAttributes {
    sid: *mut c_void,
    attributes: u32,
}

#[repr(C)]
struct TokenMandatoryLabel {
    label: SidAndAttributes,
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(h: *mut c_void, access: u32, tok: *mut *mut c_void) -> i32;
    fn GetTokenInformation(
        tok: *mut c_void,
        class: u32,
        buf: *mut c_void,
        len: u32,
        ret: *mut u32,
    ) -> i32;
    fn GetSidSubAuthority(sid: *mut c_void, index: u32) -> *mut u32;
    fn GetSidSubAuthorityCount(sid: *mut c_void) -> *mut u8;
}

/// 現在のプロセスの整合性レベル (RID)。取得できなければ None。
fn integrity_rid() -> Option<u32> {
    unsafe {
        let mut tok: *mut c_void = std::ptr::null_mut();
        // GetCurrentProcess() は常に疑似ハンドル (-1)。
        if OpenProcessToken(-1isize as *mut c_void, TOKEN_QUERY, &mut tok) == 0 {
            return None;
        }
        let mut size = 0u32;
        GetTokenInformation(tok, TOKEN_INTEGRITY_LEVEL, std::ptr::null_mut(), 0, &mut size);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        if GetTokenInformation(
            tok,
            TOKEN_INTEGRITY_LEVEL,
            buf.as_mut_ptr() as *mut c_void,
            size,
            &mut size,
        ) == 0
        {
            return None;
        }
        let tml = &*(buf.as_ptr() as *const TokenMandatoryLabel);
        let cnt = *GetSidSubAuthorityCount(tml.label.sid);
        if cnt == 0 {
            return None;
        }
        Some(*GetSidSubAuthority(tml.label.sid, (cnt as u32) - 1))
    }
}

/// 低整合性で起動していると SDRplay API は絶対に開けないので、先に警告する。
fn warn_if_low_integrity() {
    if let Some(rid) = integrity_rid() {
        if rid < SECURITY_MANDATORY_MEDIUM_RID {
            eprintln!(
                "[警告] このプロセスは低整合性 (Low Integrity, RID {rid:#x}) で動作しています。"
            );
            eprintln!(
                "       SDRplay API は中整合性の共有メモリ (Global\\sdrSrvComShMem) を使うため、"
            );
            eprintln!("       この状態では sdrplay_api_Open が必ず失敗します。対処:");
            eprintln!("        1) 通常のターミナル (サンドボックス外) から実行する");
            eprintln!("        2) exe を別フォルダへコピーしてから実行する (tools\\run-rtltcp.cmd)");
            eprintln!("        3) 管理者権限で: icacls \"<exe>\" /setintegritylevel Medium");
        }
    }
}

unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> i32 {
    if ctrl_type == 0 || ctrl_type == 2 {
        println!("\n[main] 終了します。RSP1 を解放しています...");
        if let Ok(g) = CMD_TX.lock() {
            if let Some(tx) = g.as_ref() {
                let _ = tx.send(Cmd::Quit);
            }
        }
        // ラジオスレッドが uninit / release / close を終えるのを待つ
        std::thread::sleep(std::time::Duration::from_millis(900));
        std::process::exit(0);
    }
    0
}

fn radio_thread(rx: Receiver<Cmd>, state: Arc<StreamState>, release_on_idle: bool) {
    let mut radio = Radio::new(state, release_on_idle);
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(5000)) {
            Ok(Cmd::Apply(d)) => radio.on_apply(d),
            Ok(Cmd::Close) => radio.on_close(),
            Ok(Cmd::Quit) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // まだ開けていないときは定期的に再試行する
                // (サービスが前のクライアントを解放するまで数秒〜数十秒かかることがある)
                if radio.running.is_none() {
                    radio.pump();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    radio.stop();
}

// ---- デバイス一覧 ----

fn list_devices() {
    match integrity_rid() {
        Some(rid) => println!("プロセス整合性: RID {rid:#x}{}", if rid < SECURITY_MANDATORY_MEDIUM_RID { " (低整合性 → SDRplay API は開けません)" } else { "" }),
        None => println!("プロセス整合性: 取得できません"),
    }
    let api = match Api::load() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    unsafe {
        let r = (api.open)();
        if r != API_SUCCESS {
            eprintln!("sdrplay_api_Open に失敗: {}", api.err_str(r));
            return;
        }
        let _ = (api.lock_device_api)();
        let mut devs = [DeviceT::zero(); SDRPLAY_MAX_DEVICES];
        let mut n = 0u32;
        let gd = (api.get_devices)(devs.as_mut_ptr(), &mut n, SDRPLAY_MAX_DEVICES as u32);
        let _ = (api.unlock_device_api)();
        if gd != API_SUCCESS {
            eprintln!("sdrplay_api_GetDevices に失敗: {}", api.err_str(gd));
        } else {
            println!("検出デバイス数: {n}");
            for i in 0..n as usize {
                println!(
                    "  [{i}] {} SN={} valid={} hw_ver={} tuner={}",
                    devs[i].model(),
                    devs[i].serial(),
                    devs[i].valid,
                    devs[i].hw_ver,
                    devs[i].tuner
                );
            }
        }
        let _ = (api.close)();
    }
}

// ---- RTL-TCP サーバ ----

/// RTL-TCP の 1 コマンドを desired に反映する。状態が変わったら true。
fn apply_command(
    d: &mut Desired,
    code: u8,
    v: u32,
    state: &Arc<StreamState>,
    unknown: &mut usize,
) -> bool {
    match code {
        0x01 => {
            d.freq = v as f64;
            println!("[cmd] 中心周波数 {:.6} MHz", d.freq / 1e6);
            true
        }
        0x02 => {
            d.out_rate = normalize_rate(v as f64);
            if let Ok(mut g) = state.sink.lock() {
                if let Some(s) = g.as_mut() {
                    s.set_rate(d.out_rate);
                }
            }
            println!("[cmd] サンプルレート {} Hz -> 出力 {} Hz", v, d.out_rate);
            false // デバイス側は 2.0 MHz 固定なので radio へは伝えない
        }
        0x03 => {
            d.agc = v == 0;
            println!("[cmd] ゲイン制御 {}", if d.agc { "AGC" } else { "マニュアル" });
            true
        }
        0x04 => {
            d.gain_db = (v as f32 / 10.0).clamp(0.0, 60.0);
            d.agc = false;
            println!("[cmd] ゲイン {:.1} dB -> gRdB {}", d.gain_db, gain_db_to_gr(d.gain_db));
            true
        }
        0x05 => {
            d.ppm = (v as f64).clamp(-100.0, 100.0);
            println!("[cmd] 周波数補正 {v} ppm");
            true
        }
        0x08 => {
            d.agc = v != 0;
            println!("[cmd] AGC {}", if d.agc { "on" } else { "off" });
            true
        }
        0x0d => match GAIN_TABLE_DB.get(v as usize) {
            Some(db) => {
                d.gain_db = *db;
                d.agc = false;
                println!("[cmd] ゲイン index {v} = {db:.1} dB");
                true
            }
            None => {
                println!("[cmd] ゲイン index {v} は範囲外");
                false
            }
        },
        _ => {
            *unknown += 1;
            if *unknown <= 3 {
                println!("[cmd] 未対応コマンド 0x{code:02X} = {v} (無視)");
            }
            false
        }
    }
}

fn handle_client(mut sock: TcpStream, state: Arc<StreamState>, cmd_tx: Sender<Cmd>, cfg: &Config) {
    let peer = sock.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());
    let _ = sock.set_nodelay(true);

    // RTL-TCP の初期応答 (12 バイト, big-endian)
    let mut header = Vec::with_capacity(12);
    header.extend_from_slice(RTLTCP_MAGIC);
    header.extend_from_slice(&TUNER_TYPE_R820T.to_be_bytes());
    header.extend_from_slice(&(GAIN_TABLE_DB.len() as u32).to_be_bytes());
    if let Err(e) = sock.write_all(&header) {
        println!("[tcp] ヘッダ送信失敗: {e}");
        return;
    }

    // 最初のコマンドが来なければ RTL-TCP クライアントではない (ポートを叩くだけの
    // 接続でデバイスを占有しないよう、ここで弾く)
    let mut raw = [0u8; 5];
    if sock.set_read_timeout(Some(Duration::from_secs(10))).is_err() {
        return;
    }
    let first = sock.read_exact(&mut raw);
    if first.is_err() || raw[0] == 0 || raw[0] > 0x0e {
        println!("[tcp] RTL-TCP 以外の接続を切断: {peer}");
        let _ = sock.shutdown(Shutdown::Both);
        return;
    }
    let _ = sock.set_read_timeout(None);
    println!("[tcp] 接続: {peer}");

    let mut wr = match sock.try_clone() {
        Ok(s) => s,
        Err(e) => {
            println!("[tcp] socket 複製失敗: {e}");
            return;
        }
    };

    let mut desired = Desired {
        out_rate: normalize_rate(cfg.rate),
        freq: cfg.freq,
        gain_db: cfg.gain_db,
        agc: cfg.agc,
        ppm: 0.0,
    };
    let mut unknown = 0usize;
    apply_command(
        &mut desired,
        raw[0],
        u32::from_be_bytes([raw[1], raw[2], raw[3], raw[4]]),
        &state,
        &mut unknown,
    );

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let pending = Arc::new(AtomicUsize::new(0));
    match state.sink.lock() {
        Ok(mut g) => {
            *g = Some(Sink::new(
                tx,
                pending.clone(),
                desired.out_rate,
                cfg.digital_agc,
            ))
        }
        Err(_) => return,
    }

    let writer = std::thread::Builder::new()
        .name("rtltcp-writer".into())
        .spawn(move || {
            while let Ok(buf) = rx.recv() {
                pending.fetch_sub(buf.len(), Ordering::Relaxed);
                if wr.write_all(&buf).is_err() {
                    break;
                }
            }
        })
        .ok();

    let _ = cmd_tx.send(Cmd::Apply(desired));
    println!(
        "[tcp] 配信開始: {:.6} MHz / {} Hz (デジタル AGC {})",
        desired.freq / 1e6,
        desired.out_rate,
        if cfg.digital_agc { "on" } else { "off" }
    );
    println!(
        "[tcp] SDR# 側で Source=RTL-SDR (TCP), Host={}, Port={} を選んでください。",
        cfg.bind, cfg.port
    );

    loop {
        if let Err(e) = sock.read_exact(&mut raw) {
            println!("[tcp] 切断: {peer} ({e})");
            break;
        }
        let changed = apply_command(
            &mut desired,
            raw[0],
            u32::from_be_bytes([raw[1], raw[2], raw[3], raw[4]]),
            &state,
            &mut unknown,
        );
        if changed {
            let _ = cmd_tx.send(Cmd::Apply(desired));
        }
    }

    let _ = sock.shutdown(Shutdown::Both);
    if let Ok(mut g) = state.sink.lock() {
        *g = None;
    }
    let _ = cmd_tx.send(Cmd::Close);
    if let Some(w) = writer {
        let _ = w.join();
    }
    if unknown > 0 {
        println!(
            "[tcp] 切断処理完了: {peer} (未対応コマンド {unknown} 件){}",
            if cfg.release_on_idle { " / RSP1 を解放" } else { " / RSP1 は開いたまま" }
        );
    } else {
        println!(
            "[tcp] 切断処理完了: {peer}{}",
            if cfg.release_on_idle { " / RSP1 を解放" } else { " / RSP1 は開いたまま" }
        );
    }
}

fn main() {
    warn_if_low_integrity();
    let cfg = match Config::parse(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    if cfg.list {
        list_devices();
        return;
    }

    let state = Arc::new(StreamState { sink: Mutex::new(None) });
    let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
    let release_on_idle = cfg.release_on_idle;
    if let Ok(mut g) = CMD_TX.lock() {
        *g = Some(cmd_tx.clone());
    }
    unsafe {
        // Ctrl+C / ウィンドウを閉じたときに RSP1 を解放してから終了する
        SetConsoleCtrlHandler(Some(ctrl_handler), 1);
    }
    {
        let state = state.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("rsp1-radio".into())
            .spawn(move || radio_thread(cmd_rx, state, release_on_idle))
        {
            eprintln!("受信スレッドを起動できません: {e}");
            std::process::exit(1);
        }
    }

    let addr = format!("{}:{}", cfg.bind, cfg.port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{addr} を待ち受けできません: {e}");
            std::process::exit(1);
        }
    };
    println!("RSP1 RTL-TCP ブリッジ起動: {addr}  (Ctrl+C で終了)");
    println!(
        "SDR# : Source = \"RTL-SDR (TCP)\" / Host = {} / Port = {}",
        cfg.bind, cfg.port
    );

    for conn in listener.incoming() {
        match conn {
            Ok(sock) => handle_client(sock, state.clone(), cmd_tx.clone(), &cfg),
            Err(e) => eprintln!("[tcp] accept エラー: {e}"),
        }
    }
}
