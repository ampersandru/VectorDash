"""Reproduce SQLITE_FULL at a small memdb ceiling, then verify protected growth."""
import os
from pathlib import Path
import subprocess
import sys
import unittest

class SqliteGrowthTests(unittest.TestCase):
    @unittest.skipUnless(os.name=='nt','Windows SQLite DLL configuration regression')
    def test_protected_writer_grows_beyond_deserialize_limit(self):
        script=r"""
import ctypes,importlib.util,tempfile
from pathlib import Path
library=ctypes.CDLL(str(Path(importlib.util.find_spec('_sqlite3').origin).with_name('sqlite3.dll')))
library.sqlite3_config.argtypes=[ctypes.c_int,ctypes.c_longlong]
library.sqlite3_config.restype=ctypes.c_int
assert library.sqlite3_config(29,4*1024*1024)==0
import sqlite3
seed=sqlite3.connect(':memory:')
seed.execute('CREATE TABLE payload(data BLOB)')
seed.execute('INSERT INTO payload VALUES(zeroblob(1048576))');seed.commit()
snapshot=seed.serialize();seed.close()
old=sqlite3.connect(':memory:');old.deserialize(snapshot)
try:
    old.execute('INSERT INTO payload VALUES(zeroblob(6291456))')
except sqlite3.OperationalError as error:
    assert error.sqlite_errorcode==sqlite3.SQLITE_FULL
else:raise AssertionError('Old storage did not reproduce the memdb cap')
old.close()
from engine import Engine
from security import MAGIC
with tempfile.TemporaryDirectory() as directory:
    engine=Engine(Path(directory))
    with engine.connect() as db:
        db.execute('CREATE TABLE growth_fixture(data BLOB)')
        db.execute('INSERT INTO growth_fixture VALUES(zeroblob(6291456))')
    with engine.connect() as db:
        db.execute('INSERT INTO growth_fixture VALUES(zeroblob(6291456))')
    assert not engine.db.read_bytes().startswith(b'SQLite format 3')
    reopened=Engine(Path(directory))
    with reopened.connect(readonly=True) as db:
        assert db.execute('SELECT sum(length(data)) FROM growth_fixture').fetchone()[0]==12582912
        assert db.execute('PRAGMA temp_store').fetchone()[0]==2
    version=reopened.storage_version
    try:
        with reopened.connect() as db:
            db.execute('DELETE FROM growth_fixture')
            raise InterruptedError('cancel fixture')
    except InterruptedError:pass
    assert reopened.storage_version==version
    with reopened.connect(readonly=True) as db:
        assert db.execute('SELECT count(*) FROM growth_fixture').fetchone()[0]==2
print('Original SQLITE_FULL reproduced; disk writer has no memdb ceiling; encrypted reopen and rollback passed.')
"""
        result=subprocess.run([sys.executable,'-c',script],cwd=Path(__file__).parent,capture_output=True,text=True,timeout=40)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

if __name__=='__main__':unittest.main()
