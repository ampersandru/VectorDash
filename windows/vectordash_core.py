"""VectorDash: local multimodal file search with llama.cpp embeddings."""
import os
import base64
import json
import math
import urllib.request
import urllib.error
import zipfile
import xml.etree.ElementTree as ET

SKIP = {'.git', '.ssh', '.aws', 'node_modules', 'windows', '$recycle.bin', 'system volume information', 'appdata', '__pycache__'}
SECRET = {'.pem', '.key', '.pfx', '.p12', '.kdbx'}
TEXT = {'.txt', '.md', '.csv', '.log', '.html', '.rtf'}

def safe(path):
    return not any(p.lower().startswith('.') or p.lower() in SKIP for p in path.parts) and path.suffix.lower() not in SECRET and not any(s in path.name.lower() for s in ('password', 'credential', 'secret'))

def read_text(path):
    if path.stat().st_size > 8 * 1024 * 1024:
        return ''
    if path.suffix.lower() in TEXT:
        return path.read_text(encoding='utf-8', errors='replace')
    if path.suffix.lower() == '.docx':
        with zipfile.ZipFile(path) as z:
            if z.getinfo('word/document.xml').file_size > 8 * 1024 * 1024:
                return ''
            root = ET.fromstring(z.read('word/document.xml'))
            return ' '.join(root.itertext())
    return ''

def cosine(a, b):
    if len(a) != len(b):
        return 0
    norm = math.sqrt(sum(x*x for x in a) * sum(x*x for x in b))
    return sum(x*y for x,y in zip(a,b)) / norm if norm else 0

def command(settings):
    args = [settings['server'], '-m', settings['model'], '--embedding', '--host', '127.0.0.1', '--port', '8787', '-c', '2048', '--batch-size', '2048', '--ubatch-size', '2048', '--parallel', '1']
    args += ['--pooling', 'mean']
    if settings.get('projector') and any(settings.get(key,True) for key in ('index_images','index_audio','index_video')):
        args += ['--mmproj', settings['projector'], '--image-min-tokens', '70', '--image-max-tokens', '280']
        if settings['mode'] == 'CPU / system RAM':
            args += ['--no-mmproj-offload']
    if settings['mode'] == 'CPU / system RAM':
        args += ['--device', 'none', '--gpu-layers', '0']
    else:
        if not settings.get('device'):
            raise ValueError('Select a device identifier from List devices before starting GPU inference.')
        args += ['--device', settings['device'], '--gpu-layers', 'all']
    return args

def server_url(settings=None):
    return (settings.get('remote_url','') if settings and settings.get('server_mode')=='Remote' else os.environ.get('VECTORDASH_SERVER_URL','http://127.0.0.1:8787')).rstrip('/')

def embedding_request(text, settings=None):
    if isinstance(text, bytes):
        with urllib.request.urlopen(urllib.request.Request(server_url(settings)+'/props',headers={'Authorization':'Bearer '+settings['remote_key']} if settings and settings.get('remote_key') else {}), timeout=5) as response:
            props=json.load(response)
        marker=props.get('media_marker')
        if not marker:
            raise RuntimeError('This llama-server does not expose multimodal embeddings. Install a current runtime and its matching encoder.')
        text={'prompt_string': marker, 'multimodal_data': [base64.b64encode(text).decode('ascii')]}
    request = urllib.request.Request(server_url(settings)+'/v1/embeddings', data=json.dumps({'input': text}).encode(), headers={'Content-Type': 'application/json', **({'Authorization': 'Bearer '+settings['remote_key']} if settings and settings.get('remote_key') else {})})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            vector = json.load(response)['data'][0]['embedding']
        if not vector or not all(isinstance(x, (int, float)) and math.isfinite(x) for x in vector):
            raise RuntimeError('Model returned an invalid embedding')
        return vector
    except urllib.error.HTTPError as error:
        detail = error.read().decode('utf-8', errors='replace')[:2000]
        # Only input-size failures can be recovered by splitting. Other server
        # failures must remain visible and preserve the previous index.
        lower = detail.lower()
        if error.code in (400, 500) and any(term in lower for term in
                ('too large', 'too long', 'batch size', 'context size', 'context length', 'exceeds')):
            raise InputTooLarge(detail) from error
        raise RuntimeError(f'Model HTTP {error.code}: {detail or error.reason}') from error


class InputTooLarge(RuntimeError):
    pass


def embed(text, checkpoint=lambda: None, request=None):
    """Embed all bounded extracted text, splitting oversized model requests.

    Character counts cannot predict token counts, especially for code and
    multilingual text. Let the runtime reject an oversized chunk, then bisect
    it without discarding any content. Pool the successful chunk vectors.
    """
    request = request or embedding_request
    pending = [text[i:i+6000] for i in range(0, len(text), 6000)] or ['']
    vectors = []
    while pending:
        checkpoint()
        chunk = pending.pop()
        try:
            vectors.append((request(chunk), max(1, len(chunk))))
        except InputTooLarge:
            if len(chunk) <= 1:
                raise RuntimeError('Model cannot embed a minimal input; check its context/batch configuration')
            midpoint = len(chunk) // 2
            pending.extend((chunk[:midpoint], chunk[midpoint:]))
    dimensions = len(vectors[0][0])
    if any(len(vector) != dimensions for vector, _ in vectors):
        raise RuntimeError('Model embedding dimensions changed during indexing')
    total = sum(weight for _, weight in vectors)
    pooled = [sum(vector[i] * weight for vector, weight in vectors) / total for i in range(dimensions)]
    norm = math.sqrt(sum(x*x for x in pooled))
    return [x / norm for x in pooled] if norm else pooled


if __name__ == '__main__':
    from desktop import main
    raise SystemExit(main())
