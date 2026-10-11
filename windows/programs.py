"""Small, account-protected Start menu catalog; scanning never launches shortcuts."""
import json
import logging
import os
from pathlib import Path
import re
import subprocess
import threading
import time
import process_lifecycle
from security import seal, unseal

SOURCE = '@programs'
APP_ID = re.compile(r'[A-Za-z0-9_.-]+![A-Za-z0-9_.-]+\Z')

def start_roots():
    return [Path(os.environ[key])/'Microsoft/Windows/Start Menu/Programs'
            for key in ('APPDATA','PROGRAMDATA') if os.environ.get(key)]

def packaged_apps():
    if os.name != 'nt': return []
    script = "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); @(Get-StartApps | Select-Object Name,AppID) | ConvertTo-Json -Compress"
    powershell=Path(os.environ.get('SystemRoot',r'C:\Windows'))/'System32/WindowsPowerShell/v1.0/powershell.exe'
    result = process_lifecycle.run([str(powershell),'-NoProfile','-NonInteractive','-Command',script],
        capture_output=True, timeout=20, check=True, creationflags=subprocess.CREATE_NO_WINDOW)
    rows=json.loads(result.stdout.decode('utf-8-sig'))
    return [rows] if isinstance(rows,dict) else (rows or [])

class ProgramCatalog:
    def __init__(self, home):
        self.path=Path(home)/'programs.catalog'
        self.lock=threading.RLock()
        self.entries=[];self.scanned=''
        if self.path.exists():
            try:
                raw=self.path.read_bytes()
                data=json.loads(unseal(raw) if os.name=='nt' else raw)
                self.entries=data['entries'];self.scanned=data.get('scanned','')
            except Exception:
                logging.getLogger('vectordash').warning('Unable to read program catalog; scan Start menu again.',exc_info=True)

    def scan(self, roots=None, apps=None):
        entries={};warnings=[]
        for root in (start_roots() if roots is None else roots):
            root=Path(root)
            if not root.is_dir():continue
            pending=[root]
            while pending:
                folder=pending.pop()
                try:
                    with os.scandir(folder) as items:
                        for item in items:
                            if item.is_symlink() or getattr(item.stat(follow_symlinks=False),'st_file_attributes',0)&0x400:continue
                            if item.is_dir(follow_symlinks=False):pending.append(Path(item.path))
                            elif item.is_file(follow_symlinks=False) and Path(item.name).suffix.lower() in ('.lnk','.url'):
                                relative=Path(item.path).relative_to(root)
                                entries.setdefault(str(relative).casefold(),{'name':relative.stem,'path':item.path,'group':str(relative.parent) if str(relative.parent)!='.' else 'Start menu'})
                except OSError as error:warnings.append(str(error))
        try:
            for app in (packaged_apps() if apps is None else apps):
                appid=app.get('AppID','');name=app.get('Name','')
                if name and APP_ID.fullmatch(appid):
                    entries.setdefault('app:'+appid,{'name':name,'path':'shell:AppsFolder\\'+appid,'group':'Windows app'})
        except Exception as error:
            warnings.append('Windows app discovery unavailable: '+str(error))
        data={'entries':sorted(entries.values(),key=lambda x:(x['name'].casefold(),x['path'])),'scanned':time.strftime('%Y-%m-%d %H:%M')}
        raw=json.dumps(data,ensure_ascii=False).encode('utf-8')
        temporary=self.path.with_suffix('.tmp');temporary.write_bytes(seal(raw) if os.name=='nt' else raw);temporary.replace(self.path)
        with self.lock:self.entries=data['entries'];self.scanned=data['scanned']
        for warning in warnings:logging.getLogger('vectordash').warning('Program scan: %s',warning)
        return len(data['entries']),warnings

    def search(self, query, settings):
        from engine import SearchResult
        if not settings.get('programs_enabled'):return []
        category=settings.get('search_category','All files')
        explicit=query.lower().startswith('app:')
        if category not in ('All files','Programs') and not explicit:return []
        selected=settings.get('search_sources')
        if selected is not None and SOURCE not in selected:return []
        query=(query[4:] if explicit else query).strip().casefold()
        if not query:return []
        tokens=query.split();results=[]
        hidden={os.path.normcase(os.path.abspath(p)) for p in settings.get('search_excluded_files',[])}
        with self.lock:entries=list(self.entries)
        for entry in entries:
            name=entry['name'].casefold();path=entry['path']
            if os.path.normcase(os.path.abspath(path)) in hidden or not all(token in name for token in tokens):continue
            score=3.5 if name==query else 3.0 if name.startswith(query) else 2.5
            results.append(SearchResult(score,path,entry['group'],kind='application',symbol=entry['name'],query=query,keyword_score=score,reasons=['Matches the program name']))
        return sorted(results,key=lambda r:(-r.score,r.symbol.casefold(),r.path))[:100]
