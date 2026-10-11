"""Bounded background thumbnails; file icons never query Windows Shell per result."""
from collections import OrderedDict
import queue
import threading
from PySide6.QtCore import QObject,Signal,QSize,Qt
from PySide6.QtGui import QImageReader

class Thumbnails(QObject):
    ready=Signal(object,object)
    def __init__(self,parent=None):
        super().__init__(parent);self.cache=OrderedDict();self.pending=set();self.jobs=queue.Queue()
        self.ready.connect(self.receive)
        for _ in range(2):threading.Thread(target=self.work,daemon=True).start()
    def request(self,key):
        if key in self.cache:return self.cache[key]
        if key not in self.pending:self.pending.add(key);self.jobs.put(key)
        return None
    def receive(self,key,image):
        self.pending.discard(key);self.cache[key]=image
        while len(self.cache)>256:self.cache.popitem(last=False)
    def work(self):
        while True:
            key=self.jobs.get()
            try:
                if len(key)>2 and key[2]=='program':
                    from program_icons import load_icon
                    image=load_icon(key[1])
                    try:self.ready.emit(key,image)
                    except RuntimeError:return
                    continue
                reader=QImageReader(key[1]);reader.setAutoTransform(True)
                size=reader.size()
                if size.isValid() and size.width()*size.height()<=178956970:
                    reader.setScaledSize(size.scaled(QSize(56,56),Qt.AspectRatioMode.KeepAspectRatio));image=reader.read()
                else:image=None
                try:self.ready.emit(key,image)
                except RuntimeError:return
            except Exception:
                try:self.ready.emit(key,None)
                except RuntimeError:return
            finally:self.jobs.task_done()
