"""Bounded Office XML, archive manifests and model metadata; never extract/run entries."""
from pathlib import Path
import json
import posixpath
import struct
import zipfile
import xml.etree.ElementTree as ET
from content import Segment,chunks
from security import contains_credentials

MAX_ENTRY=8*1024*1024
MAX_TOTAL=64*1024*1024

def office_segments(path):
    with zipfile.ZipFile(path) as archive:
        infos=archive.infolist()
        if len(infos)>10000:raise ValueError('Archive contains too many entries')
        total=0
        def xml(name):
            nonlocal total
            info=archive.getinfo(name);total+=info.file_size
            if info.file_size>MAX_ENTRY or total>MAX_TOTAL or info.file_size/max(info.compress_size,1)>250:
                raise ValueError('Office document exceeds safe decompression limits')
            data=archive.read(info)
            if b'<!DOCTYPE' in data.upper() or b'<!ENTITY' in data.upper():raise ValueError('XML entities are not supported')
            return ET.fromstring(data)
        suffix=path.suffix.lower()
        if suffix=='.docx':
            names=[info.filename for info in infos if info.filename=='word/document.xml' or info.filename.startswith(('word/header','word/footer')) and info.filename.endswith('.xml')]
            for name in names:
                root=xml(name)
                text='\n'.join(' '.join(node.itertext()) for node in root.iter() if node.tag.endswith('}p'))
                for passage in chunks(text):yield Segment(kind='document',text=passage)
        elif suffix=='.pptx':
            names=sorted([i.filename for i in infos if i.filename.startswith('ppt/slides/slide') and i.filename.endswith('.xml')],key=lambda n:int(Path(n).stem[5:]))
            for name in names:
                number=int(Path(name).stem[5:])
                text=' '.join(node.text or '' for node in xml(name).iter() if node.tag.endswith('}t'))
                for passage in chunks(text):yield Segment(kind='slides',text=passage,symbol=f'Slide {number}')
        elif suffix=='.xlsx':
            shared=[]
            if 'xl/sharedStrings.xml' in archive.namelist():
                shared=[''.join(node.itertext()) for node in xml('xl/sharedStrings.xml') if node.tag.endswith('}si')]
            workbook=xml('xl/workbook.xml')
            sheet_nodes=[node for node in workbook.iter() if node.tag.endswith('}sheet')]
            sheets=sorted([i.filename for i in infos if i.filename.startswith('xl/worksheets/sheet') and i.filename.endswith('.xml')],key=lambda n:int(Path(n).stem[5:]))
            relationships={}
            rel_name='xl/_rels/workbook.xml.rels'
            if rel_name in archive.namelist():
                for relation in xml(rel_name):
                    if relation.attrib.get('TargetMode')=='External':continue
                    target=relation.attrib.get('Target','')
                    resolved=posixpath.normpath(target.lstrip('/') if target.startswith('/') else posixpath.join('xl',target))
                    if resolved.startswith('xl/worksheets/') and resolved.endswith('.xml'):
                        relationships[relation.attrib.get('Id')]=resolved
            labeled=[]
            for index,node in enumerate(sheet_nodes):
                label=node.attrib.get('name','Sheet')
                relation_id=next((value for key,value in node.attrib.items() if key.endswith('}id')),None)
                name=relationships.get(relation_id)
                if not name and not relationships and index<len(sheets):name=sheets[index]
                if not name:continue
                labeled.append((label,name))
            for label,name in labeled:
                lines=[]
                for cell in xml(name).iter():
                    if not cell.tag.endswith('}c'):continue
                    value=''.join(n.text or '' for n in cell.iter() if n.tag.endswith(('}v','}t')))
                    if cell.attrib.get('t')=='s' and value.isdigit():value=shared[int(value)] if int(value)<len(shared) else ''
                    if value:lines.append(cell.attrib.get('r','')+': '+value)
                for passage in chunks('\n'.join(lines)):yield Segment(kind='spreadsheet',text=passage,symbol=label)

def archive_segments(path):
    with zipfile.ZipFile(path) as archive:
        infos=archive.infolist()
        if len(infos)>10000:raise ValueError('Archive manifest contains too many entries')
        from vectordash_core import safe
        entries=[f'{info.filename} ({info.file_size:,} bytes)' for info in infos if safe(Path(info.filename)) and not Path(info.filename).is_absolute() and '..' not in Path(info.filename).parts]
        for passage in chunks('\n'.join(entries)):yield Segment(kind='archive',text=passage,symbol='Archive file list')

def weight_segments(path):
    if path.suffix.lower()=='.safetensors':
        with path.open('rb') as stream:
            length=struct.unpack('<Q',stream.read(8))[0]
            if length>1024*1024:raise ValueError('Model metadata header too large')
            header=json.loads(stream.read(length))
        text=json.dumps(header.get('__metadata__',{}),ensure_ascii=False)
        for passage in chunks(text):yield Segment(kind='model_metadata',text=passage)
    else:yield Segment(kind='filename')
