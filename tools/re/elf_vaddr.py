import struct,sys
class ELF:
    def __init__(self,p):
        self.d=open(p,'rb').read()
        self.phoff,=struct.unpack_from("<Q",self.d,0x20)
        self.phentsize,self.phnum=struct.unpack_from("<HH",self.d,0x36)
        self.segs=[]
        for i in range(self.phnum):
            o=self.phoff+i*self.phentsize
            p_type,p_flags=struct.unpack_from("<II",self.d,o)
            p_offset,p_vaddr,p_paddr,p_filesz,p_memsz,p_align=struct.unpack_from("<QQQQQQ",self.d,o+8)
            self.segs.append((p_type,p_offset,p_vaddr,p_filesz,p_memsz))
    def read(self,va,n):
        for t,off,va_,fsz,msz in self.segs:
            if t==1 and va_<=va<va_+fsz:
                s=va-va_
                return self.d[off+s:off+s+min(n,fsz-s)]
        return b""
    def cstr(self,va,lim=300):
        b=self.read(va,lim)
        i=b.find(b"\x00")
        return b[:i if i>=0 else lim].decode('utf8','replace')
if __name__=="__main__":
    e=ELF(sys.argv[1])
    n=int(sys.argv[3]) if len(sys.argv)>3 else 11
    base=int(sys.argv[2],16)
    for i in range(n):
        nm,=struct.unpack("<Q",e.read(base+i*24,8))
        sg,=struct.unpack("<Q",e.read(base+i*24+8,8))
        fn,=struct.unpack("<Q",e.read(base+i*24+16,8))
        print(f"{i:2} {e.cstr(nm):32} {e.cstr(sg):60} 0x{fn:x}")
