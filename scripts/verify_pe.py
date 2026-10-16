#!/usr/bin/env python3
"""Static inspection of the Windows deliverable, without executing it."""
import hashlib, json, pathlib, struct, sys, xml.etree.ElementTree as ET

def inspect(path):
    data=pathlib.Path(path).read_bytes()
    def u16(offset):return struct.unpack_from('<H',data,offset)[0]
    def u32(offset):return struct.unpack_from('<I',data,offset)[0]
    assert data[:2]==b'MZ','Missing DOS header'
    pe=u32(0x3c)
    assert data[pe:pe+4]==b'PE\0\0','Missing PE signature'
    assert u16(pe+4)==0x8664,'Expected Windows x64'
    opt=pe+24
    assert u16(opt)==0x20b,'Expected PE32+'
    assert u16(opt+68)==2,'Expected GUI subsystem'
    sections=[]
    for i in range(u16(pe+6)):
        off=opt+u16(pe+20)+40*i
        sections.append((u32(off+12),max(u32(off+8),u32(off+16)),u32(off+20)))
    def offset(rva):
        for address,size,raw in sections:
            if address<=rva<address+size:return raw+rva-address
        raise ValueError(f'Unmapped RVA {rva:#x}')
    def directory(index):return u32(opt+112+index*8),u32(opt+116+index*8)
    imports=[]
    rva,_=directory(1)
    if rva:
        off=offset(rva)
        while u32(off+12):
            name=offset(u32(off+12));imports.append(data[name:data.index(0,name)].decode('ascii'));off+=20
    forbidden=('libgcc','libstdc++','winpthread')
    assert not any(any(word in dll.lower() for word in forbidden) for dll in imports),'Unexpected toolchain runtime DLL dependency'
    rva,_=directory(2);base=offset(rva)
    resources={}
    def walk(relative,chain):
        off=base+relative
        for i in range(u16(off+12)+u16(off+14)):
            entry=off+16+i*8;name=u32(entry);child=u32(entry+4)
            assert name&0x80000000==0,'Unexpected named resource'
            key=(*chain,name)
            if child&0x80000000:walk(child&0x7fffffff,key)
            else:
                leaf=base+child;start=offset(u32(leaf));size=u32(leaf+4);resources[key]=data[start:start+size]
    walk(0,())
    manifest=next(value for key,value in resources.items() if key[:2]==(24,1))
    xml=ET.fromstring(manifest)
    assert any(e.tag.endswith('dpiAwareness') and e.text=='PerMonitorV2' for e in xml.iter()),'Missing PMv2'
    assert any(e.tag.endswith('requestedExecutionLevel') and e.attrib.get('level')=='asInvoker' for e in xml.iter()),'Unexpected elevation'
    assert b'Microsoft.Windows.Common-Controls' in manifest,'Missing Common Controls v6'
    assert any(key[0]==14 for key in resources),'Missing group icon'
    assert sum(key[0]==3 for key in resources)>=7,'Missing icon resolutions'
    version=next(value for key,value in resources.items() if key[0]==16)
    assert 'Orbit'.encode('utf-16le') in version,'Missing product metadata'
    flags=u16(opt+70)
    assert flags&0x40 and flags&0x100,'ASLR/DEP not enabled'
    _,certificate_size=directory(4)
    return {'format':'Windows x64 PE32+ GUI','bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),
        'imports':sorted(imports,key=str.lower),'manifest':{'dpi':'PerMonitorV2','execution_level':'asInvoker','common_controls':6},
        'embedded_icon_images':sum(key[0]==3 for key in resources),'version_metadata':True,
        'aslr':bool(flags&0x40),'dep':bool(flags&0x100),'authenticode_signed':certificate_size>0,
        'inspection_scope':'Static PE structure, resources and imports; does not execute or validate Windows runtime behavior.'}

if __name__=='__main__':print(json.dumps(inspect(sys.argv[1]),indent=2))
