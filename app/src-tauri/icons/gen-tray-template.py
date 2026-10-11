# Draws icons/tray-template.png: black headphones on transparent, 22pt @2x.
# Run: python3 -I gen-tray-template.py tray-template.png
import math, struct, sys, zlib
S = 44; SS = 4  # 22pt @2x, 4x4 supersampling
def inside(x, y):
    cx, cy = 22.0, 24.0
    d = math.hypot(x - cx, y - cy)
    if y <= cy and 12.5 <= d <= 16.5:   # headband
        return True
    for x0, x1 in ((5.5, 13.5), (30.5, 38.5)):  # ear cups, rounded rects
        y0, y1, r = 21.0, 38.0, 3.0
        if x0 <= x <= x1 and y0 <= y <= y1:
            qx = min(max(x, x0 + r), x1 - r); qy = min(max(y, y0 + r), y1 - r)
            if math.hypot(x - qx, y - qy) <= r:
                return True
    return False
rows = []
for py in range(S):
    row = bytearray([0])
    for px in range(S):
        hits = sum(inside(px + (i + .5) / SS, py + (j + .5) / SS) for i in range(SS) for j in range(SS))
        row += bytes([0, 0, 0, round(255 * hits / SS**2)])
    rows.append(bytes(row))
def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", S, S, 8, 6, 0, 0, 0)) \
    + chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
