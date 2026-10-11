"""Read-only native previews for the exact page, moment, image or code location."""
from pathlib import Path
import re
from PIL import Image,ImageOps
import pymupdf
from PySide6.QtCore import Qt,QUrl,QTimer
from PySide6.QtGui import QImage,QPixmap,QPainter,QColor,QTextCursor,QTextCharFormat,QShortcut,QKeySequence
from PySide6.QtWidgets import QApplication,QDialog,QWidget,QVBoxLayout,QHBoxLayout,QLabel,QPushButton,QScrollArea,QSpinBox,QSlider,QPlainTextEdit,QTextEdit,QSizePolicy
from PySide6.QtMultimedia import QMediaPlayer,QAudioOutput
from PySide6.QtMultimediaWidgets import QVideoWidget


def query_words(query):
    stop={'the','a','an','in','on','about','where','they','of','to','and','is','with'}
    return [word for word in re.findall(r'[\w]+(?:[-./:][\w]+)*',query.casefold()) if word not in stop]

def matches(text,query):
    words=query_words(query)
    return any(word in text.casefold() for word in words)

class FitPicture(QLabel):
    def __init__(self):
        super().__init__()
        self.original=QPixmap()
        self.setMinimumSize(1,1)
        self.setSizePolicy(QSizePolicy.Policy.Ignored,QSizePolicy.Policy.Ignored)
    def setPixmap(self,pixmap):
        self.original=pixmap
        self.fit()
    def fit(self):
        if not self.original.isNull():
            super().setPixmap(self.original.scaled(self.size(),Qt.AspectRatioMode.KeepAspectRatio,Qt.TransformationMode.SmoothTransformation))
    def resizeEvent(self,event):
        self.fit()
        super().resizeEvent(event)

class Preview(QDialog):
    def __init__(self,result,parent=None,embedded=False):
        super().__init__(parent)
        self.embedded=embedded
        if embedded:
            self.setWindowFlags(Qt.WindowType.Widget)
        if not embedded:
            self.space_close=QShortcut(QKeySequence(Qt.Key.Key_Space),self)
            self.space_close.setContext(Qt.ShortcutContext.WidgetWithChildrenShortcut)
            self.space_close.activated.connect(self.close)
        self.result=result
        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self.setWindowTitle(result.location()+' — VectorDash')
        self.resize(980,760)
        self.layout=QVBoxLayout(self)
        self.caption=QLabel(result.location())
        self.caption.setTextFormat(Qt.TextFormat.PlainText)
        self.caption.setWordWrap(True)
        self.match_info=QLabel(f'Match score: {result.score:.3f} (ranking score, not a probability)'+'\n'+' · '.join(result.reasons or ['Matched this indexed location']))
        self.match_info.setTextFormat(Qt.TextFormat.PlainText)
        self.match_info.setWordWrap(True)
        self.doc=None
        self.player=None
        self.released=False
        self.finished.connect(self.release)
        QApplication.instance().aboutToQuit.connect(self.release)
        if result.page:
            self.pdf()
        elif result.seconds>=0:
            self.media()
        elif result.kind=='code':
            self.code()
        else:
            from content import IMAGE
            if Path(result.path).suffix.lower() in IMAGE:
                self.image()
            else:
                self.text()

        self.layout.addWidget(self.caption)
        self.layout.addWidget(self.match_info)

    def canvas(self):
        self.scroll=QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.picture=FitPicture() if self.embedded else QLabel()
        self.picture.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.scroll.setWidget(self.picture)
        self.layout.addWidget(self.scroll,1)

    def pdf(self):
        self.doc=pymupdf.open(self.result.path)
        self.canvas()
        controls=QHBoxLayout()
        previous=QPushButton('Previous page')
        following=QPushButton('Next page')
        self.page=QSpinBox()
        self.page.setRange(1,len(self.doc))
        self.page.setValue(self.result.page)
        controls.addWidget(previous); controls.addWidget(self.page); controls.addWidget(QLabel(f'/ {len(self.doc)} pages')); controls.addWidget(following)
        self.layout.addLayout(controls)
        previous.clicked.connect(lambda:self.page.setValue(self.page.value()-1))
        following.clicked.connect(lambda:self.page.setValue(self.page.value()+1))
        self.page.valueChanged.connect(self.render_pdf)
        self.render_pdf()

    def render_pdf(self):
        page=self.doc[self.page.value()-1]
        rectangles=[]
        if self.page.value()==self.result.page:
            for word in query_words(self.result.query):
                rectangles.extend(page.search_for(word))
            if not rectangles:
                # Semantic queries need not share any words with the document.
                for phrase in re.split(r'[.!?\n]',self.result.snippet):
                    if len(phrase.strip())>12:
                        rectangles.extend(page.search_for(phrase.strip()))
            if not rectangles:
                rectangles=[pymupdf.Rect(x,y,x+w,y+h) for word,x,y,w,h in (self.result.boxes or []) if matches(word,self.result.query)]
            if not rectangles and self.result.boxes:
                rectangles=[pymupdf.Rect(x,y,x+w,y+h) for word,x,y,w,h in self.result.boxes if word.casefold() in self.result.snippet.casefold()]
        self.highlight_count=len(rectangles)
        scale=min(1.4,380/max(page.rect.width,1)) if self.embedded else 1.4
        pixmap=page.get_pixmap(matrix=pymupdf.Matrix(scale,scale),alpha=False)
        image=QImage(pixmap.samples,pixmap.width,pixmap.height,pixmap.stride,QImage.Format.Format_RGB888).copy()
        painter=QPainter(image)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QColor(255,215,40,100))
        for rect in rectangles:
            painter.drawRect(int(rect.x0*scale),int(rect.y0*scale),max(1,int(rect.width*scale)),max(1,int(rect.height*scale)))
        painter.end()
        self.picture.setPixmap(QPixmap.fromImage(image))
        if not self.embedded:
            self.picture.setMinimumSize(image.size())
        self.caption.setText(f'Page {self.page.value()} · {len(rectangles)} highlights')

    def image(self):
        self.canvas()
        with Image.open(self.result.path) as source:
            image=ImageOps.exif_transpose(source).convert('RGB')
        original=image.size
        image.thumbnail((380,480) if self.embedded else (1600,1200))
        pixels=image.tobytes()
        display=QImage(pixels,image.width,image.height,image.width*3,QImage.Format.Format_RGB888).copy()
        painter=QPainter(display)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QColor(255,215,40,100))
        for word,x,y,w,h in self.result.boxes or []:
            if matches(word,self.result.query):
                sx,sy=image.width/original[0],image.height/original[1]
                painter.drawRect(int(x*sx),int(y*sy),int(w*sx),int(h*sy))
        painter.end()
        self.picture.setPixmap(QPixmap.fromImage(display))
        if not self.embedded:
            self.picture.setMinimumSize(display.size())

    def media(self):
        self.player=QMediaPlayer(self)
        self.audio=QAudioOutput(self)
        self.player.setAudioOutput(self.audio)
        self.audio.setVolume(.7)
        self.video=QVideoWidget()
        self.player.setVideoOutput(self.video)
        self.layout.addWidget(self.video,1)
        controls=QHBoxLayout()
        play=QPushButton('Pause / play')
        play.clicked.connect(lambda:self.player.pause() if self.player.playbackState()==QMediaPlayer.PlaybackState.PlayingState else self.player.play())
        self.seek=QSlider(Qt.Orientation.Horizontal)
        self.seek.setRange(0,0)
        self.seek.sliderMoved.connect(self.player.setPosition)
        self.player.durationChanged.connect(lambda duration:self.seek.setRange(0,duration))
        self.player.positionChanged.connect(self.position)
        self.player.errorOccurred.connect(lambda error,message:self.caption.setText('Playback error: '+message))
        controls.addWidget(play); controls.addWidget(self.seek,1)
        self.layout.addLayout(controls)
        self.pending_seek=max(0,round(self.result.seconds*1000))
        self.player.mediaStatusChanged.connect(self.loaded)
        self.player.setSource(QUrl.fromLocalFile(self.result.path))
        if not self.embedded:
            self.player.play()

    def loaded(self,status):
        if status in (QMediaPlayer.MediaStatus.LoadedMedia,QMediaPlayer.MediaStatus.BufferedMedia) and self.pending_seek is not None:
            self.player.setPosition(self.pending_seek)
            self.opened_position=self.pending_seek
            self.pending_seek=None

    def position(self,milliseconds):
        if not self.seek.isSliderDown():
            self.seek.setValue(milliseconds)
        seconds=milliseconds//1000
        self.caption.setText(f'{seconds//60}:{seconds%60:02d} · match at {self.result.location()}')

    def code(self):
        viewer=QPlainTextEdit()
        viewer.setReadOnly(True)
        viewer.setLineWrapMode(QPlainTextEdit.LineWrapMode.NoWrap)
        viewer.setPlainText(Path(self.result.path).read_text(encoding='utf-8',errors='replace'))
        viewer.setStyleSheet('font-family: Consolas; font-size: 13px;')
        self.layout.addWidget(viewer,1)
        block=viewer.document().findBlockByLineNumber(max(0,self.result.line-1))
        cursor=QTextCursor(block)
        cursor.movePosition(QTextCursor.MoveOperation.StartOfBlock)
        start=cursor.position()
        endblock=viewer.document().findBlockByLineNumber(max(0,self.result.end_line-1))
        endcursor=QTextCursor(endblock)
        endcursor.movePosition(QTextCursor.MoveOperation.EndOfBlock)
        cursor.setPosition(endcursor.position(),QTextCursor.MoveMode.KeepAnchor)
        selection=QTextEdit.ExtraSelection()
        selection.cursor=cursor
        selection.format.setBackground(QColor(103,87,232,65))
        viewer.setExtraSelections([selection])
        cursor.setPosition(start)
        viewer.setTextCursor(cursor)
        viewer.centerCursor()
        self.code_viewer=viewer

    def text(self):
        from vectordash_core import read_text
        viewer=QPlainTextEdit()
        viewer.setReadOnly(True)
        text=(self.result.snippet if self.result.kind in ('document','spreadsheet','slides','archive','model_metadata') else read_text(Path(self.result.path))) or self.result.snippet or 'No text preview available for this file.'
        viewer.setPlainText(text[:100000])
        selections=[]
        for word in query_words(self.result.query):
            cursor=viewer.document().find(word)
            while not cursor.isNull() and len(selections)<200:
                selection=QTextEdit.ExtraSelection()
                selection.cursor=cursor
                selection.format.setBackground(QColor(255,215,40,100))
                selections.append(selection)
                cursor=viewer.document().find(word,cursor)
        viewer.setExtraSelections(selections)
        self.layout.addWidget(viewer,1)

    def release(self,*args):
        if self.released:
            return
        self.released=True
        if self.player:
            self.player.blockSignals(True)
            self.player.stop()
            self.player.setAudioOutput(None)
            self.player.setVideoOutput(None)
            self.player.setSource(QUrl())
        if self.doc:
            self.doc.close()
            self.doc=None

    def closeEvent(self,event):
        self.release()
        super().closeEvent(event)


class SelectionPreview(QWidget):
    """Shared right-hand preview; replaces and releases the previous selection."""
    def __init__(self,parent=None):
        super().__init__(parent)
        self.setMinimumWidth(260)
        self.layout=QVBoxLayout(self)
        self.layout.setContentsMargins(0,0,0,0)
        self.placeholder=QLabel('Select a result to preview it.')
        self.placeholder.setWordWrap(True)
        self.placeholder.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.layout.addWidget(self.placeholder)
        self.current=None
    def show_result(self,result):
        self.clear()
        if not result:
            return
        from engine import SearchResult
        if not isinstance(result,SearchResult):
            result=SearchResult(0,str(result),'')
        try:
            self.current=Preview(result,self,embedded=True)
            self.layout.addWidget(self.current,1)
            self.current.show()
            self.placeholder.hide()
        except Exception as error:
            for child in self.findChildren(Preview,options=Qt.FindChildOption.FindDirectChildrenOnly):
                child.release()
                child.close()
            if self.current:
                self.current.release()
                self.current.deleteLater()
                self.current=None
            self.placeholder.setText('Preview unavailable: '+str(error))
    def clear(self):
        if self.current:
            self.current.release()
            self.layout.removeWidget(self.current)
            self.current.close()
            self.current=None
        self.placeholder.setText('Select a result to preview it.')
        self.placeholder.show()
