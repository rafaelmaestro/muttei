import math, struct, sys, zlib

def seg(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    t = max(0, min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy or 1)))
    return math.hypot(px - ax - t * dx, py - ay - t * dy)

def mic(px, py, off, w=1.1, fill=False):
    body = seg(px, py, 12, 6, 12, 11) - 3
    d = body if fill else abs(body)
    if py >= 11:
        d = min(d, abs(math.hypot(px - 12, py - 11) - 7))
    else:
        d = min(d, math.hypot(px - 5, py - 11), math.hypot(px - 19, py - 11))
    d = min(d, seg(px, py, 12, 18, 12, 21)) - w
    if off:
        slash = seg(px, py, 3, 3, 21, 21) - w
        d = min(max(d, 1.3 - slash), slash)
    return d

def png(path, size, fg, bg=None, off=False, pad=0.0, w=1.1, fill=False, circle=False):
    ss, rows = 4, []
    inner = size * (1 - 2 * pad)
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            cov = 0
            for sy in range(ss):
                for sx in range(ss):
                    u = ((x + (sx + .5) / ss) - size * pad) / inner * 24
                    v = ((y + (sy + .5) / ss) - size * pad) / inner * 24
                    cov += mic(u, v, off, w, fill) <= 0
            a = cov / ss / ss
            if bg and circle:
                e = size / 2 - math.hypot(x + .5 - size / 2, y + .5 - size / 2)
                k = max(0.0, min(1.0, e + .5))
                if k == 0:
                    row += bytes(4); continue
                row += bytes(round(bg[i] * (1 - a) + fg[i] * a) for i in range(3)) + bytes([round(k * 255)])
            elif bg:
                r = size * .22
                cx, cy = min(max(x + .5, r), size - r), min(max(y + .5, r), size - r)
                inside = math.hypot(x + .5 - cx, y + .5 - cy) <= r
                if not inside:
                    row += bytes(4); continue
                row += bytes(round(bg[i] * (1 - a) + fg[i] * a) for i in range(3)) + b'\xff'
            else:
                row += bytes(fg) + bytes([round(a * 255)])
        rows.append(bytes(row))
    def chunk(t, d):
        return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    raw = zlib.compress(b''.join(rows), 9)
    with open(path, 'wb') as f:
        f.write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0)) + chunk(b'IDAT', raw) + chunk(b'IEND', b''))

out = sys.argv[1]
png(f'{out}/tray.png', 64, (0xf8, 0xf8, 0xf2), w=1.7, fill=True)
# ponytail: no slash on the muted icon; at panel size (16-22px) a slash turns to noise and the red disc already says "mudo"
png(f'{out}/tray-muted.png', 64, (0xff, 0xff, 0xff), bg=(0xff, 0x55, 0x55), pad=.2, w=1.9, fill=True, circle=True)
for s, n in ((32, '32x32'), (128, '128x128'), (512, 'icon')):
    png(f'{out}/{n}.png', s, (0xbd, 0x93, 0xf9), bg=(0x28, 0x2a, 0x36), pad=.2)
