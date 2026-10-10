"""One-click Windows OCR, verified language packs and bundled media decoder setup."""
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import process_lifecycle
import threading
import imageio_ffmpeg
from content import ocr_program
from setup_runtime import download,fetch_json


def check_program(path,argument='--version'):
    result=process_lifecycle.run([str(path),argument],capture_output=True,text=True,timeout=20,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
    if result.returncode:
        raise RuntimeError(f'{Path(path).name} failed its readiness check: '+(result.stderr or result.stdout)[-500:])
    return str(path)


def install_ffmpeg(home,cancel,progress):
    if cancel.is_set():
        raise InterruptedError('Setup cancelled')
    # imageio-ffmpeg downloads the platform binary with VectorDash's dependencies.
    source=Path(imageio_ffmpeg.get_ffmpeg_exe())
    check_program(source,'-version')
    target=Path(home)/'tools/ffmpeg/ffmpeg.exe'
    target.parent.mkdir(parents=True,exist_ok=True)
    staging=target.with_suffix('.tmp')
    progress('Setting up included FFmpeg',0,0)
    shutil.copy2(source,staging)
    if hashlib.sha256(staging.read_bytes()).digest()!=hashlib.sha256(source.read_bytes()).digest():
        staging.unlink()
        raise RuntimeError('FFmpeg copy verification failed')
    if cancel.is_set():
        staging.unlink()
        raise InterruptedError('Setup cancelled')
    staging.replace(target)
    return {'ffmpeg_path':check_program(target,'-version')}


def language_codes(value):
    codes=list(dict.fromkeys((value or 'eng').split('+')))
    if any(not re.fullmatch(r'[a-z][a-z0-9_]{1,20}',code) for code in codes):
        raise ValueError('Use OCR language codes separated by +, such as eng+ben+ara.')
    return codes


def install_ocr(settings,home,cancel,progress):
    codes=language_codes(settings.get('ocr_languages','eng'))
    if cancel.is_set():
        raise InterruptedError('Setup cancelled')
    program=ocr_program(settings) or ocr_program(dict(settings,ocr_path=''))
    if not program:
        winget=shutil.which('winget')
        if not winget:
            raise RuntimeError('Windows App Installer (winget) is missing. Install/update App Installer from Microsoft Store, then retry OCR setup.')
        progress('Windows is installing Tesseract OCR; approve its Windows permission prompt if shown',0,0)
        log=Path(home)/'ocr-setup.log'
        # WinGet downloads and verifies the community-maintained Windows package.
        # CurrentUser is the installer's documented per-user mode.
        args=[winget,'install','--id','UB-Mannheim.TesseractOCR','--exact','--source','winget','--silent','--accept-package-agreements','--accept-source-agreements','--disable-interactivity','--override','/S /CurrentUser']
        with log.open('w',encoding='utf-8') as output:
            process=process_lifecycle.popen(args,stdout=output,stderr=output,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
            # Do not interrupt a Windows installer halfway through writing files.
            process.wait()
        program=ocr_program(dict(settings,ocr_path=''))
        if not program:
            detail=log.read_text(encoding='utf-8',errors='replace')[-1200:]
            raise RuntimeError(f'Tesseract setup did not complete (exit {process.returncode}). {detail}')
    check_program(program)
    if cancel.is_set():
        raise InterruptedError('OCR engine setup finished; remaining downloads cancelled')
    progress('Loading official OCR language packs',0,0)
    repo='tesseract-ocr/tessdata_fast'
    revision=fetch_json(f'https://api.github.com/repos/{repo}/commits/main')['sha']
    tree=fetch_json(f'https://api.github.com/repos/{repo}/git/trees/{revision}')['tree']
    files={entry['path']:entry for entry in tree if entry['type']=='blob'}
    target=Path(home)/'tools/tessdata'
    target.mkdir(parents=True,exist_ok=True)
    for code in codes:
        name=code+'.traineddata'
        item=files.get(name)
        if not item:
            raise ValueError(f'OCR language {code} is not present in the official language catalog.')
        download(f'https://raw.githubusercontent.com/{repo}/{revision}/{name}',target/name,item['size'],'git-sha1:'+item['sha'],cancel,lambda done,total:progress('OCR language '+code,done,total))
    # Check the exact managed data folder, not just the engine executable.
    result=process_lifecycle.run([program,'--tessdata-dir',str(target),'--list-langs'],capture_output=True,text=True,timeout=20,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
    if result.returncode or any(code not in result.stdout.splitlines() for code in codes):
        raise RuntimeError('The selected OCR languages did not pass verification.')
    return {'ocr_path':program,'ocr_data':str(target),'ocr_languages':'+'.join(codes)}
