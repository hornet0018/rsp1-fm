#!/usr/bin/env python3
"""Python 側と Rust 側で、Global 共有メモリへのアクセス権とトークンを比較する。"""

from __future__ import annotations

import ctypes
import subprocess
import sys
from ctypes import wintypes

DLL = r"C:\Program Files\SDRplay\API\x64\sdrplay_api.dll"

k = ctypes.WinDLL("kernel32", use_last_error=True)
a = ctypes.WinDLL("advapi32", use_last_error=True)
k.GetCurrentProcess.restype = wintypes.HANDLE
k.GetCurrentProcessId.restype = wintypes.DWORD

TOKEN_QUERY = 0x0008
TokenUser, TokenElevation, TokenIntegrityLevel, TokenIsAppContainer = 1, 20, 25, 29


class SID_AND_ATTRIBUTES(ctypes.Structure):
    _fields_ = [("Sid", ctypes.c_void_p), ("Attributes", wintypes.DWORD)]


class TOKEN_MANDATORY_LABEL(ctypes.Structure):
    _fields_ = [("Label", SID_AND_ATTRIBUTES)]


def token_info() -> None:
    tok = wintypes.HANDLE()
    a.OpenProcessToken.argtypes = [wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)]
    if not a.OpenProcessToken(k.GetCurrentProcess(), TOKEN_QUERY, ctypes.byref(tok)):
        print("  OpenProcessToken failed err=", ctypes.get_last_error())
        return

    def raw(cls: int):
        size = wintypes.DWORD()
        a.GetTokenInformation(tok, cls, None, 0, ctypes.byref(size))
        buf = ctypes.create_string_buffer(size.value)
        if not a.GetTokenInformation(tok, cls, buf, size.value, ctypes.byref(size)):
            return None
        return buf

    a.ConvertSidToStringSidW.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_wchar_p)]
    buf = raw(TokenUser)
    if buf:
        sid = ctypes.cast(buf, ctypes.POINTER(SID_AND_ATTRIBUTES)).contents.Sid
        s = ctypes.c_wchar_p()
        a.ConvertSidToStringSidW(sid, ctypes.byref(s))
        print(f"  token user SID = {s.value}")
    buf = raw(TokenIntegrityLevel)
    if buf:
        sid = ctypes.cast(buf, ctypes.POINTER(TOKEN_MANDATORY_LABEL)).contents.Label.Sid
        a.GetSidSubAuthorityCount.argtypes = [ctypes.c_void_p]
        a.GetSidSubAuthorityCount.restype = ctypes.POINTER(ctypes.c_ubyte)
        a.GetSidSubAuthority.argtypes = [ctypes.c_void_p, wintypes.DWORD]
        a.GetSidSubAuthority.restype = ctypes.POINTER(wintypes.DWORD)
        cnt = a.GetSidSubAuthorityCount(sid).contents.value
        print(f"  integrity RID = {a.GetSidSubAuthority(sid, cnt - 1).contents.value}")
    buf = raw(TokenElevation)
    if buf:
        print(f"  elevated = {ctypes.cast(buf, ctypes.POINTER(wintypes.DWORD)).contents.value}")
    buf = raw(TokenIsAppContainer)
    if buf:
        print(f"  appcontainer = {ctypes.cast(buf, ctypes.POINTER(wintypes.DWORD)).contents.value}")


def try_mapping() -> None:
    k.OpenFileMappingA.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.LPCSTR]
    k.OpenFileMappingA.restype = wintypes.HANDLE
    for name in (b"Global\\sdrSrvComShMem", b"sdrSrvComShMem"):
        ctypes.set_last_error(0)
        h = k.OpenFileMappingA(0x0006, False, name)
        print(f"  OpenFileMappingA({name.decode()}) = {h} err={ctypes.get_last_error()}")


def main() -> int:
    s = wintypes.DWORD()
    k.ProcessIdToSessionId(k.GetCurrentProcessId(), ctypes.byref(s))
    print(f"python pid={k.GetCurrentProcessId()} session={s.value}")
    token_info()
    try_mapping()

    lib = ctypes.WinDLL(DLL)
    lib.sdrplay_api_Open.restype = ctypes.c_int
    lib.sdrplay_api_Close.restype = ctypes.c_int
    r = lib.sdrplay_api_Open()
    print(f"  Python Open = {r}")
    if r == 0:
        lib.sdrplay_api_Close()

    exe = r"E:\github\rsp1-fm\target\debug\examples\raw_load_probe.exe"
    print(f"--- spawn {exe} ---")
    out = subprocess.run([exe], capture_output=True, text=True)
    print(out.stdout.rstrip())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
