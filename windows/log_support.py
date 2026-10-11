"""Bounded model log tail and severity detection."""
import re

def log_level(line):
    match=re.search(r'\b(DEBUG|INFO|WARNING|ERROR|CRITICAL)\b',line)
    if match:return match.group(1)
    match=re.match(r'^\s*(?:\d[\d.:]*\s+)?([DIWEF])\s+',line)
    return {'D':'DEBUG','I':'INFO','W':'WARNING','E':'ERROR','F':'CRITICAL'}.get(match.group(1),'INFO') if match else 'INFO'

def tail(path,limit=256*1024):
    if not path.is_file():return []
    with path.open('rb') as stream:
        stream.seek(0,2);start=max(0,stream.tell()-limit);stream.seek(start)
        if start:stream.readline()
        return stream.read(limit).decode('utf-8',errors='replace').splitlines()
