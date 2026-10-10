"""Keep startup diagnostics available for the windowed executable."""
import os
from pathlib import Path
import sys
if sys.stdout is None or sys.stderr is None:
    home=Path(os.environ.get('VECTORDASH_DATA_HOME') or Path(os.environ.get('LOCALAPPDATA',Path.home()))/'VectorDash')
    home.mkdir(parents=True,exist_ok=True)
    stream=open(home/'startup.log','a',encoding='utf-8',buffering=1)
    if sys.stdout is None: sys.stdout=stream
    if sys.stderr is None: sys.stderr=stream
