"""AnimSet codec (Game Data/Animations.dat) — used to port HF's Troll animation
set into rcce2 so an imported HF enemy mesh animates correctly.
Format (Animations.bb LoadAnimSets): repeat until EOF:
  id(short) + name(string) + 150 x [name(string), start(short), end(short), speed(float)]
"""
import struct, os, sys

class R:
    def __init__(self, b): self.b=b; self.p=0
    def eof(self): return self.p>=len(self.b)
    def short(self): v=struct.unpack_from('<h',self.b,self.p)[0]; self.p+=2; return v
    def f(self): v=struct.unpack_from('<f',self.b,self.p)[0]; self.p+=4; return v
    def st(self):
        n=struct.unpack_from('<i',self.b,self.p)[0]; self.p+=4
        v=self.b[self.p:self.p+n]; self.p+=n; return v.decode('latin-1')

class W:
    def __init__(self): self.o=bytearray()
    def short(self,v): self.o+=struct.pack('<h',v)
    def f(self,v): self.o+=struct.pack('<f',v)
    def st(self,s):
        raw=s.encode('latin-1'); self.o+=struct.pack('<i',len(raw)); self.o+=raw

def read_sets(data):
    r=R(data); out=[]
    while not r.eof():
        sid=r.short()
        if sid<0 or sid>999: break
        name=r.st()
        anims=[(r.st(), r.short(), r.short(), r.f()) for _ in range(150)]
        out.append({'id':sid,'name':name,'anims':anims})
    return out

def write_sets(sets):
    w=W()
    for s in sets:
        w.short(s['id']); w.st(s['name'])
        for an,a0,a1,sp in s['anims']:
            w.st(an); w.short(a0); w.short(a1); w.f(sp)
    return bytes(w.o)

DATA=os.path.normpath(os.path.join(os.path.dirname(__file__),'..','..','data'))
HF='C:/Users/dyanr/Desktop/HeroesFate/Game/Data/Game Data/Animations.dat'

if __name__=='__main__':
    rc=read_sets(open(os.path.join(DATA,'Game Data','Animations.dat'),'rb').read())
    raw=open(os.path.join(DATA,'Game Data','Animations.dat'),'rb').read()
    print('rcce2 round-trip:', 'PASS' if write_sets(rc)==raw else 'FAIL')
    print('=== rcce2 sets ===')
    for s in rc:
        named=[a[0] for a in s['anims'] if a[0]]
        print(f"  {s['id']} {s['name']!r}: {len(named)} named")
    hf=read_sets(open(HF,'rb').read())
    print(f"=== HF sets ({len(hf)}) ===")
    for s in hf:
        print(f"  {s['id']} {s['name']!r}")
