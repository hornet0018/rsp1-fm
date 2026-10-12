//! 生の LoadLibraryW + GetProcAddress で sdrplay_api.dll をロードし、
//! プロセスのトークン情報も表示する比較用ツール。

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;

type Handle = *mut c_void;

#[repr(C)]
struct SidAndAttributes {
    sid: *mut c_void,
    attributes: u32,
}

#[repr(C)]
struct TokenMandatoryLabel {
    label: SidAndAttributes,
}

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> Handle;
    fn GetProcAddress(h: Handle, name: *const u8) -> *mut c_void;
    fn GetCurrentProcess() -> Handle;
    fn GetCurrentProcessId() -> u32;
    fn GetModuleFileNameW(h: Handle, buf: *mut u16, n: u32) -> u32;
    fn ProcessIdToSessionId(pid: u32, s: *mut u32) -> i32;
    fn GetLastError() -> u32;
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenFileMappingA(access: u32, inherit: i32, name: *const u8) -> Handle;
    fn OpenProcessToken(h: Handle, access: u32, tok: *mut Handle) -> i32;
    fn GetTokenInformation(tok: Handle, class: u32, buf: *mut c_void, len: u32, ret: *mut u32) -> i32;
    fn GetSidSubAuthority(sid: *mut c_void, index: u32) -> *mut u32;
    fn GetSidSubAuthorityCount(sid: *mut c_void) -> *mut u8;
    fn ConvertSidToStringSidW(sid: *mut c_void, out: *mut *mut u16) -> i32;
}

const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_USER: u32 = 1;
const TOKEN_ELEVATION: u32 = 20;
const TOKEN_INTEGRITY_LEVEL: u32 = 25;
const TOKEN_IS_APP_CONTAINER: u32 = 29;

fn token_raw(tok: Handle, class: u32) -> Option<Vec<u8>> {
    unsafe {
        let mut size = 0u32;
        GetTokenInformation(tok, class, std::ptr::null_mut(), 0, &mut size);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let ok = GetTokenInformation(tok, class, buf.as_mut_ptr() as *mut c_void, size, &mut size);
        if ok == 0 {
            None
        } else {
            Some(buf)
        }
    }
}

fn token_info() {
    unsafe {
        let mut tok: Handle = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
            println!("  OpenProcessToken failed");
            return;
        }
        if let Some(buf) = token_raw(tok, TOKEN_USER) {
            let sa = &*(buf.as_ptr() as *const SidAndAttributes);
            let mut s: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(sa.sid, &mut s) != 0 {
                let mut n = 0usize;
                while *s.add(n) != 0 {
                    n += 1;
                }
                let v = std::slice::from_raw_parts(s, n);
                println!("  token user SID = {}", String::from_utf16_lossy(v));
            }
        }
        if let Some(buf) = token_raw(tok, TOKEN_INTEGRITY_LEVEL) {
            let tml = &*(buf.as_ptr() as *const TokenMandatoryLabel);
            let cnt = *GetSidSubAuthorityCount(tml.label.sid);
            let rid = *GetSidSubAuthority(tml.label.sid, (cnt as u32) - 1);
            println!("  integrity RID = {rid} ({rid:#x})");
        }
        if let Some(buf) = token_raw(tok, TOKEN_ELEVATION) {
            println!("  elevated = {}", *(buf.as_ptr() as *const u32));
        }
        if let Some(buf) = token_raw(tok, TOKEN_IS_APP_CONTAINER) {
            println!("  appcontainer = {}", *(buf.as_ptr() as *const u32));
        }
    }
}

fn main() {
    unsafe {
        let mut session = 0u32;
        ProcessIdToSessionId(GetCurrentProcessId(), &mut session);
        println!("raw_load_probe pid={} session={session}", GetCurrentProcessId());
        println!("  cwd={:?}", std::env::current_dir().ok());
        token_info();
    }

    let path = r"C:\Program Files\SDRplay\API\x64\sdrplay_api.dll";
    let w: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let h = unsafe { LoadLibraryW(w.as_ptr()) };
    println!("  LoadLibraryW handle={h:?}");
    if h.is_null() {
        return;
    }
    unsafe {
        let mut buf = vec![0u16; 1024];
        let n = GetModuleFileNameW(h, buf.as_mut_ptr(), buf.len() as u32);
        buf.truncate(n as usize);
        println!("  dll path = {}", String::from_utf16_lossy(&buf));
    }

    unsafe {
        for name in [
            &b"Global\\sdrSrvComShMem\0"[..],
            &b"sdrSrvComShMem\0"[..],
            &b"Global\\sdrSrvComMtx\0"[..],
        ] {
            let hm = OpenFileMappingA(0x0006, 0, name.as_ptr());
            let err = GetLastError();
            println!(
                "  OpenFileMappingA({}) = {:?} GetLastError={err}",
                String::from_utf8_lossy(&name[..name.len() - 1]),
                hm
            );
        }
    }

    unsafe {
        let p = GetProcAddress(h, b"sdrplay_api_Open\0".as_ptr());
        let open: unsafe extern "C" fn() -> i32 = std::mem::transmute(p);
        let r = open();
        println!("  Rust Open = {r}");
    }
}
