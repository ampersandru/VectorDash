"""Download pinned official catalogs and install user-selected local assets."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import threading
import urllib.request
import urllib.error
import urllib.parse
import time
from email.utils import parsedate_to_datetime
import zipfile

MODEL_REPO = 'ggml-org/embeddinggemma-2-GGUF'
HEADERS = {'User-Agent': 'VectorDash-Windows/0.3', 'Accept': 'application/json'}


class DownloadError(RuntimeError):
    pass


class SafeRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,req,fp,code,msg,headers,newurl):
        target=urllib.parse.urlparse(newurl)
        if target.scheme!='https':
            raise DownloadError('Download refused an insecure redirect.')
        redirected=super().redirect_request(req,fp,code,msg,headers,newurl)
        if redirected and (target.scheme,target.netloc)!=(urllib.parse.urlparse(req.full_url).scheme,urllib.parse.urlparse(req.full_url).netloc):
            redirected.remove_header('Authorization')
        return redirected


def request_headers(url,settings=None):
    headers=dict(HEADERS)
    parsed=urllib.parse.urlparse(url)
    key={'api.github.com':'github_token','huggingface.co':'hf_token'}.get(parsed.hostname)
    token=(settings or {}).get(key,'') if key else ''
    if token and any(ord(char)<33 or ord(char)>126 for char in token.strip()):
        raise DownloadError('Download token contains invalid characters. Paste only the token value, then save settings.')
    if token and parsed.scheme=='https' and parsed.port in (None,443):
        headers['Authorization']='Bearer '+token.strip()
    return headers


def retry_delay(error,attempt):
    headers=error.headers or {}
    delay=headers.get('Retry-After')
    try:
        if delay is not None:
            try:return max(0,float(delay))
            except ValueError:return max(0,parsedate_to_datetime(delay).timestamp()-time.time())
        if headers.get('X-RateLimit-Remaining')=='0' and headers.get('X-RateLimit-Reset'):
            return max(0,float(headers['X-RateLimit-Reset'])-time.time())+1
    except (ValueError,TypeError,OverflowError):pass
    return 2**attempt


def open_download(request,cancel=None,progress=None):
    cancel=cancel or threading.Event()
    host=urllib.parse.urlparse(request.full_url).hostname
    provider='GitHub' if host in ('api.github.com','github.com','raw.githubusercontent.com') else 'Hugging Face' if host=='huggingface.co' else 'Download server'
    for attempt in range(3):
        if cancel.is_set():raise InterruptedError('Download cancelled; partial files retained')
        try:
            if request.has_header('Authorization'):
                return urllib.request.build_opener(SafeRedirect()).open(request,timeout=30)
            return urllib.request.urlopen(request,timeout=30)
        except urllib.error.HTTPError as error:
            body=error.read(4096).decode('utf-8',errors='replace').lower()
            limited=error.code==429 or error.code==403 and ((error.headers or {}).get('X-RateLimit-Remaining')=='0' or 'rate limit' in body or 'ratelimit' in body)
            transient=limited or error.code in (500,502,503,504)
            delay=retry_delay(error,attempt)
            error.close()
            if transient and attempt<2 and delay<=10:
                if progress:progress(f'{provider} busy; retrying in {delay:g} seconds ({attempt+1}/2)')
                if cancel.wait(delay):raise InterruptedError('Download cancelled; partial files retained')
                continue
            if limited:
                key='GitHub token' if provider=='GitHub' else 'Hugging Face token' if provider=='Hugging Face' else 'access token'
                wait=f' Retry after about {max(1,round(delay))} seconds.' if delay>10 else ' Retry setup later.'
                raise DownloadError(f'{provider} rate limit reached (HTTP {error.code}).'+wait+f' You can add an optional {key} in Models & hardware → Download and set up. Partial downloads are kept; retry resumes them.') from None
            if error.code in (401,403):
                raise DownloadError(f'{provider} denied access (HTTP {error.code}). Check or remove its download token, and check account/model access. This is not necessarily a rate limit.') from None
            raise DownloadError(f'{provider} request failed (HTTP {error.code}). Retry setup; completed downloads are retained.') from None
        except (urllib.error.URLError,TimeoutError,OSError):
            if attempt<2:
                if progress:progress(f'{provider} connection interrupted; retrying')
                if cancel.wait(2**attempt):raise InterruptedError('Download cancelled; partial files retained')
                continue
            raise DownloadError(f'Cannot connect to {provider}. Check your internet connection or proxy, then retry setup. Partial downloads are retained.') from None


_catalog_lock=threading.RLock()
_catalog_cache={}


def fetch_json(url,home=None,settings=None,cancel=None,progress=None):
    # Only public pinned-publisher catalog metadata is cached; never tokens.
    if cancel and cancel.is_set():raise InterruptedError('Download cancelled')
    cached_file=Path(home)/'downloads'/'catalog-cache'/(hashlib.sha256(url.encode()).hexdigest()+'.json') if home else None
    with _catalog_lock:
        cached=_catalog_cache.get(url) if home else None
        if cached_file and cached_file.exists():
            try:cached=json.loads(cached_file.read_text(encoding='utf-8'))
            except (ValueError,OSError):pass
        age=time.time()-cached.get('time',0) if cached else float('inf')
        if cached and 0<=age<600:return cached['data']
        try:
            with open_download(urllib.request.Request(url,headers=request_headers(url,settings)),cancel,progress) as response:data=json.load(response)
        except DownloadError:
            if cached and 0<=age<86400:
                if progress:progress('Using a previously verified publisher catalog while the service is unavailable')
                return cached['data']
            raise
        if home:
            cached={'time':time.time(),'data':data};_catalog_cache[url]=cached
            try:
                cached_file.parent.mkdir(parents=True,exist_ok=True)
                temp=cached_file.with_suffix('.tmp');temp.write_text(json.dumps(cached),encoding='utf-8');temp.replace(cached_file)
            except OSError:pass
        return data


def catalogs(home=None,settings=None,cancel=None,progress=None):
    arch = 'arm64' if platform.machine().lower() in ('arm64','aarch64') else 'x64'
    releases=fetch_json('https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=20',home,settings,cancel,progress)
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
    model=fetch_json(f'https://huggingface.co/api/models/{MODEL_REPO}?blobs=true',home,settings,cancel,progress)
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


def download(url, destination, size, checksum, cancel, progress, settings=None, status=None):
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
        headers=request_headers(url,settings)
        headers.pop('Accept',None)
        if offset:
            headers['Range']=f'bytes={offset}-'
        request=urllib.request.Request(url,headers=headers)
        with open_download(request,cancel,status) as response:
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


def install_runtime(item, home, cancel, progress, settings=None, status=None):
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
        if not re.fullmatch(r'sha256:[0-9a-fA-F]{64}',asset.get('digest') or ''):
            raise RuntimeError('Runtime publisher did not provide a SHA-256 checksum; download refused.')
        if Path(asset['name']).name!=asset['name']:
            raise RuntimeError('Invalid runtime filename')
        free=shutil.disk_usage(home).free
        if free<asset['size']*3:
            raise RuntimeError('Not enough disk space to download and unpack the runtime.')
        archive=download(asset['browser_download_url'],home/'downloads'/asset['name'],asset['size'],asset.get('digest'),cancel,lambda done,total:progress(f'Runtime {index+1}/{len(assets)}',done,total),settings,lambda message:progress(message,0,0))
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


def install_model(item,home,cancel,progress,settings=None):
    if not re.fullmatch(r'[0-9a-fA-F]{64}',item.get('sha256') or ''):
        raise RuntimeError('Model publisher did not provide a SHA-256 checksum; download refused.')
    if Path(item['name']).name!=item['name'] or not item['name'].lower().endswith('.gguf'):
        raise RuntimeError('Invalid model filename')
    home=Path(home)
    if shutil.disk_usage(home).free<item['size']:
        raise RuntimeError('Not enough free disk space for the model.')
    target=home/'models'/item['name']
    return str(download(item['url'],target,item['size'],item.get('sha256'),cancel,lambda done,total:progress('EmbeddingGemma 2',done,total),settings,lambda message:progress(message,0,0)))
