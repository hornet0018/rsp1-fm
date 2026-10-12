//! SDRplay API 3.x (sdrplay_api.dll) への最小 FFI バインディング。
//!
//! 構造体はローカルのヘッダ (`C:\Program Files\SDRplay\API\inc\*.h`) に合わせて
//! #[repr(C)] で定義している。x64 / MSVC ABI を前提とする。

use std::ffi::{c_char, c_void, CStr};
use std::path::PathBuf;

pub const SDRPLAY_MAX_DEVICES: usize = 16;

// --- 戻り値 (sdrplay_api_ErrT) ---
pub const API_SUCCESS: i32 = 0;

// --- チューナ選択 (sdrplay_api_TunerSelectT) ---
pub const TUNER_NEITHER: i32 = 0;
pub const TUNER_A: i32 = 1;
pub const TUNER_B: i32 = 2;

// --- 更新理由 (sdrplay_api_ReasonForUpdateT) ---
pub const UPDATE_NONE: i32 = 0;
pub const UPDATE_DEV_FS: i32 = 0x0000_0001;
pub const UPDATE_DEV_PPM: i32 = 0x0000_0002;
pub const UPDATE_TUNER_GR: i32 = 0x0000_8000;
pub const UPDATE_TUNER_FRF: i32 = 0x0002_0000;
pub const UPDATE_TUNER_BW: i32 = 0x0004_0000;
pub const UPDATE_TUNER_IF: i32 = 0x0008_0000;
pub const UPDATE_CTRL_AGC: i32 = 0x0100_0000;

// --- IF 帯域 (sdrplay_api_Bw_MHzT) ---
pub const BW_0_200: i32 = 200;
pub const BW_0_300: i32 = 300;
pub const BW_0_600: i32 = 600;
pub const BW_1_536: i32 = 1536;
pub const BW_5_000: i32 = 5000;

// --- IF 周波数 (sdrplay_api_If_kHzT) ---
pub const IF_ZERO: i32 = 0;
pub const IF_450: i32 = 450;

// --- AGC (sdrplay_api_AgcControlT) ---
pub const AGC_DISABLE: i32 = 0;
pub const AGC_100HZ: i32 = 1;
pub const AGC_50HZ: i32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GainValuesT {
    pub curr: f32,
    pub max: f32,
    pub min: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GainT {
    pub g_rdb: i32,
    pub lna_state: u8,
    pub sync_update: u8,
    pub min_gr: i32,
    pub gain_vals: GainValuesT,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RfFreqT {
    pub rf_hz: f64,
    pub sync_update: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DcOffsetTunerT {
    pub dc_cal: u8,
    pub speed_up: u8,
    pub track_time: i32,
    pub refresh_rate_time: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TunerParamsT {
    pub bw_type: i32,
    pub if_type: i32,
    pub lo_mode: i32,
    pub gain: GainT,
    pub rf_freq: RfFreqT,
    pub dc_offset_tuner: DcOffsetTunerT,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DcOffsetT {
    pub dc_enable: u8,
    pub iq_enable: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DecimationT {
    pub enable: u8,
    pub decimation_factor: u8,
    pub wide_band_signal: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AgcT {
    pub enable: i32,
    pub set_point_dbfs: i32,
    pub attack_ms: u16,
    pub decay_ms: u16,
    pub decay_delay_ms: u16,
    pub decay_threshold_db: u16,
    pub sync_update: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ControlParamsT {
    pub dc_offset: DcOffsetT,
    pub decimation: DecimationT,
    pub agc: AgcT,
    pub adsb_mode: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rsp1aTunerParamsT {
    pub bias_t_enable: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rsp2TunerParamsT {
    pub bias_t_enable: u8,
    pub am_port_sel: i32,
    pub antenna_sel: i32,
    pub rf_notch_enable: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RspDuoTunerParamsT {
    pub bias_t_enable: u8,
    pub tuner1_am_port_sel: i32,
    pub tuner1_am_notch_enable: u8,
    pub rf_notch_enable: u8,
    pub rf_dab_notch_enable: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RspDxTunerParamsT {
    pub hdr_bw: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RxChannelParamsT {
    pub tuner_params: TunerParamsT,
    pub ctrl_params: ControlParamsT,
    pub rsp1a_tuner_params: Rsp1aTunerParamsT,
    pub rsp2_tuner_params: Rsp2TunerParamsT,
    pub rspduo_tuner_params: RspDuoTunerParamsT,
    pub rspdx_tuner_params: RspDxTunerParamsT,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rsp1aParamsT {
    pub rf_notch_enable: u8,
    pub rf_dab_notch_enable: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rsp2ParamsT {
    pub ext_ref_output_en: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RspDuoParamsT {
    pub ext_ref_output_en: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RspDxParamsT {
    pub hdr_enable: u8,
    pub bias_t_enable: u8,
    pub antenna_sel: i32,
    pub rf_notch_enable: u8,
    pub rf_dab_notch_enable: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FsFreqT {
    pub fs_hz: f64,
    pub sync_update: u8,
    pub re_cal: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SyncUpdateT {
    pub sample_num: u32,
    pub period: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResetFlagsT {
    pub reset_gain_update: u8,
    pub reset_rf_update: u8,
    pub reset_fs_update: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DevParamsT {
    pub ppm: f64,
    pub fs_freq: FsFreqT,
    pub sync_update: SyncUpdateT,
    pub reset_flags: ResetFlagsT,
    pub mode: i32,
    pub samples_per_pkt: u32,
    pub rsp1a_params: Rsp1aParamsT,
    pub rsp2_params: Rsp2ParamsT,
    pub rspduo_params: RspDuoParamsT,
    pub rspdx_params: RspDxParamsT,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceParamsT {
    pub dev_params: *mut DevParamsT,
    pub rx_channel_a: *mut RxChannelParamsT,
    pub rx_channel_b: *mut RxChannelParamsT,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceT {
    pub ser_no: [c_char; 64],
    pub hw_ver: u8,
    pub tuner: i32,
    pub rspduo_mode: i32,
    pub valid: u8,
    pub rspduo_sample_freq: f64,
    pub dev: *mut c_void,
}

impl DeviceT {
    pub fn zero() -> Self {
        DeviceT {
            ser_no: [0; 64],
            hw_ver: 0,
            tuner: 0,
            rspduo_mode: 0,
            valid: 0,
            rspduo_sample_freq: 0.0,
            dev: std::ptr::null_mut(),
        }
    }
    pub fn serial(&self) -> String {
        let bytes: Vec<u8> = self.ser_no.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
        String::from_utf8_lossy(&bytes).to_string()
    }
    pub fn model(&self) -> &'static str {
        match self.hw_ver {
            1 => "RSP1",
            2 => "RSP2",
            3 => "RSPduo",
            4 => "RSPdx",
            255 => "RSP1A",
            _ => "RSP?",
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StreamCbParamsT {
    pub first_sample_num: u32,
    pub gr_changed: i32,
    pub rf_changed: i32,
    pub fs_changed: i32,
    pub num_samples: u32,
}

pub type StreamCbFn =
    unsafe extern "C" fn(*mut i16, *mut i16, *mut StreamCbParamsT, u32, u32, *mut c_void);
pub type EventCbFn = unsafe extern "C" fn(i32, i32, *mut c_void, *mut c_void) -> ();

#[repr(C)]
pub struct CallbackFnsT {
    pub stream_a: Option<StreamCbFn>,
    pub stream_b: Option<StreamCbFn>,
    pub event: Option<EventCbFn>,
}

/// sdrplay_api_GetLastError が返す詳細情報。
#[repr(C)]
pub struct ErrorInfoT {
    pub file: [c_char; 256],
    pub function: [c_char; 256],
    pub line: i32,
    pub message: [c_char; 1024],
}

/// ロード済みの SDRplay API。
pub struct Api {
    _lib: libloading::Library,
    pub open: unsafe extern "C" fn() -> i32,
    pub close: unsafe extern "C" fn() -> i32,
    pub api_version: unsafe extern "C" fn(*mut f32) -> i32,
    pub lock_device_api: unsafe extern "C" fn() -> i32,
    pub unlock_device_api: unsafe extern "C" fn() -> i32,
    pub get_devices: unsafe extern "C" fn(*mut DeviceT, *mut u32, u32) -> i32,
    pub select_device: unsafe extern "C" fn(*mut DeviceT) -> i32,
    pub release_device: unsafe extern "C" fn(*mut DeviceT) -> i32,
    pub get_error_string: unsafe extern "C" fn(i32) -> *const c_char,
    pub get_last_error: unsafe extern "C" fn(*mut c_void) -> *mut ErrorInfoT,
    pub get_device_params: unsafe extern "C" fn(*mut c_void, *mut *mut DeviceParamsT) -> i32,
    pub init: unsafe extern "C" fn(*mut c_void, *mut CallbackFnsT, *mut c_void) -> i32,
    pub uninit: unsafe extern "C" fn(*mut c_void) -> i32,
    pub update: unsafe extern "C" fn(*mut c_void, i32, i32, i32) -> i32,
}

macro_rules! sym {
    ($lib:expr, $name:literal, $ty:ty) => {{
        let s = unsafe { $lib.get::<$ty>($name) }
            .map_err(|e| format!("シンボル {} が見つかりません: {}", String::from_utf8_lossy(&$name[..$name.len() - 1]), e))?;
        *s
    }};
}

impl Api {
    /// sdrplay_api.dll を探してロードする。
    pub fn load() -> Result<Api, String> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(p) = std::env::var("SDRPLAY_API_DLL") {
            candidates.push(PathBuf::from(p));
        }
        candidates.push(PathBuf::from(r"C:\Program Files\SDRplay\API\x64\sdrplay_api.dll"));
        candidates.push(PathBuf::from("sdrplay_api.dll"));

        let mut last_err = String::new();
        for path in candidates {
            let lib = match unsafe { libloading::Library::new(&path) } {
                Ok(l) => l,
                Err(e) => {
                    last_err = format!("{} を開けません: {e}", path.display());
                    continue;
                }
            };
            // 必要なシンボルを全部取り出す (どれか欠けたらこの候補は失敗)
            let res = (|| -> Result<Api, String> {
                Ok(Api {
                    open: sym!(lib, b"sdrplay_api_Open\0", unsafe extern "C" fn() -> i32),
                    close: sym!(lib, b"sdrplay_api_Close\0", unsafe extern "C" fn() -> i32),
                    api_version: sym!(lib, b"sdrplay_api_ApiVersion\0", unsafe extern "C" fn(*mut f32) -> i32),
                    lock_device_api: sym!(lib, b"sdrplay_api_LockDeviceApi\0", unsafe extern "C" fn() -> i32),
                    unlock_device_api: sym!(lib, b"sdrplay_api_UnlockDeviceApi\0", unsafe extern "C" fn() -> i32),
                    get_devices: sym!(lib, b"sdrplay_api_GetDevices\0", unsafe extern "C" fn(*mut DeviceT, *mut u32, u32) -> i32),
                    select_device: sym!(lib, b"sdrplay_api_SelectDevice\0", unsafe extern "C" fn(*mut DeviceT) -> i32),
                    release_device: sym!(lib, b"sdrplay_api_ReleaseDevice\0", unsafe extern "C" fn(*mut DeviceT) -> i32),
                    get_error_string: sym!(lib, b"sdrplay_api_GetErrorString\0", unsafe extern "C" fn(i32) -> *const c_char),
                    get_last_error: sym!(lib, b"sdrplay_api_GetLastError\0", unsafe extern "C" fn(*mut c_void) -> *mut ErrorInfoT),
                    get_device_params: sym!(lib, b"sdrplay_api_GetDeviceParams\0", unsafe extern "C" fn(*mut c_void, *mut *mut DeviceParamsT) -> i32),
                    init: sym!(lib, b"sdrplay_api_Init\0", unsafe extern "C" fn(*mut c_void, *mut CallbackFnsT, *mut c_void) -> i32),
                    uninit: sym!(lib, b"sdrplay_api_Uninit\0", unsafe extern "C" fn(*mut c_void) -> i32),
                    update: sym!(lib, b"sdrplay_api_Update\0", unsafe extern "C" fn(*mut c_void, i32, i32, i32) -> i32),
                    _lib: lib,
                })
            })();
            match res {
                Ok(api) => return Ok(api),
                Err(e) => last_err = e,
            }
        }
        Err(format!(
            "sdrplay_api.dll を読み込めませんでした。SDRplay API をインストールしてください。({last_err})"
        ))
    }

    /// GetLastError の詳細 (file / function:line message) を 1 行で返す。
    pub fn last_error(&self) -> Option<String> {
        unsafe {
            let p = (self.get_last_error)(std::ptr::null_mut());
            if p.is_null() {
                return None;
            }
            let e = &*p;
            let s = |b: &[c_char]| {
                let bytes: Vec<u8> = b.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
                String::from_utf8_lossy(&bytes).into_owned()
            };
            Some(format!(
                "{} / {}:{} {}",
                s(&e.file),
                s(&e.function),
                e.line,
                s(&e.message).trim()
            ))
        }
    }

    pub fn err_str(&self, code: i32) -> String {
        unsafe {
            let p = (self.get_error_string)(code);
            if p.is_null() {
                format!("エラーコード {code}")
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        }
    }
}
