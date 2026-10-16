"""Publish only checksum-verified artifacts from a successful build of current main."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import urllib.error
import urllib.parse
import urllib.request
import zipfile


def file_digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verify_assets(directory, version):
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-beta\.[1-9]\d*)?", version):
        raise ValueError("Expected x.y.z or x.y.z-beta.N")
    directory = Path(directory)
    installer_name = f"VectorDash-{'Beta' if '-' in version else 'Rust'}-Setup-{version}-x64.exe"
    expected = {installer_name, f"VectorDash-Rust-{version}-x64.zip"}
    manifest = directory / "RUST-SHA256SUMS.txt"
    seen = set()
    for line in manifest.read_text(encoding="ascii").splitlines():
        digest, name = line.split()
        if name not in expected or name in seen or not re.fullmatch(r"[a-f0-9]{64}", digest):
            raise ValueError("Unexpected or duplicate release manifest entry")
        if file_digest(directory / name) != digest:
            raise ValueError(f"Checksum mismatch: {name}")
        seen.add(name)
    if seen != expected:
        raise ValueError("Release manifest is incomplete")
    installer = directory / installer_name
    with installer.open("rb") as stream:
        magic = stream.read(2)
    if magic != b"MZ":
        raise ValueError("Installer is not a Windows executable")
    with zipfile.ZipFile(directory / f"VectorDash-Rust-{version}-x64.zip") as bundle:
        members = bundle.namelist()
        normalized = {name.replace("\\", "/").lower() for name in members}
        if not {"vectordash.exe", "pdfium.dll", "portable.flag"} <= normalized:
            raise ValueError("Portable package is missing required files")
        if len(normalized) != len(members):
            raise ValueError("Duplicate portable package entries")
        for name in normalized:
            path = Path(name)
            if name.startswith("/") or ".." in path.parts or ":" in name:
                raise ValueError("Unsafe package path")
            if name.startswith("data/") or any(path.name.endswith(suffix) for suffix in (".sqlite-wal", ".sqlite-shm", ".db-wal", ".db-shm")) or path.suffix in {".gguf", ".onnx", ".db", ".sqlite", ".part", ".key", ".wav"} or path.name in {"settings.json", "window-state.json"}:
                raise ValueError("Portable package contains models or user data")
    return [directory / name for name in sorted(expected)] + [manifest]


def authorize_run(run, main_sha, repository):
    if not (run.get("status") == "completed" and run.get("conclusion") == "success"
            and run.get("head_branch") == "main" and run.get("head_sha") == main_sha
            and run.get("head_repository", {}).get("full_name") == repository
            and run.get("path") == ".github/workflows/native-windows.yml"
            and run.get("event") in {"push", "workflow_dispatch"}):
        raise ValueError("Release requires a successful build of current main")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--verify-only", action="store_true")
    parser.add_argument("--publish", action="store_true")
    parser.add_argument("--run-id")
    args = parser.parse_args()
    assets = verify_assets(args.directory, args.version)
    if not args.publish:
        print("Release manifest and package verified")
        return
    if "-" in args.version:
        raise ValueError("Official-release publication requires a stable version")
    repository = os.environ["GITHUB_REPOSITORY"]
    token = os.environ["GITHUB_TOKEN"]
    base = "https://api.github.com/repos/" + repository
    headers = {"Authorization": "Bearer " + token, "Accept": "application/vnd.github+json", "Content-Type": "application/json"}

    def api(path, method="GET", data=None):
        request = urllib.request.Request(base + path, data=json.dumps(data).encode() if data is not None else None, headers=headers, method=method)
        with urllib.request.urlopen(request, timeout=60) as response:
            return json.load(response)

    if not args.run_id or not args.run_id.isdigit():
        raise ValueError("A successful build run ID is required")
    run = api("/actions/runs/" + args.run_id)
    main_sha = api("/branches/main")["commit"]["sha"]
    authorize_run(run, main_sha, repository)
    import tomllib
    version = tomllib.loads(Path("native/Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    if version != args.version:
        raise ValueError("Release version differs from tested source")
    # Checkout in the workflow is pinned to the authorized run; recheck before publishing.
    local_sha = __import__("subprocess").check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if local_sha != main_sha:
        raise ValueError("Checkout is not the authorized build commit")
    try:
        ref = api("/git/ref/tags/v" + version)["object"]
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
    else:
        for _ in range(8):
            if ref["type"] != "tag":
                break
            ref = api("/git/tags/" + ref["sha"])["object"]
        if ref["type"] != "commit" or ref["sha"] != main_sha:
            raise ValueError("Existing release tag does not identify the tested commit")
    notes = Path(f"native/docs/{version}.md").read_text(encoding="utf-8")
    release = api("/releases", "POST", {"tag_name": "v" + version, "target_commitish": main_sha, "name": "VectorDash " + version, "body": notes, "draft": True, "prerelease": False})
    upload = release["upload_url"].split("{")[0]
    for asset in assets:
        name = "SHA256SUMS.txt" if asset.name == "RUST-SHA256SUMS.txt" else asset.name
        payload = asset.read_bytes()
        request = urllib.request.Request(upload + "?name=" + urllib.parse.quote(name), data=payload, headers={**headers, "Content-Type": "application/octet-stream"}, method="POST")
        with urllib.request.urlopen(request, timeout=180) as response:
            result = json.load(response)
        expected = "sha256:" + hashlib.sha256(payload).hexdigest()
        if result.get("size") != len(payload) or result.get("digest") != expected:
            raise ValueError("Uploaded asset failed checksum verification")
    uploaded = api("/releases/" + str(release["id"]) + "/assets")
    wanted = {"SHA256SUMS.txt" if p.name == "RUST-SHA256SUMS.txt" else p.name: p for p in assets}
    if len(uploaded) != len(wanted) or {a["name"] for a in uploaded} != set(wanted):
        raise ValueError("Published asset set differs from verified package")
    for asset in uploaded:
        path = wanted[asset["name"]]
        if asset.get("digest") != "sha256:" + file_digest(path) or asset.get("size") != path.stat().st_size:
            raise ValueError("Uploaded release asset changed before publication")
    if api("/branches/main")["commit"]["sha"] != main_sha:
        raise ValueError("Main changed during publication; verified draft retained")
    api("/releases/" + str(release["id"]), "PATCH", {"draft": False, "prerelease": False, "make_latest": "true"})
    print(release["html_url"])


if __name__ == "__main__":
    main()
