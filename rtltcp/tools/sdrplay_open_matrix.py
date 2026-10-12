#!/usr/bin/env python3
"""sdrplay_api_Open が成功する条件を切り分ける (DLL 検索パス / ApiVersion 先行呼び出し)。

    python sdrplay_open_matrix.py --dirs all --apiversion
    python sdrplay_open_matrix.py --dirs none --apiversion
"""

from __future__ import annotations

import argparse
import ctypes
import os

DLL = r"C:\Program Files\SDRplay\API\x64\sdrplay_api.dll"
ALL_DIRS = [
    r"C:\Program Files\SDRplay\API\x64",
    r"C:\Program Files\SDRplay\API",
    r"C:\Program Files\SDRplay",
]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dirs", choices=["none", "all"], default="all")
    ap.add_argument("--apiversion", action="store_true")
    args = ap.parse_args()

    if args.dirs == "all":
        for d in ALL_DIRS:
            if os.path.isdir(d):
                os.add_dll_directory(d)

    api = ctypes.WinDLL(DLL)
    api.sdrplay_api_Open.restype = ctypes.c_int
    api.sdrplay_api_Close.restype = ctypes.c_int

    ver_txt = "-"
    if args.apiversion:
        api.sdrplay_api_ApiVersion.argtypes = [ctypes.POINTER(ctypes.c_float)]
        api.sdrplay_api_ApiVersion.restype = ctypes.c_int
        v = ctypes.c_float(0.0)
        r = api.sdrplay_api_ApiVersion(ctypes.byref(v))
        ver_txt = f"ret={r} ver={v.value}"

    r = api.sdrplay_api_Open()
    print(f"dirs={args.dirs:4s} apiversion_first={str(args.apiversion):5s} ApiVersion[{ver_txt}] Open={r}")
    if r == 0:
        api.sdrplay_api_Close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
