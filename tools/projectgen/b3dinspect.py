"""Minimal Blitz3D .b3d inspector — lists NODE (joint/bone) names, ANIM frame
count, and vertex/bone stats. Used to compare a working actor mesh (stag) against
the ones that crash the Blitz client (rat, Orc) to find the structural difference
(e.g. a missing 'Head' joint the client expects, or anomalous bone/frame counts).

b3d = "BB3D" + int size + int version, then nested chunks: 4-char tag + int size +
data. NODE = name(null-term) + 9 floats (pos/scale/rot... actually pos3+scale3+quat4)
+ subchunks (MESH/BONE/KEYS/ANIM/NODE).
"""
import struct, sys, os

def inspect(path):
    data = open(path, 'rb').read()
    tag = data[:4]
    assert tag == b'BB3D', f"not a b3d: {tag!r}"
    size = struct.unpack_from('<i', data, 4)[0]
    version = struct.unpack_from('<i', data, 8)[0]
    nodes, anim = [], []
    stats = {'verts': 0, 'tris': 0, 'bones': 0, 'meshes': 0}

    def read_chunk(off, end, depth):
        while off + 8 <= end:
            ctag = data[off:off+4]
            csize = struct.unpack_from('<i', data, off+4)[0]
            body = off + 8
            bend = body + csize
            if ctag == b'NODE':
                # name = null-terminated
                p = body
                while data[p] != 0:
                    p += 1
                name = data[body:p].decode('latin-1')
                nodes.append((depth, name))
                # skip name + 10 floats (pos3, scale3, rot4)
                sub = p + 1 + 10*4
                read_chunk(sub, bend, depth+1)
            elif ctag == b'ANIM':
                # flags(int), frames(int), fps(float)
                flags, frames = struct.unpack_from('<ii', data, body)
                anim.append(frames)
            elif ctag == b'BONE':
                stats['bones'] += 1
            elif ctag == b'MESH':
                stats['meshes'] += 1
                read_chunk(body+4, bend, depth)  # skip brush int, recurse for VRTS/TRIS
            elif ctag == b'VRTS':
                # flags(int) tc_sets(int) tc_size(int) then verts
                pass
            # don't recurse into unknown leaf chunks
            off = bend

    read_chunk(12, 12 + size, 0)
    print(f"\n=== {os.path.basename(path)} (v{version/100:.2f}, {len(data)} bytes) ===")
    print(f"  ANIM frames: {anim}")
    print(f"  bones: {stats['bones']}, mesh chunks: {stats['meshes']}, total nodes: {len(nodes)}")
    names = [n for _, n in nodes]
    print(f"  has 'Head' joint: {'Head' in names}  (case-insensitive: {any(n.lower()=='head' for n in names)})")
    # print first ~40 node names
    shown = [f"{'  '*d}{n}" for d, n in nodes[:40]]
    print("  nodes:\n    " + "\n    ".join(shown))
    if len(nodes) > 40:
        print(f"    ... (+{len(nodes)-40} more)")

if __name__ == '__main__':
    base = os.path.join(os.path.dirname(__file__), '..', '..', 'data', 'Meshes')
    for rel in ['Actors/Animals/stag.b3d', 'Actors/Animals/rat.b3d', 'Actors/Orks/Orc.b3d']:
        try:
            inspect(os.path.join(base, rel))
        except Exception as e:
            print(f"\n=== {rel} ===\n  PARSE ERROR: {e!r}")
