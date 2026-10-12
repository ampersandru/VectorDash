from pathlib import Path
import importlib.metadata
from PyInstaller.utils.hooks import collect_data_files
root=Path(SPEC).resolve().parents[1]
datas=[(str(root/'windows/assets'),'assets'),(str(root/'LICENSE'),'notices'),(str(root/'ACKNOWLEDGEMENTS.md'),'notices'),(str(root/'README.md'),'notices')]
datas.append((str(root/'docs'),'notices/docs'))
datas.append((str(root/'packaging/notices'),'notices/database'))
datas+=collect_data_files('imageio_ffmpeg')
for dependency in ('sqlcipher3','apsw'):
    distribution=importlib.metadata.distribution(dependency)
    for file in distribution.files or []:
        if '.dist-info/licenses/' in str(file):datas.append((str(distribution.locate_file(file)),str(Path('notices')/dependency)))
datas+=collect_data_files('PySide6',includes=['**/licenses/**'])
# Preserve the complete application and build source alongside the executable.
for folder in ('windows','packaging'):
    for file in (root/folder).rglob('*'):
        if file.is_file() and not any(part in ('.smoke','.venv','__pycache__') for part in file.parts) and file.suffix in ('.py','.txt','.md','.spec','.iss','.ps1'):
            datas.append((str(file),str(Path('source')/file.relative_to(root).parent)))
a=Analysis([str(root/'windows/desktop.py')],pathex=[str(root/'windows')],binaries=[],datas=datas,hiddenimports=[],hookspath=[],hooksconfig={},runtime_hooks=[str(root/'packaging/runtime_logging.py')],excludes=['tkinter','matplotlib','pandas','scipy'],noarchive=False)
pyz=PYZ(a.pure)
exe=EXE(pyz,a.scripts,[],exclude_binaries=True,name='VectorDash',debug=False,bootloader_ignore_signals=False,strip=False,upx=False,console=False,icon=str(root/'packaging/VectorDash.ico'),version=str(root/'packaging/version_info.txt'))
worker=EXE(pyz,a.scripts,[],exclude_binaries=True,name='VectorDash-Worker',debug=False,bootloader_ignore_signals=False,strip=False,upx=False,console=True)
coll=COLLECT(exe,worker,a.binaries,a.datas,strip=False,upx=False,name='VectorDash')

