import hashlib
from pathlib import Path
import tempfile
import unittest
import zipfile
from release_gate import authorize_run, verify_assets

class ReleaseGateTests(unittest.TestCase):
    def bundle(self, root, extra=None):
        installer=root/"VectorDash-Rust-Setup-1.2.3-x64.exe";installer.write_bytes(b"MZfixture")
        archive=root/"VectorDash-Rust-1.2.3-x64.zip"
        with zipfile.ZipFile(archive,"w") as z:
            for name in ["VectorDash.exe","pdfium.dll","portable.flag"]:z.writestr(name,b"fixture")
            if extra:z.writestr(extra,b"unwanted")
        (root/"RUST-SHA256SUMS.txt").write_text("".join(hashlib.sha256(p.read_bytes()).hexdigest()+"  "+p.name+"\n" for p in [installer,archive]),encoding="ascii")
    def test_clean_package_and_corrupt_download(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);self.bundle(root);self.assertEqual(len(verify_assets(root,"1.2.3")),3)
            (root/"VectorDash-Rust-Setup-1.2.3-x64.exe").write_bytes(b"MZcorrupt")
            with self.assertRaises(ValueError):verify_assets(root,"1.2.3")
    def test_models_private_data_and_traversal_are_rejected(self):
        for extra in ["settings.json","data/private.txt","models/model.gguf","../escape.txt","index.sqlite","index.key","index.sqlite-wal","index.sqlite-shm","cache.db-wal"]:
            with self.subTest(extra=extra),tempfile.TemporaryDirectory() as folder:
                root=Path(folder);self.bundle(root,extra)
                with self.assertRaises(ValueError):verify_assets(root,"1.2.3")
    def test_failed_stale_fork_and_wrong_workflow_cannot_publish(self):
        run={"status":"completed","conclusion":"success","head_branch":"main","head_sha":"abc","head_repository":{"full_name":"owner/repo"},"path":".github/workflows/native-windows.yml","event":"push"}
        authorize_run(run,"abc","owner/repo")
        for key,value in [("conclusion","failure"),("head_sha","old"),("event","pull_request"),("path","other.yml"),("head_branch","feature"),("head_repository",{"full_name":"fork/repo"})]:
            with self.subTest(key=key),self.assertRaises(ValueError):authorize_run({**run,key:value},"abc","owner/repo")

if __name__=="__main__":unittest.main()
