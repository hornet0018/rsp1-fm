#!/usr/bin/env python3
"""SDRplay API の疎通確認 (Open が失敗する原因を切り分ける)。

    python sdrplay_api_probe.py [--dll PATH]

やること:
  1. sdrplay_api.dll を読み込み、ApiVersion を表示
  2. Open / Close / GetDevices の戻り値を表示
  3. GetErrorString と GetLastError (file/function/line/message) を表示
"""

from __future__ import annotations

import argparse
import ctypes
import os
import sys

DEFAULT_DLL = r"C:\Program Files\SDRplay\API\x64\sdrplay_api.dll"

ERR = {
    0: "Success", 1: "Fail", 2: "InvalidParam", 3: "OutOfRange", 7: "HwError",
    9: "AlreadyInitialised", 10: "NotInitialised", 12: "HwVerError",
    13: "OutOfMemError", 14: "ServiceNotResponding", 15: "StartPending",
    16: "StopPending", 17: "InvalidMode", 18: "FailedVerification1",
    19: "FailedVerification2", 20: "FailedVerification3", 21: "FailedVerification4",
    22: "FailedVerification5", 23: "FailedVerification6", 24: "InvalidServiceVersion",
}


class ErrorInfo(ctypes.Structure):
    _fields_ = [
        ("file", ctypes.c_char * 256),
        ("function", ctypes.c_char * 256),
        ("line", ctypes.c_int),
        ("message", ctypes.c_char * 1024),
    ]


def name(code: int) -> str:
    return f"{code} ({ERR.get(code, '?')})"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dll", default=os.environ.get("SDRPLAY_API_DLL", DEFAULT_DLL))
    args = ap.parse_args()

    print(f"dll: {args.dll}")
    if not os.path.exists(args.dll):
        print("DLL が見つかりません")
        return 1

    # 依存 DLL (libusb-1.0.dll など) を解決できるよう、置いてあるフォルダを追加する
    dll_dir = os.path.dirname(args.dll)
    for extra in (dll_dir, os.path.dirname(dll_dir), r"C:\Program Files\SDRplay"):
        if os.path.isdir(extra):
            os.add_dll_directory(extra)

    try:
        api = ctypes.WinDLL(args.dll)
    except OSError as e:
        print(f"LoadLibrary に失敗: {e}")
        return 1
    print("LoadLibrary: OK")

    api.sdrplay_api_ApiVersion.argtypes = [ctypes.POINTER(ctypes.c_float)]
    api.sdrplay_api_ApiVersion.restype = ctypes.c_int
    api.sdrplay_api_GetErrorString.argtypes = [ctypes.c_int]
    api.sdrplay_api_GetErrorString.restype = ctypes.c_char_p
    api.sdrplay_api_GetLastError.argtypes = [ctypes.c_void_p]
    api.sdrplay_api_GetLastError.restype = ctypes.POINTER(ErrorInfo)

    ver = ctypes.c_float(0.0)
    r = api.sdrplay_api_ApiVersion(ctypes.byref(ver))
    print(f"ApiVersion: ret={name(r)} value={ver.value}")

    def last_error() -> None:
        try:
            p = api.sdrplay_api_GetLastError(None)
            if p:
                e = p.contents
                print(
                    "  GetLastError: "
                    + e.file.decode(errors="replace")
                    + " / "
                    + e.function.decode(errors="replace")
                    + f":{e.line} "
                    + e.message.decode(errors="replace").strip()
                )
            else:
                print("  GetLastError: (null)")
        except Exception as exc:  # noqa: BLE001
            print(f"  GetLastError 呼び出し例外: {exc}")

    r = api.sdrplay_api_Open()
    print(f"Open: ret={name(r)} str={api.sdrplay_api_GetErrorString(r)}")
    last_error()
    if r != 0:
        # もう一度試す (サービスの状態が変わることがある)
        r2 = api.sdrplay_api_Open()
        print(f"Open (2 回目): ret={name(r2)}")
        last_error()
        return 1

    try:
        print(f"Close: ret={name(api.sdrplay_api_Close())}")
    except Exception as exc:  # noqa: BLE001
        print(f"Close 例外: {exc}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
