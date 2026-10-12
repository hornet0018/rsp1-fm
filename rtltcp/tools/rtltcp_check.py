#!/usr/bin/env python3
"""RTL-TCP ブリッジ (rsp1-rtltcp) の動作確認クライアント。

SDR# の代わりに RTL-TCP で接続し、ヘッダ・コマンド・IQ の流れを検証する。

    python rtltcp_check.py --freq 80.0 --rate 2048000 --seconds 1.5
"""

from __future__ import annotations

import argparse
import socket
import struct
import sys
import time

import numpy as np


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=1234)
    ap.add_argument("--freq", default="80.0", help="MHz (小数点付き) または Hz")
    ap.add_argument("--rate", type=int, default=2_048_000)
    ap.add_argument("--gain-db", type=float, default=30.0)
    ap.add_argument("--seconds", type=float, default=1.5)
    ap.add_argument("--agc", action="store_true", help="AGC を有効にする (既定はマニュアル)")
    args = ap.parse_args()

    freq_hz = int(float(args.freq) * 1e6) if "." in str(args.freq) else int(args.freq)

    sock = socket.create_connection((args.host, args.port), timeout=10)
    sock.settimeout(10)
    header = b""
    while len(header) < 12:
        chunk = sock.recv(12 - len(header))
        if not chunk:
            print("ヘッダ受信で切断されました")
            return 1
        header += chunk
    magic = header[:4]
    tuner, gain_count = struct.unpack(">II", header[4:12])
    print(f"header: magic={magic!r} tuner={tuner} gain_count={gain_count}")
    if magic != b"RTL0":
        print("magic が RTL0 ではありません")
        return 1

    def send(code: int, value: int) -> None:
        sock.sendall(struct.pack(">BI", code, value))

    send(0x02, args.rate)  # set sample rate
    send(0x01, freq_hz)  # set center frequency
    if args.agc:
        send(0x03, 0)  # gain mode auto
    else:
        send(0x04, int(round(args.gain_db * 10)))  # gain (tenths of dB)
    print(f"命令送信: freq={freq_hz} Hz rate={args.rate} Hz gain={args.gain_db} dB agc={args.agc}")

    want = int(args.rate * 2 * args.seconds)
    buf = bytearray(want + (1 << 16))
    mv = memoryview(buf)
    filled = 0
    calls = 0
    t0 = time.time()
    first_byte_at = None
    while filled < want:
        try:
            n = sock.recv_into(mv[filled:])
        except socket.timeout:
            print("受信タイムアウト")
            break
        if n == 0:
            print("サーバから切断されました")
            break
        if first_byte_at is None:
            first_byte_at = time.time()
        filled += n
        calls += 1
    elapsed = time.time() - t0
    stream_secs = (time.time() - first_byte_at) if first_byte_at else elapsed
    sock.close()
    del mv
    buf = buf[:filled]

    usable = len(buf) // 2 * 2
    if usable < 4096:
        print(f"受信バイト数が少なすぎます: {len(buf)}")
        return 1
    raw = np.frombuffer(bytes(buf[:usable]), dtype=np.uint8).astype(np.float32)
    iq = (raw - 127.5) / 128.0
    i, q = iq[0::2], iq[1::2]
    print(
        f"受信: {len(buf)} バイト / {len(i)} サンプル / {elapsed:.2f} s "
        f"(実効 {len(i) / elapsed / 1e6:.3f} Msps / 定常 {len(i) / max(stream_secs, 1e-6) / 1e6:.3f} Msps / recv {calls} 回)"
    )
    print(f"振幅: min={iq.min():.3f} max={iq.max():.3f} mean={iq.mean():.4f} std={iq.std():.4f}")
    if float(iq.std()) < 1e-4:
        print("IQ がほぼ一定です (無信号ではなくデータが来ていない可能性)")
        return 1

    n = 1 << int(np.log2(min(len(i), 1 << 20)))
    if n < 1024:
        n = 1 << int(np.log2(len(i)))
    z = i[:n] + 1j * q[:n]
    z = z - z.mean()  # DC (中心の DC オフセット) を除去して評価する
    x = z * np.hanning(n)
    spec = np.fft.fftshift(np.abs(np.fft.fft(x)) / n)
    db = 20 * np.log10(spec + 1e-9)
    freqs = np.fft.fftshift(np.fft.fftfreq(n, 1 / args.rate))
    gate = np.abs(freqs) > 5e3  # ±5 kHz 以内 (DC 成分) は無視
    idx = np.argsort(np.where(gate, db, -999.0))[::-1]
    print(f"スペクトル({n}点, DC除去): ノイズ中央値 {np.median(db[gate]):.1f} dB")
    for k in idx[:5]:
        print(f"  ピーク {freqs[k] / 1e3:+8.1f} kHz : {db[k]:7.1f} dB")

    # 周期性 (パケット境界などの周期的な傷) を探す
    m = np.abs(z[: min(len(z), 1 << 16)]).astype(np.float64)
    m = m - m.mean()
    denom = float(np.dot(m, m)) + 1e-12
    lags = np.arange(64, 400)
    ac = np.array([float(np.dot(m[:-l], m[l:])) / denom for l in lags])
    order = np.argsort(ac)[::-1]
    top = ", ".join(f"{lags[o]} サンプル ({ac[o]:.2f})" for o in order[:4])
    print(f"自己相関の上位ラグ: {top}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
