#!/usr/bin/env python3
"""Generate Orbit's original vector-style icon; Python standard library only."""
import math, pathlib, struct, zlib
def png(size):
    rows=[]
    for y in range(size):
        row=bytearray([0])
        for x in range(size):
            channels=[0.,0.,0.,0.]
            for sy in range(3):
                for sx in range(3):
                    px=(x+(sx+.5)/3)/size*256-128
                    py=(y+(sy+.5)/3)/size*256-128
                    # Rounded square with an open ring and a bright north-east segment.
                    qx=max(abs(px)-74,0); qy=max(abs(py)-74,0)
                    inside=math.hypot(qx,qy)<=46
                    radius=math.hypot(px,py)
                    angle=math.atan2(py,px)
                    ring=60<radius<83
                    highlight=ring and -1.57<angle<-.12
                    dot=math.hypot(px,py)<13
                    c=(110,234,210,255) if highlight or dot else (75,104,126,255) if ring else (18,25,36,255) if inside else (0,0,0,0)
                    for k in range(4): channels[k]+=c[k]/9
            row.extend(round(c) for c in channels)
        rows.append(row)
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',size,size,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(rows),9))+chunk(b'IEND',b'')
sizes=[16,24,32,48,64,128,256]
images=[png(n) for n in sizes]
offset=6+16*len(images)
entries=[]
for size,data in zip(sizes,images):
    entries.append(struct.pack('<BBBBHHII',size%256,size%256,0,0,1,32,len(data),offset)); offset+=len(data)
root=pathlib.Path(__file__).resolve().parents[1]
(root/'assets').mkdir(exist_ok=True)
(root/'assets/orbit.ico').write_bytes(struct.pack('<HHH',0,1,len(images))+b''.join(entries)+b''.join(images))
(root/'assets/orbit.png').write_bytes(images[-1])
