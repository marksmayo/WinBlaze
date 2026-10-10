import struct,sys
from pathlib import Path
p=Path(sys.argv[2]) if len(sys.argv) > 2 else Path('src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe')
b=p.read_bytes()
pe=struct.unpack_from('<I',b,0x3c)[0]
n=struct.unpack_from('<H',b,pe+6)[0];optional=pe+24
assert struct.unpack_from('<H',b,optional)[0]==0x20b
size=struct.unpack_from('<H',b,pe+20)[0]
sections=[]
for i in range(n):
    off=optional+size+i*40
    virtual_size,virtual,raw_size,raw=struct.unpack_from('<IIII',b,off+8)
    sections.append((virtual,max(virtual_size,raw_size),raw))
def offset(rva):
    for virtual,size,raw in sections:
        if virtual<=rva<virtual+size:return raw+rva-virtual
    raise AssertionError('Invalid PE RVA')
def name(rva):
    off=offset(rva);return b[off:b.index(b'\0',off)].decode('ascii').lower()
def imports(directory,delayed=False):
    rva,_=struct.unpack_from('<II',b,optional+112+directory*8)
    if not rva:return set()
    off=offset(rva);names=set()
    while any(b[off:off+(32 if delayed else 20)]):
        names.add(name(struct.unpack_from('<I',b,off+(4 if delayed else 12))[0]))
        off+=32 if delayed else 20
    return names
dll=sys.argv[1].lower()
assert dll not in imports(1),'Eager executable import remains: '+dll
assert dll in imports(13,True),'Missing first-use import: '+dll
print('PASS: '+dll+' is bound on first use, not an eager executable import.')
