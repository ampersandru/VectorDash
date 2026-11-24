"""Generate a deterministic larger synthetic retrieval regression corpus.
Requires Pillow for images; --ffmpeg optionally adds paired forward/reverse silent videos.
Writes only to a new directory; never modifies an existing corpus.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def generate(output, ffmpeg=None):
    from PIL import Image, ImageDraw, ImageFont
    output.mkdir(parents=True, exist_ok=False)
    corpus = output / "corpus"
    corpus.mkdir()
    files, queries = [], []
    def register(path, category):
        files.append({"file": path.name, "category": category, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    def query(text, name, category, group, **extra):
        queries.append({"id": f"Q{len(queries)+1:03}", "query": text, "expected_files": [name], "category": category, "group": group, **extra})
    jobs = [("bicycle", "replace the frayed rear brake cable; keep the front tire"),
            ("garden", "repair the irrigation valve; leave the outdoor lights alone"),
            ("meeting", "move the choir rehearsal to Thursday; leave the concert on Sunday"),
            ("server", "rotate the access logs; preserve the database backup"),
            ("kitchen", "replace the refrigerator seal; keep the compressor")]
    # Twenty-four near-duplicate families, five conflicting tasks each. RTF includes Unicode escapes.
    for family in range(24):
        for task, (topic, instruction) in enumerate(jobs):
            ident = f"CASE-{family:03}-{task}"
            text = f"Maintenance record {ident}\nSite {family:03}\nTopic: {topic}\nAction: {instruction}\n维修记录：更换后刹车内线，不更换前胎。" if task == 0 else f"Maintenance record {ident}\nSite {family:03}\nTopic: {topic}\nAction: {instruction}"
            path = corpus / f"record-{family:03}-{task}.{'rtf' if task == 0 else 'txt'}"
            if task == 0:
                encoded = "".join(f"\\u{int.from_bytes(text.encode('utf-16-le')[i:i+2], 'little', signed=True)}?" for i in range(0, len(text.encode('utf-16-le')), 2))
                path.write_text("{\\rtf1\\ansi\\uc1 " + encoded + "}", encoding="ascii")
            else:
                path.write_text(text, encoding="utf-8")
            register(path, "document")
            query(ident, path.name, "document", "document-identifiers", exact_match=True)
            query(f"Site {family:03}: {instruction}", path.name, "document", "document-conditions")
    try:
        font = ImageFont.truetype("arial.ttf", 64)
    except OSError:
        font = ImageFont.load_default(size=64)
    images = []
    for i in range(24):
        ident = f"VN-{5100+i}"
        image = Image.new("RGB", (960, 540), "white")
        draw = ImageDraw.Draw(image)
        if i % 2:
            draw.ellipse((250,150,650,500),fill="blue")
        else:
            draw.rectangle((250,150,650,500),fill="red")
        draw.text((35,35),ident,fill="black",font=font)
        path = corpus / f"frame-{i:02}.png"
        image.save(path)
        register(path,"image"); images.append(path)
        query(ident,path.name,"image","image-identifiers",exact_match=True)
    seen_video_identifiers = set()
    if ffmpeg:
        for pair in range(6):
            for reverse in (False,True):
                indices=[pair*2,pair*2+1]
                if reverse:indices.reverse()
                path=corpus/f"sequence-{pair}-{int(reverse)}.mp4"
                subprocess.run([ffmpeg,"-nostdin","-v","error","-loop","1","-t","6","-i",str(images[indices[0]]),"-loop","1","-t","6","-i",str(images[indices[1]]),"-filter_complex","[0:v][1:v]concat=n=2:v=1:a=0,format=yuv420p[v]","-map","[v]","-r","10",str(path)],check=True,timeout=90)
                register(path,"video")
                for j, index in enumerate(indices):
                    ident=f"VN-{5100+index}"
                    if ident not in seen_video_identifiers:
                        seen_video_identifiers.add(ident)
                        query(ident,path.name,"video","video-identifiers",exact_match=True,time_ranges=[{"file":path.name,"start_seconds":j*6,"end_seconds":j*6+5.99,"modality":"visual"}])
                        queries[-1]["expected_files"] = [f"sequence-{pair}-0.mp4", f"sequence-{pair}-1.mp4"]
                        queries[-1]["time_ranges"] = [{"file": f"sequence-{pair}-{r}.mp4", "start_seconds": (j if not r else 1-j)*6, "end_seconds": (j if not r else 1-j)*6+5.99, "modality": "visual"} for r in (0,1)]
                shapes=["blue circle" if index%2 else "red square" for index in indices]
                query(f"video labelled VN-{5100+indices[0]} showing a {shapes[0]} then VN-{5100+indices[1]} showing a {shapes[1]}",path.name,"video","video-order",ordered=True,time_ranges=[{"file":path.name,"start_seconds":0,"end_seconds":5.99,"modality":"visual"},{"file":path.name,"start_seconds":6,"end_seconds":11.99,"modality":"visual"}])
    truth={"schema":1,"description":"Synthetic pipeline regression, not a representative quality ranking","files":files,"queries":queries}
    (output/"ground-truth.json").write_text(json.dumps(truth,ensure_ascii=False,indent=2),encoding="utf-8")
    return {"files":len(files),"queries":len(queries)}

if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output",type=Path)
    parser.add_argument("--ffmpeg")
    args=parser.parse_args()
    print(json.dumps(generate(args.output,args.ffmpeg)))
