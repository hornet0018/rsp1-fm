//! SDRplay API の疎通を切り分ける診断ツール。
//!
//!     cargo run -p rsp1fm-core --example api_probe            # 素の状態
//!     cargo run -p rsp1fm-core --example api_probe -- --setdll  # DLL 検索パスを追加

use rsp1fm_core::sdrplay::Api;
use std::ffi::{c_void, OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetModuleFileNameW(h: *mut c_void, buf: *mut u16, n: u32) -> u32;
    fn SetDllDirectoryW(p: *const u16) -> i32;
    fn GetCurrentProcessId() -> u32;
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn loaded_module_path(name: &str) -> Option<String> {
    unsafe {
        let h = GetModuleHandleW(wide(name).as_ptr());
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let n = GetModuleFileNameW(h, buf.as_mut_ptr(), buf.len() as u32);
        if n == 0 {
            return None;
        }
        buf.truncate(n as usize);
        Some(OsString::from_wide(&buf).to_string_lossy().into_owned())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let setdll = args.iter().any(|a| a == "--setdll");

    unsafe {
        let mut session = 0u32;
        let pid = GetCurrentProcessId();
        ProcessIdToSessionId(pid, &mut session);
        println!("pid={pid} session={session} exe={:?}", std::env::current_exe().ok());
    }
    println!("cwd={:?}", std::env::current_dir().ok());
    println!("SDRPLAY_API_DLL={:?}", std::env::var("SDRPLAY_API_DLL").ok());

    if setdll {
        unsafe {
            let d = wide(r"C:\Program Files\SDRplay");
            let r = SetDllDirectoryW(d.as_ptr());
            println!("SetDllDirectoryW(C:\\Program Files\\SDRplay) = {r}");
        }
    }

    let api = match Api::load() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Api::load 失敗: {e}");
            return;
        }
    };
    println!("Api::load: OK");

    match loaded_module_path("sdrplay_api.dll") {
        Some(p) => println!("ロードされた sdrplay_api.dll: {p}"),
        None => println!("ロードされた sdrplay_api.dll: (名前で取得できず)"),
    }

    unsafe {
        let mut v = 0f32;
        let r = (api.api_version)(&mut v);
        println!("ApiVersion: ret={r} value={v} ({})", api.err_str(r));
        if let Some(e) = api.last_error() {
            println!("  last_error: {e}");
        }
    }

    unsafe {
        let r = (api.open)();
        println!("Open: ret={r} ({})", api.err_str(r));
        if let Some(e) = api.last_error() {
            println!("  last_error: {e}");
        }
        if r == 0 {
            let c = (api.close)();
            println!("Close: ret={c}");
        }
    }
}
