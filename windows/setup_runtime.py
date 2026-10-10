"""Download pinned official catalogs and install user-selected local assets."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import threading
import urllib.request
import zipfile

MODEL_REPO = 'ggml-org/embeddinggemma-2-GGUF'
HEADERS = {'User-Agent': 'VectorDash-Windows/0.3', 'Accept': 'application/json'}


def fetch_json(url):
    with urllib.request.urlopen(urllib.request.Request(url,headers=HEADERS),timeout=30) as response:
        return json.load(response)


def catalogs():
    arch = 'arm64' if platform.machine().lower() in ('arm64','aarch64') else 'x64'
    releases=fetch_json('https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=20')
    runtimes=[]
    for release in releases:
        if release.get('draft'):
            continue
        assets=release['assets']
        for asset in assets:
            name=asset['name']
            if not name.startswith('llama-') or not name.endswith(f'-{arch}.zip') or '-bin-win-' not in name:
                continue
            backend=name.split('-bin-win-',1)[1].removesuffix(f'-{arch}.zip')
            if not (backend in ('cpu','vulkan') or backend.startswith('cuda-')):
                continue
            bundle=[asset]
            if backend.startswith('cuda-'):
                companion=next((a for a in assets if a['name']==f'cudart-llama-bin-win-{backend}-{arch}.zip'),None)
                if companion:
                    bundle.append(companion)
            runtimes.append({'label':f'{backend.upper()} · {release["tag_name"]} · {arch}', 'version':release['tag_name'], 'backend':backend, 'assets':bundle,'size':sum(a['size'] for a in bundle)})
        if runtimes:
            break
    model=fetch_json(f'https://huggingface.co/api/models/{MODEL_REPO}?blobs=true')
    models=[]
    projectors={file['rfilename']: file for file in model.get('siblings',[]) if file['rfilename'].startswith('mmproj-')}
    for file in model.get('siblings',[]):
        name=file['rfilename']
        if not name.startswith('embeddinggemma-2-') or not name.endswith('.gguf'):
            continue
        info=file.get('lfs',{})
        models.append({'label':name.removeprefix('embeddinggemma-2-').removesuffix('.gguf'), 'name':name,'size':info.get('size',file.get('size',0)), 'sha256':info.get('sha256'), 'url':f'https://huggingface.co/{MODEL_REPO}/resolve/{model["sha"]}/{name}'})
    for item in models:
        name='mmproj-embeddinggemma-2-'+item['label']+'.gguf'
        file=projectors.get(name) or projectors.get('mmproj-embeddinggemma-2-Q8_0.gguf')
        if file:
            info=file.get('lfs',{})
            item['projector']={'label':'Vision + audio encoder','name':file['rfilename'],'size':info.get('size',file.get('size',0)),'sha256':info.get('sha256'),'url':f'https://huggingface.co/{MODEL_REPO}/resolve/{model["sha"]}/{file["rfilename"]}'}
    if not runtimes or not models:
        raise RuntimeError('No compatible Windows runtime/model assets were found in the official catalogs.')
    return runtimes,models


def download(url, destination, size, checksum, cancel, progress):
    if cancel.is_set():
        raise InterruptedError('Download cancelled')
    destination=Path(destination)
    destination.parent.mkdir(parents=True,exist_ok=True)
    partial=destination.with_name(destination.name+'.part')
    offset=partial.stat().st_size if partial.exists() else 0
    if size and offset>size:
        partial.unlink()
        offset=0
    if not size or offset<size:
        headers={'User-Agent':HEADERS['User-Agent']}
        if offset:
            headers['Range']=f'bytes={offset}-'
        request=urllib.request.Request(url,headers=headers)
        with urllib.request.urlopen(request,timeout=30) as response:
            if offset and response.status != 206:
                offset=0
            if offset and not response.headers.get('Content-Range','').startswith(f'bytes {offset}-'):
                raise RuntimeError('Download server returned an unexpected resume range.')
            with partial.open('ab' if offset else 'wb') as output:
                while True:
                    if cancel.is_set():
                        raise InterruptedError('Download cancelled; partial file retained for resume')
                    chunk=response.read(1024*1024)
                    if not chunk:
                        break
                    output.write(chunk)
                    offset+=len(chunk)
                    if size and offset>size:
                        raise RuntimeError('Download exceeds the expected file size.')
                    progress(offset,size)
    if size and partial.stat().st_size != size:
        raise RuntimeError('Incomplete download; retry to resume.')
    if checksum:
        git_hash=checksum.startswith('git-sha1:')
        digest=hashlib.sha1() if git_hash else hashlib.sha256()
        if git_hash:
            digest.update(b'blob '+str(partial.stat().st_size).encode()+b'\0')
        with partial.open('rb') as stream:
            while chunk:=stream.read(1024*1024):
                if cancel.is_set():
                    raise InterruptedError('Verification cancelled; partial file retained')
                digest.update(chunk)
        if digest.hexdigest().lower() != checksum.removeprefix('sha256:').removeprefix('git-sha1:').lower():
            partial.unlink()
            raise RuntimeError('Checksum mismatch; corrupt download discarded. Retry the download.')
    if cancel.is_set():
        raise InterruptedError('Download cancelled; partial file retained')
    partial.replace(destination)
    return destination


def extract_archive(archive, target):
    target=Path(target).resolve()
    with zipfile.ZipFile(archive) as bundle:
        if sum(info.file_size for info in bundle.infolist())>4*1024**3:
            raise RuntimeError('Runtime archive expands beyond the allowed size.')
        for info in bundle.infolist():
            name=info.filename.replace('\\','/')
            path=PurePosixPath(name)
            if path.is_absolute() or '..' in path.parts or ':' in name or (info.external_attr >> 16) & 0o170000 == 0o120000:
                raise RuntimeError('Unsafe path in runtime archive.')
            resolved=(target/name).resolve()
            if not resolved.is_relative_to(target):
                raise RuntimeError('Archive entry escapes the installation folder.')
        bundle.extractall(target)


def install_runtime(item, home, cancel, progress):
    home=Path(home)
    target=home/'runtimes'/f'{item["version"]}-{item["backend"]}'
    if target.exists():
        found=list(target.rglob('llama-server.exe'))
        if found:
            return str(found[0])
    staging=target.with_name(target.name+'.staging')
    staging.mkdir(parents=True,exist_ok=True)
    assets=item['assets']
    for index,asset in enumerate(assets):
        free=shutil.disk_usage(home).free
        if free<asset['size']*3:
            raise RuntimeError('Not enough disk space to download and unpack the runtime.')
        archive=download(asset['browser_download_url'],home/'downloads'/asset['name'],asset['size'],asset.get('digest'),cancel,lambda done,total:progress(f'Runtime {index+1}/{len(assets)}',done,total))
        if cancel.is_set():
            raise InterruptedError('Setup cancelled')
        extract_archive(archive,staging)
    found=list(staging.rglob('llama-server.exe'))
    if not found:
        raise RuntimeError('Downloaded runtime does not contain llama-server.exe.')
    # Put runtime dependencies next to the executable even if companion ZIP layouts differ.
    server=found[0]
    for dll in list(staging.rglob('*.dll')):
        if dll.parent != server.parent and not (server.parent/dll.name).exists():
            shutil.copy2(dll,server.parent/dll.name)
    if target.exists():
        raise RuntimeError('An incomplete install already exists; choose another runtime version.')
    relative=server.relative_to(staging)
    staging.rename(target)
    return str(target/relative)


def install_model(item,home,cancel,progress):
    home=Path(home)
    if shutil.disk_usage(home).free<item['size']:
        raise RuntimeError('Not enough free disk space for the model.')
    target=home/'models'/item['name']
    return str(download(item['url'],target,item['size'],item.get('sha256'),cancel,lambda done,total:progress('EmbeddingGemma 2',done,total)))
