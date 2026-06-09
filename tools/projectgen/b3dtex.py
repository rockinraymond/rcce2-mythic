"""Extract the texture filenames a .b3d references (TEXS chunk) and locate each
in the HF data tree, so an imported mesh's textures can be copied alongside it.
TEXS chunk = repeated: name(null-term) + flags(int) + blend(int) + 5 floats."""
import struct, os, sys, glob

def texs(path):
    data=open(path,'rb').read()
    assert data[:4]==b'BB3D'
    size=struct.unpack_from('<i',data,4)[0]
    off=12; end=12+size; names=[]
    while off+8<=end:
        tag=data[off:off+4]; csize=struct.unpack_from('<i',data,off+4)[0]
        body=off+8; bend=body+csize
        if tag==b'TEXS':
            p=body
            while p<bend:
                s=p
                while data[p]!=0: p+=1
                nm=data[s:p].decode('latin-1'); p+=1
                p+=4+4+5*4  # flags,blend,5 floats
                names.append(nm)
        off=bend
    return names

if __name__=='__main__':
    mesh=sys.argv[1] if len(sys.argv)>1 else 'C:/Users/dyanr/Desktop/HeroesFate/Game/Data/Meshes/Actors/Monsters/Troll.b3d'
    hfroot='C:/Users/dyanr/Desktop/HeroesFate/Game/Data'
    names=texs(mesh)
    print(f"{os.path.basename(mesh)} references {len(names)} textures:")
    for n in names:
        # locate in HF (textures usually under Textures\ or next to the mesh)
        base=os.path.basename(n.replace('\\','/'))
        hits=glob.glob(os.path.join(hfroot,'**',base), recursive=True)
        print(f"  {n!r}  -> {hits[0] if hits else 'NOT FOUND'}")
