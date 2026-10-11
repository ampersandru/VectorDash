"""Modern VectorDash Windows desktop, quick search, setup and scheduling."""
import sys
if '--extract-worker' in sys.argv:
    from parser_worker import worker_main
    raise SystemExit(worker_main())

import logging
import math
import ctypes
from ctypes import wintypes
from datetime import datetime
import hashlib
import socket as net_socket
import json
import os
from pathlib import Path
import subprocess
import process_lifecycle
import sys
import threading
import time
import urllib.request
import psutil
from PySide6.QtCore import Qt, QObject, Signal, QTimer, QTime, QAbstractNativeEventFilter, QFileInfo, QEvent, QRectF, QMimeData, QUrl
from PySide6.QtGui import QIcon, QKeySequence, QShortcut, QCursor, QPainter, QColor, QPen, QPixmap, QLinearGradient
from PySide6.QtNetwork import QLocalServer, QLocalSocket
from PySide6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton, QLineEdit, QListWidget, QListWidgetItem, QStackedWidget, QComboBox, QCheckBox, QSpinBox, QTimeEdit, QFileDialog, QMessageBox, QFrame, QSystemTrayIcon, QMenu, QProgressBar, QScrollArea, QFileIconProvider, QAbstractItemView, QPlainTextEdit, QSplitter)
from engine import Engine, Settings, SearchResult, verify_server_model
from previews import Preview, SelectionPreview
from vectordash_core import command, embed, embedding_request, server_url
from windows_integration import GlobalHotkey, HOTKEY_ID, set_startup, reveal_in_explorer, parse_devices, idle_seconds, gpu_utilization
from scheduling import schedule_due, idle_allowed, next_run
from setup_runtime import catalogs, install_runtime, install_model
from setup_tools import install_ocr, install_ffmpeg
from content import ocr_program, ffmpeg

class Events(QObject):
    status = Signal(str)
    indexed = Signal(str, str, bool, str)
    searched = Signal(int, object, str)
    devices = Signal(object, str)
    catalog = Signal(object, object, str)
    installed = Signal(str, str, str)
    progress = Signal(str, object, object)
    metrics = Signal(object)
    model = Signal(str)
    log = Signal(str, str)
    tools_ready = Signal(object)

def app_icon(active=False, phase=0):
    pixmap=QPixmap(64,64)
    pixmap.fill(Qt.GlobalColor.transparent)
    painter=QPainter(pixmap)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setBrush(QColor('#d83b50' if active else '#6757e8'))
    painter.setPen(Qt.PenStyle.NoPen)
    painter.drawRoundedRect(2,2,60,60,17,17)
    if active:
        painter.translate(32,32)
        painter.rotate(math.sin(phase)*18)
        painter.translate(-32,-32)
    painter.setPen(QPen(QColor('white'),5))
    painter.setBrush(Qt.BrushStyle.NoBrush)
    painter.drawEllipse(17,14,25,25)
    painter.drawLine(38,37,49,49)
    painter.end()
    return QIcon(pixmap)

def search_icon():
    pixmap=QPixmap(24,24)
    pixmap.fill(Qt.GlobalColor.transparent)
    painter=QPainter(pixmap)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setPen(QPen(QColor('#8573ff'),3))
    painter.setBrush(Qt.BrushStyle.NoBrush)
    painter.drawEllipse(3,3,12,12)
    painter.drawLine(14,14,21,21)
    painter.end()
    return QIcon(pixmap)

def label(text, style=''):
    result=QLabel(text)
    result.setTextFormat(Qt.TextFormat.PlainText)
    result.setWordWrap(True)
    if style:
        result.setObjectName(style)
    return result

def button(text, callback, primary=False):
    result=QPushButton(text)
    result.setCursor(Qt.CursorShape.PointingHandCursor)
    result.clicked.connect(callback)
    if primary:
        result.setObjectName('primary')
    return result

def row(*widgets):
    frame=QWidget()
    layout=QHBoxLayout(frame)
    layout.setContentsMargins(0,0,0,0)
    for widget in widgets:
        layout.addWidget(widget)
    return frame

def card(title, subtitle=''):
    frame=QFrame()
    frame.setObjectName('card')
    layout=QVBoxLayout(frame)
    layout.setContentsMargins(22,18,22,18)
    layout.setSpacing(12)
    layout.addWidget(label(title,'cardTitle'))
    if subtitle:
        layout.addWidget(label(subtitle,'muted'))
    return frame,layout

def scroll_settings(widget,event):
    event.ignore()
    parent=widget.parentWidget()
    while parent is not None and not isinstance(parent,QScrollArea):
        parent=parent.parentWidget()
    if parent is not None:
        QApplication.sendEvent(parent.viewport(),event)

class ScrollSafeSpinBox(QSpinBox):
    def wheelEvent(self,event):
        scroll_settings(self,event)

class ScrollSafeTimeEdit(QTimeEdit):
    def wheelEvent(self,event):
        scroll_settings(self,event)

class ScrollSafeComboBox(QComboBox):
    def wheelEvent(self,event):
        scroll_settings(self,event)

class SearchEdit(QLineEdit):
    def __init__(self, results):
        super().__init__()
        self.results=results
        self.setPlaceholderText('Describe a file, or search for exact words…')
        self.setObjectName('searchInput')
    def keyPressEvent(self,event):
        if event.key()==Qt.Key.Key_Down and self.results.count():
            self.results.setCurrentRow(0)
            self.results.setFocus()
        else:
            super().keyPressEvent(event)

class Results(QListWidget):
    def __init__(self, owner):
        super().__init__()
        self.owner=owner
        self.setObjectName('results')
        self.setSpacing(6)
        self.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        self.icons=QFileIconProvider()
        self.itemActivated.connect(lambda item:owner.open_file(item.data(Qt.ItemDataRole.UserRole)))
    def keyPressEvent(self,event):
        if event.key()==Qt.Key.Key_Space and event.modifiers()==Qt.KeyboardModifier.NoModifier:
            self.owner.preview_result(self.selected_result())
            event.accept()
        elif event.matches(QKeySequence.StandardKey.Copy):
            self.owner.copy_file(self.selected_path())
            event.accept()
        else:
            super().keyPressEvent(event)

    def contextMenuEvent(self,event):
        item=self.itemAt(event.pos())
        if not item:return
        self.setCurrentItem(item)
        path=self.selected_path()
        quick=self.owner.quick
        active=quick.isVisible()
        quick.context_menu_open=True
        try:
            menu=QMenu(self)
            exclude=menu.addAction('Exclude file from future searches')
            chosen=menu.exec(event.globalPos())
            if chosen==exclude:self.owner.exclude_search_file(path)
        finally:
            quick.context_menu_open=False
            if active and not self.owner.shutting_down:
                quick.show();quick.raise_();quick.activateWindow();quick.results.setFocus()

    def populate(self, results):
        self.clear()
        for result in results:
            score,path,snippet = result
            item=QListWidgetItem()
            item.setData(Qt.ItemDataRole.UserRole,result if isinstance(result,SearchResult) else path)
            item.setToolTip(path)
            widget=QWidget()
            layout=QHBoxLayout(widget)
            layout.setContentsMargins(12,10,12,10)
            icon=QLabel()
            icon.setPixmap(self.icons.icon(QFileInfo(path)).pixmap(32,32))
            layout.addWidget(icon)
            content=QVBoxLayout()
            content.setSpacing(3)
            content.addWidget(label(Path(path).name,'resultTitle'))
            content.addWidget(label((result.location()+' · ' if isinstance(result,SearchResult) else '')+path,'muted'))
            if snippet:
                content.addWidget(label(snippet,'snippet'))
            layout.addLayout(content,1)
            item.setSizeHint(widget.sizeHint())
            self.addItem(item)
            self.setItemWidget(item,widget)
        if self.count():
            self.setCurrentRow(0)
    def selected_path(self):
        item=self.currentItem()
        value=item.data(Qt.ItemDataRole.UserRole) if item else None
        return value.path if isinstance(value,SearchResult) else value
    def selected_result(self):
        item=self.currentItem()
        return item.data(Qt.ItemDataRole.UserRole) if item else None

class HotkeyFilter(QAbstractNativeEventFilter):
    def __init__(self, callback):
        super().__init__()
        self.callback=callback
    def nativeEventFilter(self,event_type,message):
        if os.name=='nt':
            msg=wintypes.MSG.from_address(int(message))
            if msg.message==0x0312 and msg.wParam==HOTKEY_ID:
                self.callback()
                return True,0
        return False,0

class QuickSearch(QWidget):
    def __init__(self, owner):
        super().__init__(None,Qt.WindowType.Tool | Qt.WindowType.FramelessWindowHint)
        self.owner=owner
        self.preview_open=False
        self.context_menu_open=False
        self.setWindowTitle('VectorDash Quick Search')
        self.setWindowIcon(owner.windowIcon())
        self.setObjectName('quickSearch')
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAutoFillBackground(False)
        self.resize(1060,570)
        layout=QVBoxLayout(self)
        layout.setContentsMargins(24,22,24,20)
        layout.addWidget(row(label('VectorDash','cardTitle'),button('Open full app',self.full)))
        self.results=Results(owner)
        self.input=SearchEdit(self.results)
        self.input.addAction(search_icon(),QLineEdit.ActionPosition.LeadingPosition)
        layout.addWidget(self.input)
        self.filter_category,self.filter_excludes=owner.filter_controls(layout)
        self.preview=SelectionPreview(self)
        split=QSplitter(Qt.Orientation.Horizontal)
        split.addWidget(self.results)
        split.addWidget(self.preview)
        split.setSizes([600,420])
        layout.addWidget(split,1)
        self.results.currentItemChanged.connect(lambda current,previous:self.preview.show_result(self.results.selected_result()))
        self.hint=label('↑ ↓ select     Space preview     Ctrl+C copy file     Enter open     Esc dismiss','muted')
        layout.addWidget(self.hint)
        self.timer=QTimer(self)
        self.timer.setSingleShot(True)
        self.timer.setInterval(120)
        self.timer.timeout.connect(lambda:owner.request_search(self.input.text()))
        self.input.textChanged.connect(lambda:self.changed())
        self.input.returnPressed.connect(lambda:owner.open_file(self.results.selected_result()) if self.results.count() else owner.request_search(self.input.text()))
        QShortcut(QKeySequence('Escape'),self,activated=self.hide)
    def paintEvent(self,event):
        painter=QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        dark=self.owner.settings['theme']=='Dark' or (self.owner.settings['theme']=='System' and QApplication.styleHints().colorScheme()==Qt.ColorScheme.Dark)
        rect=QRectF(self.rect()).adjusted(.5,.5,-.5,-.5)
        painter.setPen(Qt.PenStyle.NoPen)
        for spread in range(10,0,-1):
            painter.setBrush(QColor(10,4,18,2+(10-spread)//2))
            painter.drawRoundedRect(rect.adjusted(-spread,-spread/2,spread,spread),16+spread,16+spread)
        gradient=QLinearGradient(rect.topLeft(),rect.bottomRight())
        gradient.setColorAt(0,QColor(42,27,53,191) if dark else QColor(255,252,255,207))
        gradient.setColorAt(1,QColor(46,28,61,198) if dark else QColor(239,231,252,211))
        painter.setBrush(gradient)
        painter.setPen(QPen(QColor(138,115,168,31),1))
        painter.drawRoundedRect(rect,16,16)

    def showEvent(self,event):
        super().showEvent(event)
        self.refresh_material()

    def refresh_material(self):
        from windows_integration import apply_frosted_backdrop
        dark=self.owner.settings['theme']=='Dark' or (self.owner.settings['theme']=='System' and QApplication.styleHints().colorScheme()==Qt.ColorScheme.Dark)
        self.backdrop_enabled=apply_frosted_backdrop(int(self.winId()),dark)
        self.update()

    def hideEvent(self,event):
        self.preview.clear()
        super().hideEvent(event)

    def changed(self):
        self.results.clear()
        self.owner.search_sequence+=1
        self.timer.start()

    def full(self):
        self.owner.query.setText(self.input.text())
        self.hide()
        self.owner.show_main()
    def summon(self):
        screen=QApplication.screenAt(QCursor.pos()) or QApplication.primaryScreen()
        area=screen.availableGeometry()
        self.resize(min(1060,area.width()-40),min(570,area.height()-80))
        self.move(area.center().x()-self.width()//2,area.top()+max(40,int(area.height()*.22)))
        self.show()
        self.raise_()
        self.activateWindow()
        self.input.setFocus()
        self.input.selectAll()
        self.owner.request_search(self.input.text())
    def event(self,event):
        if event.type()==QEvent.Type.WindowDeactivate and self.isVisible() and not self.preview_open and not self.context_menu_open:
            self.hide()
        return super().event(event)

class UiLogHandler(logging.Handler):
    def __init__(self, events):
        super().__init__()
        self.events=events
    def emit(self, record):
        self.events.log.emit(record.levelname,self.format(record))

class Desktop(QMainWindow):
    def __init__(self, settings, ipc=None):
        super().__init__()
        self.store=settings
        process_lifecycle.reopen()
        self.settings=settings.values
        self.settings['semantic']=True
        self.model_generation=0
        self.model_loaded=False
        self.loaded_modalities={}
        self.tool_ready={}
        self.engine=Engine(settings.home,settings.values)
        self.mcp=None
        self.events=Events()
        self.indexing=False
        self.automatic_active=False
        self.installing=False
        self.collecting=False
        self.metrics={'cpu':None,'gpu':None,'idle':None,'on_ac':None}
        self.metrics_time=0
        self.schedule_retry=0
        self.attached_server=None
        self.model_process=None
        self.model_log=None
        self.download_cancel=threading.Event()
        self.search_sequence=0
        self.pending_search=None
        self.search_running=False
        self.log_records=[]
        existing_log=self.store.home/'vectordash.log'
        if existing_log.exists():
            for line in existing_log.read_text(encoding='utf-8',errors='replace')[-100000:].splitlines():
                level=next((name for name in ('DEBUG','INFO','WARNING','ERROR','CRITICAL') if ' '+name+' ' in line),'INFO')
                self.log_records.append((level,line))
            self.log_records=self.log_records[-2000:]
        self.probing_model=False
        self.icon_phase=0
        self.widgets={}
        self.runtimes=[]
        self.models=[]
        self.shutting_down=False
        self.setWindowTitle('VectorDash')
        self.setWindowIcon(app_icon())
        self.resize(1120,820)
        self.setMinimumSize(850,620)
        self.events.tools_ready.connect(self.tools_status_finished)
        self.events.log.connect(self.append_log)
        self.logger=logging.getLogger('vectordash')
        self.logger.setLevel(logging.DEBUG)
        self.log_handler=UiLogHandler(self.events)
        self.log_file_handler=logging.FileHandler(self.store.home/'vectordash.log',encoding='utf-8')
        self.log_file_handler.setFormatter(logging.Formatter('%(asctime)s %(levelname)s %(message)s'))
        self.logger.addHandler(self.log_handler)
        self.logger.addHandler(self.log_file_handler)
        self.events.status.connect(self.set_status)
        self.events.indexed.connect(self.index_finished)
        self.events.searched.connect(self.search_finished)
        self.events.devices.connect(self.devices_finished)
        self.events.catalog.connect(self.catalog_finished)
        self.events.installed.connect(self.install_finished)
        self.events.progress.connect(self.download_progress)
        self.events.metrics.connect(self.metrics_finished)
        self.events.model.connect(self.model_status)
        self.build()
        self.launch(self.warm_search_cache)
        self.quick=QuickSearch(self)
        self.hotkey=GlobalHotkey()
        self.hotkey_filter=HotkeyFilter(self.toggle_quick)
        QApplication.instance().installNativeEventFilter(self.hotkey_filter)
        self.tray=QSystemTrayIcon(self.windowIcon(),self)
        menu=QMenu()
        menu.addAction('Quick search',self.toggle_quick)
        menu.addAction('Open VectorDash',lambda:self.show_main())
        menu.addSeparator()
        menu.addAction('Pause / resume indexing',self.pause_index)
        menu.addAction('Update index',lambda:self.start_index())
        menu.addAction('Settings',lambda:self.show_main(4))
        menu.addSeparator()
        menu.addAction('Exit VectorDash',self.exit_app)
        self.tray.setContextMenu(menu)
        self.tray.setToolTip('VectorDash • local file search')
        self.tray.activated.connect(lambda reason:self.toggle_quick() if reason==QSystemTrayIcon.ActivationReason.Trigger else None)
        self.tray.show()
        if ipc:
            ipc.newConnection.connect(lambda:self.accept_ipc(ipc))
        self.apply_theme()
        try:
            self.hotkey.register(self.settings['hotkey'])
            if self.settings['startup']:
                set_startup(True)
        except (OSError,ValueError) as error:
            self.set_status('Quick-search shortcut unavailable. You can still use VectorDash; choose another shortcut in Settings.')
        self.update_hotkey_status()
        self.clock=QTimer(self)
        self.clock.setInterval(15000)
        self.clock.timeout.connect(self.tick)
        self.clock.start()
        self.process_timer=QTimer(self)
        self.process_timer.setInterval(2000)
        self.process_timer.timeout.connect(self.check_model_exit)
        self.process_timer.start()
        self.icon_timer=QTimer(self)
        self.icon_timer.setInterval(120)
        self.icon_timer.timeout.connect(self.animate_index_icon)
        self.icon_timer.start()
        QApplication.instance().aboutToQuit.connect(self.cleanup)
        QTimer.singleShot(1000,self.refresh_component_status)
        QTimer.singleShot(1200,self.apply_mcp)
        QApplication.styleHints().colorSchemeChanged.connect(lambda:self.apply_theme() if self.settings['theme']=='System' else None)
        QTimer.singleShot(500,self.tick)
        if self.settings['auto_model'] and (self.settings.get('server_mode')=='Remote' or all(Path(self.settings.get(k,'')).is_file() for k in ('server','model'))):
            QTimer.singleShot(800,self.start_model)

    def launch(self, work):
        threading.Thread(target=work,daemon=True).start()

    def warm_search_cache(self):
        try:
            self.engine.cached_index('files')
            if self.settings.get('code_enabled'):
                self.engine.cached_index('code')
        except Exception as error:
            self.logger.debug('Search cache warm-up: %s',error)

    def build(self):
        central=QWidget()
        root=QHBoxLayout(central)
        root.setContentsMargins(0,0,0,0)
        root.setSpacing(0)
        sidebar=QFrame()
        sidebar.setObjectName('sidebar')
        sidebar.setFixedWidth(205)
        nav=QVBoxLayout(sidebar)
        nav.setContentsMargins(20,30,16,18)
        nav.setSpacing(12)
        nav.addWidget(label('VectorDash','brand'))
        nav.addWidget(label('Find what you remember.','muted'))
        nav.addSpacing(28)
        self.navigation=QListWidget()
        self.navigation.setObjectName('navigation')
        self.navigation.addItems(['Search','Sources','Models & hardware','Scheduling','Settings','Logs'])
        self.navigation.setSpacing(6)
        nav.addWidget(self.navigation,1)
        nav.addWidget(label('Everything runs locally\nYour files and data stay on your computer','muted'))
        root.addWidget(sidebar)
        body=QWidget()
        layout=QVBoxLayout(body)
        layout.setContentsMargins(28,26,28,16)
        layout.setSpacing(18)
        self.stack=QStackedWidget()
        layout.addWidget(self.stack,1)
        self.status=label('Ready','muted')
        layout.addWidget(self.status)
        root.addWidget(body,1)
        self.setCentralWidget(central)
        self.navigation.currentRowChanged.connect(self.stack.setCurrentIndex)
        self.search_page()
        self.sources_page()
        self.models_page()
        self.schedule_page()
        self.settings_page()
        self.logs_page()
        self.navigation.setCurrentRow(0)

    def page(self,title,description):
        page=QWidget()
        layout=QVBoxLayout(page)
        layout.setContentsMargins(0,0,0,0)
        layout.setSpacing(18)
        layout.addWidget(label(title,'heading'))
        layout.addWidget(label(description,'muted'))
        scroll=QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        content=QWidget()
        inner=QVBoxLayout(content)
        inner.setContentsMargins(0,0,8,0)
        inner.setSpacing(18)
        scroll.setWidget(content)
        layout.addWidget(scroll,1)
        self.stack.addWidget(page)
        return inner

    def field(self,layout,key,title,kind='text',options=None,minimum=0,maximum=100):
        if kind!='check':
            layout.addWidget(label(title))
        value=self.settings.get(key)
        if kind=='combo':
            widget=ScrollSafeComboBox()
            widget.addItems(options)
            widget.setCurrentText(value)
        elif kind=='check':
            widget=QCheckBox(title)
            widget.setChecked(bool(value))
        elif kind=='spin':
            widget=ScrollSafeSpinBox()
            widget.setRange(minimum,maximum)
            widget.setValue(value)
        elif kind=='time':
            widget=ScrollSafeTimeEdit(QTime.fromString(value,'HH:mm'))
            widget.setDisplayFormat('hh:mm AP')
        else:
            widget=QLineEdit(str(value or ''))
        layout.addWidget(widget)
        self.widgets[key]=widget
        return widget

    def search_page(self):
        page=QWidget()
        layout=QVBoxLayout(page)
        layout.setContentsMargins(0,0,0,0)
        layout.setSpacing(16)
        layout.addWidget(label('What are you looking for?','heading'))
        self.shortcut_hint=label(f'Describe a file, or press {self.settings["hotkey"]} from any app.','muted')
        layout.addWidget(self.shortcut_hint)
        self.results=Results(self)
        self.query=SearchEdit(self.results)
        layout.addWidget(self.query)
        self.filter_category,self.filter_excludes=self.filter_controls(layout)
        self.search_hint=label('Type to search your indexed files.','muted')
        layout.addWidget(self.search_hint)
        self.result_preview=SelectionPreview(self)
        split=QSplitter(Qt.Orientation.Horizontal)
        split.addWidget(self.results)
        split.addWidget(self.result_preview)
        split.setSizes([560,400])
        layout.addWidget(split,1)
        self.results.currentItemChanged.connect(lambda current,previous:self.result_preview.show_result(self.results.selected_result()))
        layout.addWidget(row(button('Open',lambda:self.open_file(self.results.selected_result()),True),button('Show in Explorer',self.reveal),button('Copy path',self.copy_path)))
        self.stack.addWidget(page)
        self.search_timer=QTimer(self)
        self.search_timer.setSingleShot(True)
        self.search_timer.setInterval(120)
        self.search_timer.timeout.connect(lambda:self.request_search(self.query.text()))
        self.query.textChanged.connect(self.query_changed)
        self.query.returnPressed.connect(lambda:self.open_file(self.results.selected_result()) if self.results.count() else self.request_search(self.query.text()))

    def filter_controls(self,layout):
        category=QComboBox()
        category.addItems(['All files','Photos / screenshots','Documents','Audio / video','Code','Archives / models'])
        category.setCurrentText(self.settings.get('search_category','All files'))
        excludes=QLineEdit(self.settings.get('search_excluded_types',''))
        excludes.setPlaceholderText('Exclude types: .zip, .mp4, .log')
        layout.addWidget(row(category,excludes))
        category.currentTextChanged.connect(lambda:self.filters_changed(category,excludes))
        excludes.textChanged.connect(lambda:self.filters_changed(category,excludes))
        return category,excludes

    def filters_changed(self,category,excludes):
        self.settings['search_category']=category.currentText()
        self.settings['search_excluded_types']=excludes.text().strip()
        for view in (self,getattr(self,'quick',None)):
            if not view or not hasattr(view,'filter_category') or view.filter_category is category:continue
            view.filter_category.blockSignals(True);view.filter_excludes.blockSignals(True)
            view.filter_category.setCurrentText(category.currentText());view.filter_excludes.setText(excludes.text())
            view.filter_category.blockSignals(False);view.filter_excludes.blockSignals(False)
        query=self.quick.input.text() if hasattr(self,'quick') and self.quick.isVisible() else self.query.text()
        self.store.values=self.settings;self.store.save()
        self.request_search(query)

    def sources_page(self):
        layout=self.page('Your sources','Choose the drives and folders VectorDash can read.')
        frame,inside=card('Index status','Pause keeps your progress. Cancel keeps the previous completed index.')
        self.index_count=label(f'{self.engine.indexed_file_count():,} indexed files','cardTitle')
        inside.addWidget(self.index_count)
        self.source_status=label('Ready','muted')
        self.source_progress=QProgressBar()
        self.source_progress.setValue(0)
        inside.addWidget(self.source_status)
        inside.addWidget(self.source_progress)
        inside.addWidget(row(button('Pause / resume',self.pause_index),button('Cancel indexing',self.engine.cancel.set)))
        layout.addWidget(frame)
        frame,inside=card('Drives and folders','Hidden/system folders, cloud placeholders and common secrets are skipped.')
        self.sources=QListWidget()
        self.sources.setMinimumHeight(180)
        self.sources.addItems(self.settings['roots'])
        inside.addWidget(self.sources)
        self.drives=ScrollSafeComboBox()
        if os.name=='nt':
            mask=ctypes.windll.kernel32.GetLogicalDrives()
            self.drives.addItems([f'{chr(65+i)}:\\' for i in range(26) if mask & (1<<i)])
        inside.addWidget(row(button('Add folder…',self.add_folder,True),self.drives,button('Add drive',lambda:self.add_source(self.drives.currentText())),button('Remove',self.remove_source)))
        layout.addWidget(frame)
        frame,inside=card('Search file contents','Photos, audio and video require semantic search and the matching vision + audio encoder. PDFs retain their page numbers.')
        for key,title in [('index_images','Photos and screenshots'),('index_audio','Audio and video soundtracks'),('index_video','Video frames')]:
            self.field(inside,key,title,'check')
        self.field(inside,'visual_detail','Visual indexing detail','combo',['Balanced','High detail'])
        self.field(inside,'pdf_visuals','Index PDF page pictures and layouts alongside readable text','check')
        inside.addWidget(label('High detail uses more memory and takes longer. Save these settings, restart the local model, then update the index. Remote servers control their own vision budget.','muted'))
        self.field(inside,'video_interval','Seconds between sampled video frames','spin',minimum=1,maximum=30)
        inside.addWidget(label('Scene changes are also sampled. A brief event between frames can be missed; use 1 second for finer coverage. Audio uses overlapping 30-second windows.','muted'))
        inside.addWidget(label('OCR, decoder and encoder setup are in Models & hardware.','muted'))
        layout.addWidget(frame)
        frame,inside=card('Code search','Opt in, choose project folders, then prefix your query with code:. Git ignored files are excluded.')
        self.field(inside,'code_enabled','Enable code search','check')
        self.code_sources=QListWidget()
        self.code_sources.addItems(self.settings.get('code_roots',[]))
        self.code_sources.setMinimumHeight(100)
        inside.addWidget(self.code_sources)
        inside.addWidget(row(button('Add project…',self.add_project),button('Remove project',lambda:self.code_sources.takeItem(self.code_sources.currentRow()))))
        inside.addWidget(button('Save content settings',self.save,True))
        layout.addWidget(frame)
        frame,inside=card('Indexing','Updates reuse unchanged files. A full rebuild recalculates every vector.')
        inside.addWidget(row(button('Update index',lambda:self.start_index(),True),button('Full rebuild',lambda:self.start_index(force=True)),button('Pause / resume',self.pause_index),button('Stop',self.engine.cancel.set)))
        self.index_status=label('No indexing task running.','muted')
        inside.addWidget(self.index_status)
        layout.addWidget(frame)
        layout.addStretch()

    def add_project(self):
        path=QFileDialog.getExistingDirectory(self,'Choose a project folder')
        if path and path not in [self.code_sources.item(i).text() for i in range(self.code_sources.count())]:
            self.code_sources.addItem(path)

    def models_page(self):
        layout=self.page('Models & hardware','Choose the features you want, then start your local or remote model.')
        frame,inside=card('Status','Downloads and installs the selected runtime, model, encoder, OCR language packs and media decoder. Choose hardware and quantization below first.')
        self.setup_progress=QProgressBar()
        self.setup_progress.setValue(0)
        self.setup_progress.hide()
        inside.addWidget(self.setup_progress)
        self.setup_choices={}
        self.component_status={}
        components=[('server','Local model runtime',True),('model','Text search model',True),('projector','Photos, audio and video encoder',True),('ocr','Exact text in screenshots and scanned PDFs',self.settings.get('ocr_enabled',True)),('ffmpeg','Audio and video decoder',True)]
        saved=self.settings.get('setup_components',{})
        for key,title,default in components:
            choice=QCheckBox(title)
            choice.setChecked(saved.get(key,default))
            status=label('Checking…')
            status.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
            self.setup_choices[key]=choice
            self.component_status[key]=status
            inside.addWidget(row(choice,status))
            choice.toggled.connect(self.update_tool_state)
        self.setup_button=button('Download, install and start',self.setup_all,True)
        self.setup_stop=button('Stop',self.stop_model)
        self.setup_cancel=button('Cancel setup',self.download_cancel.set)
        self.setup_cancel.hide()
        inside.addWidget(row(self.setup_button,self.setup_stop,button('Refresh status',self.refresh_component_status),self.setup_cancel))
        layout.addWidget(frame)
        frame,inside=card('Server connection','Remote mode sends indexed content and search queries to your chosen server.')
        self.field(inside,'server_mode','Embedding server','combo',['Local','Remote'])
        self.field(inside,'remote_url','Remote server URL, e.g. http://192.168.1.10:8787')
        self.field(inside,'remote_model_id','Remote model identifier (as reported by /props)')
        self.field(inside,'remote_key','Remote API key (optional)')
        self.widgets['remote_key'].setEchoMode(QLineEdit.EchoMode.Password)
        inside.addWidget(row(button('Save connection',self.save),button('Connect / check status',self.start_model)))
        layout.addWidget(frame)
        frame,inside=card('Local inference','Keep your existing runtime and model, or download them below.')
        self.field(inside,'mode','Run on','combo',['CPU / system RAM','Dedicated GPU','Integrated GPU'])
        self.devices=ScrollSafeComboBox()
        self.devices.addItem(self.settings['device'] or 'Select an adapter',self.settings['device'])
        inside.addWidget(label('Inference device'))
        inside.addWidget(row(self.devices,button('Refresh devices',self.refresh_devices)))
        for key,title in [('server','llama-server executable'),('model','Embedding model (GGUF)'),('projector','Vision + audio encoder (matching mmproj GGUF)')]:
            self.field(inside,key,title)
            inside.addWidget(button('Browse…',lambda checked=False,k=key:self.browse(k)))
        self.field(inside,'auto_model','Start model automatically when VectorDash starts','check')
        inside.addWidget(row(button('Save hardware settings',self.save,True),button('Start model',self.start_model),button('Stop model',self.stop_model),button('Test embedding',self.test_model)))
        self.model_state=label('Model readiness is checked when starting or testing.','muted')
        inside.addWidget(self.model_state)
        layout.addWidget(frame)
        frame,inside=card('Download and set up','Runtime: ggml-org/llama.cpp · Model: ggml-org/embeddinggemma-2-GGUF')
        inside.addWidget(button('Load available downloads',self.load_catalog))
        self.runtime_choice=ScrollSafeComboBox()
        self.quant_choice=ScrollSafeComboBox()
        self.quant_choice.addItem('Q8_0 (recommended)','Q8_0')
        self.quant_choice.addItem('BF16','BF16')
        if 'BF16' in Path(self.settings.get('model','')).name.upper():
            self.quant_choice.setCurrentIndex(1)
        inside.addWidget(label('Windows runtime'))
        inside.addWidget(self.runtime_choice)
        inside.addWidget(label('EmbeddingGemma 2 quantization'))
        inside.addWidget(self.quant_choice)
        self.download_summary=label('Choose a quantization and click download; catalogs load automatically. Load available downloads to choose a runtime build.','muted')
        inside.addWidget(self.download_summary)
        self.runtime_choice.currentIndexChanged.connect(self.catalog_summary)
        self.quant_choice.currentIndexChanged.connect(self.catalog_summary)
        self.quant_choice.currentIndexChanged.connect(self.update_tool_state)
        self.download_bar=QProgressBar()
        self.download_bar.setValue(0)
        self.download_bar.hide()
        inside.addWidget(self.download_bar)
        inside.addWidget(row(button('Install runtime',lambda:self.install('server'),True),button('Download model + encoder',lambda:self.install('model'),True),button('Encoder only',lambda:self.install('projector')),button('Cancel download',self.download_cancel.set)))
        inside.addWidget(label('Downloads are resumable. Models are checked against SHA-256. Runtime checksums are verified when supplied by GitHub. Existing paths remain available.','muted'))
        layout.addWidget(frame)
        frame,inside=card('OCR and media tools','Optional OCR provides exact screenshot text. FFmpeg decodes audio/video and is included with VectorDash.')
        self.field(inside,'ocr_enabled','Extract exact text from screenshots and scanned PDFs','check')
        self.field(inside,'ocr_path','Tesseract executable (blank = detect installed copy)')
        inside.addWidget(button('Browse OCR executable…',lambda:self.browse('ocr_path')))
        self.field(inside,'ocr_languages','OCR language packs, e.g. eng+ben+ara')
        self.field(inside,'ocr_data','OCR language data folder (set automatically during setup)')
        inside.addWidget(button('Download and set up OCR + languages',lambda:self.install_tool('ocr'),True))
        self.field(inside,'ffmpeg_path','FFmpeg executable (blank = installed or included copy)')
        inside.addWidget(row(button('Browse FFmpeg…',lambda:self.browse('ffmpeg_path')),button('One-click FFmpeg setup',lambda:self.install_tool('ffmpeg'),True)))
        inside.addWidget(row(button('Save tool settings',self.save),button('Cancel remaining downloads',self.download_cancel.set)))
        self.tool_state=label('','muted')
        inside.addWidget(self.tool_state)
        inside.addWidget(label('The model GGUF handles text. Its mmproj file supplies EmbeddingGemma’s own image/audio encoders. Tesseract is optional for exact OCR; FFmpeg is only needed for audio/video.','muted'))
        layout.addWidget(frame)
        self.update_tool_state()
        layout.addStretch()

    def schedule_page(self):
        layout=self.page('Scheduling','Keep your index current while VectorDash runs in the background.')
        frame,inside=card('When to update','Scheduled updates reuse unchanged files and never overlap.')
        self.field(inside,'schedule_mode','Schedule','combo',['Manual','At selected time','When idle','Time + idle'])
        self.field(inside,'schedule_time','Time (this PC’s local time)','time')
        dayrow=QWidget()
        days=QHBoxLayout(dayrow)
        days.setContentsMargins(0,0,0,0)
        self.day_checks=[]
        for i,name in enumerate(['Mon','Tue','Wed','Thu','Fri','Sat','Sun']):
            check=QCheckBox(name)
            check.setChecked(i in self.settings['schedule_days'])
            self.day_checks.append(check)
            days.addWidget(check)
        inside.addWidget(dayrow)
        self.field(inside,'idle_interval','Minimum minutes between idle updates','spin',minimum=5,maximum=1440)
        inside.addWidget(button('Save schedule',self.save,True))
        layout.addWidget(frame)
        frame,inside=card('Idle and power conditions','Idle conditions apply to When idle and Time + idle. AC power applies to every automatic run.')
        self.field(inside,'check_user_idle','Require keyboard and mouse inactivity','check')
        self.field(inside,'idle_minutes','Keyboard/mouse inactive for minutes','spin',minimum=1,maximum=120)
        self.field(inside,'check_cpu','Require low CPU usage','check')
        self.field(inside,'cpu_limit','CPU usage below (%)','spin',minimum=1,maximum=100)
        self.field(inside,'check_gpu','Require low GPU usage (all monitored GPU engines)','check')
        self.field(inside,'gpu_limit','GPU usage below (%)','spin',minimum=1,maximum=100)
        self.field(inside,'ac_only','Automatic indexing only on AC power','check')
        self.field(inside,'max_run_minutes','Maximum automatic run duration (minutes)','spin',minimum=5,maximum=1440)
        inside.addWidget(label('GPU monitoring depends on Windows performance counters. If unavailable, a GPU-conditioned schedule waits and explains why. Idle jobs pause between chunks when activity resumes.','muted'))
        inside.addWidget(button('Save conditions',self.save,True))
        layout.addWidget(frame)
        frame,inside=card('Schedule status')
        self.schedule_status=label('Checking conditions…','muted')
        self.telemetry=label('Collecting usage…','muted')
        self.next_status=label('Next run: manual only','muted')
        self.last_status=label('Last successful update: '+(self.settings['last_success'] or 'Never'),'muted')
        inside.addWidget(self.schedule_status)
        inside.addWidget(self.telemetry)
        inside.addWidget(self.next_status)
        inside.addWidget(self.last_status)
        self.history=QListWidget()
        self.history.setMinimumHeight(100)
        self.history.addItems(self.settings.get('schedule_history',[]))
        inside.addWidget(self.history)
        inside.addWidget(button('Update now',lambda:self.start_index()))
        inside.addWidget(label('Missed runs are handled once after resume. VectorDash must be running; it does not wake a sleeping PC.','muted'))
        layout.addWidget(frame)
        layout.addStretch()

    def settings_page(self):
        layout=self.page('Settings','Make VectorDash fit the way you use Windows.')
        frame,inside=card('Appearance and shortcut')
        self.field(inside,'theme','Appearance','combo',['System','Light','Dark'])
        self.field(inside,'hotkey','Global quick-search shortcut')
        self.shortcut_state=label('','muted')
        inside.addWidget(self.shortcut_state)
        inside.addWidget(label('Use Ctrl, Alt, Shift or Win plus Space, a letter, digit or function key. Conflicts are reported.','muted'))
        layout.addWidget(frame)
        frame,inside=card('Startup and window behavior')
        self.field(inside,'startup','Start VectorDash when I sign in to Windows','check')
        self.field(inside,'start_hidden','Start hidden in the system tray at startup','check')
        self.field(inside,'close_behavior','When I close the window','combo',['Hide to tray','Minimize to taskbar','Exit'])
        self.field(inside,'minimize_behavior','When I minimize the window','combo',['Taskbar','Tray'])
        inside.addWidget(label('The tray icon provides Search, Pause/resume, Settings and Exit. Closing to tray keeps schedules and the hotkey available.','muted'))
        layout.addWidget(frame)
        layout.addWidget(button('Apply settings',self.save,True))
        frame,inside=card('Agent access (MCP)','Read-only search and indexed file information. Local access by default; optionally connect trusted agents on your local network.')
        self.field(inside,'mcp_enabled','Enable MCP server','check')
        self.field(inside,'mcp_lan','Allow access from the local network','check')
        self.field(inside,'mcp_port','Local HTTP port','spin',minimum=1024,maximum=65535)
        self.field(inside,'mcp_auth','Require HTTP bearer authentication','check')
        self.field(inside,'mcp_token','Bearer token')
        self.widgets['mcp_token'].setEchoMode(QLineEdit.EchoMode.Password)
        self.mcp_status=label('Stopped','muted')
        inside.addWidget(self.mcp_status)
        inside.addWidget(row(button('Apply MCP settings',self.save,True),button('Copy connection details',self.copy_mcp),button('Generate new token',self.rotate_mcp_token)))
        inside.addWidget(label('Agents can read indexed excerpts. Use only trusted agents. LAN access uses unencrypted HTTP. Keep bearer authentication enabled on shared networks. Click Apply MCP settings to start or stop. For LAN clients, allow this app’s HTTP port through Windows Firewall on your private network.','muted'))
        layout.addWidget(frame)
        frame,inside=card('Index privacy','The index is encrypted for your Windows account. Running apps under your account can still access data while VectorDash is unlocked.')
        self.field(inside,'exclude_sensitive','Skip content with potential credentials (including OCR)','check')
        self.field(inside,'index_excluded_types','Exclude types from indexing (comma separated)')
        self.excluded_sources=QListWidget()
        self.excluded_sources.addItems(self.settings.get('excluded_folders',[]))
        self.excluded_sources.setMaximumHeight(130)
        inside.addWidget(self.excluded_sources)
        inside.addWidget(row(button('Exclude folder',self.add_exclusion),button('Remove exclusion',lambda:self.excluded_sources.takeItem(self.excluded_sources.currentRow()))))
        inside.addWidget(label('Apply settings and update the index to remove previously indexed excluded content. Credential detection is best effort, especially for unreadable screenshots.','muted'))
        layout.addWidget(frame)
        layout.addWidget(button('Apply settings',self.save,True))
        frame,inside=card('Excluded search files','Right-click a search result to hide that file. Restore it here without rebuilding the index.')
        self.excluded_search_files=QListWidget()
        self.excluded_search_files.addItems(self.settings.get('search_excluded_files',[]))
        self.excluded_search_files.setMaximumHeight(160)
        inside.addWidget(self.excluded_search_files)
        inside.addWidget(button('Include selected file again',self.restore_search_file))
        layout.addWidget(frame)
        frame,inside=card('Local data')
        inside.addWidget(label(str(self.store.home),'muted'))
        inside.addWidget(button('Open data folder',lambda:self.open_file(str(self.store.home))))
        inside.addWidget(button('Exit VectorDash',self.exit_app))
        layout.addWidget(frame)
        layout.addStretch()

    def exclude_search_file(self,path):
        normalized=os.path.normcase(os.path.abspath(path))
        paths=self.settings.setdefault('search_excluded_files',[])
        if normalized not in paths:paths.append(normalized)
        self.excluded_search_files.clear();self.excluded_search_files.addItems(paths)
        self.store.save()
        for results in (self.results,self.quick.results):
            for index in range(results.count()-1,-1,-1):
                item=results.item(index);value=item.data(Qt.ItemDataRole.UserRole)
                candidate=value.path if isinstance(value,SearchResult) else value
                if os.path.normcase(os.path.abspath(candidate))==normalized:results.takeItem(index)
        self.request_search(self.quick.input.text() if self.quick.isVisible() else self.query.text())
        self.set_status('Excluded from future searches: '+Path(path).name)

    def restore_search_file(self):
        item=self.excluded_search_files.currentItem()
        if not item:return
        self.settings['search_excluded_files'].remove(item.text())
        self.excluded_search_files.takeItem(self.excluded_search_files.row(item))
        self.store.save()
        self.request_search(self.quick.input.text() if self.quick.isVisible() else self.query.text())
        self.set_status('File included in searches again')

    def add_exclusion(self):
        path=QFileDialog.getExistingDirectory(self,'Exclude folder')
        if path:self.excluded_sources.addItem(path)

    def apply_mcp(self):
        if self.mcp:self.mcp.stop();self.mcp=None
        if not self.settings.get('mcp_enabled'):
            self.mcp_status.setText('Stopped');return
        try:
            from mcp_server import McpServer
            self.mcp=McpServer(self.engine,lambda:dict(self.settings),lambda:{'indexing':self.indexing,'paused':self.engine.paused.is_set()})
            self.mcp.start()
            self.mcp_status.setText(f'Running: {self.mcp.url} · '+('Bearer required' if self.settings.get('mcp_auth') else 'No authentication'))
        except Exception as error:
            self.mcp=None;self.mcp_status.setText('MCP unavailable: '+str(error))

    def rotate_mcp_token(self):
        import secrets
        self.widgets['mcp_token'].setText(secrets.token_urlsafe(32))

    def copy_mcp(self):
        from mcp_network import connection_details
        try:
            config=connection_details(self.read_widgets())
        except ValueError as error:
            self.set_status(str(error));return
        QApplication.clipboard().setText(json.dumps(config,indent=2))
        self.set_status('MCP connection details copied' if self.mcp else 'MCP details copied · server stopped; enable MCP and click Apply MCP settings')

    def read_widgets(self):
        values=dict(self.settings)
        for key,widget in self.widgets.items():
            if isinstance(widget,QCheckBox):
                values[key]=widget.isChecked()
            elif isinstance(widget,QComboBox):
                values[key]=widget.currentText()
            elif isinstance(widget,QSpinBox):
                values[key]=widget.value()
            elif isinstance(widget,QTimeEdit):
                values[key]=widget.time().toString('HH:mm')
            else:
                values[key]=widget.text().strip()
        values['excluded_folders']=[self.excluded_sources.item(i).text() for i in range(self.excluded_sources.count())]
        values['semantic']=True
        values['setup_components']={key:choice.isChecked() for key,choice in self.setup_choices.items()}
        values['roots']=[self.sources.item(i).text() for i in range(self.sources.count())]
        values['code_roots']=[self.code_sources.item(i).text() for i in range(self.code_sources.count())]
        values['device']=self.devices.currentData() or ''
        values['schedule_days']=[i for i,check in enumerate(self.day_checks) if check.isChecked()]
        return values

    def save(self, quiet=False):
        values=self.read_widgets()
        if values.get('mcp_enabled') and values.get('mcp_auth') and len(values.get('mcp_token',''))<24:
            self.error('Generate a bearer token with at least 24 characters.');return False
        if values.get('server_mode')=='Remote':
            from urllib.parse import urlparse
            endpoint=urlparse(values.get('remote_url',''))
            if endpoint.scheme not in ('http','https') or not endpoint.hostname:
                self.error('Enter a complete http:// or https:// remote server URL.')
                return False
        if values['schedule_mode'] in ('At selected time','Time + idle') and not values['schedule_days']:
            self.error('Select at least one scheduling day.')
            return False
        old_shortcut=self.settings['hotkey']
        try:
            if values['hotkey']!=old_shortcut:
                self.hotkey.register(values['hotkey'])
            if values['startup']!=self.settings['startup']:
                set_startup(values['startup'])
        except (ValueError,OSError) as error:
            if self.hotkey.current!=old_shortcut:
                try:
                    self.hotkey.register(old_shortcut)
                except (OSError,ValueError):
                    pass
            self.error(str(error))
            return False
        schedule_keys=['schedule_mode','schedule_time','schedule_days']
        if any(values[key]!=self.settings[key] for key in schedule_keys):
            values['schedule_enabled_at']=datetime.now().isoformat()
            values['last_schedule_slot']=''
            self.schedule_retry=0
            if self.automatic_active:
                self.engine.cancel.set()
        self.settings=values
        self.store.values=values
        self.store.save()
        self.apply_mcp()
        self.apply_theme()
        self.update_tool_state()
        self.update_hotkey_status()
        if not quiet:
            self.set_status('Settings saved')
        return True

    def update_hotkey_status(self):
        if self.hotkey.current:
            self.shortcut_state.setText('Shortcut active: '+self.hotkey.current)
            self.shortcut_hint.setText('Describe a file, or press '+self.hotkey.current+' from any app.')
        else:
            self.shortcut_state.setText('Shortcut unavailable: another app is using it. Choose a different combination and Apply settings. Search and indexing still work.')
            self.shortcut_hint.setText('Describe a file. Quick-search shortcut unavailable; change it in Settings.')

    def apply_theme(self):
        theme=self.settings['theme']
        dark=theme=='Dark' or (theme=='System' and QApplication.styleHints().colorScheme()==Qt.ColorScheme.Dark)
        bg='#14151d' if dark else '#f5f6fa'
        surface='#20212c' if dark else '#ffffff'
        side='#191a24' if dark else '#eceef6'
        text='#eceef8' if dark else '#202239'
        muted='#a8acc2' if dark else '#646b85'
        border='#35384b' if dark else '#dfe3ef'
        hover='#2c2f42' if dark else '#e8e7fd'
        tick_icon=(Path(__file__).parent/'assets'/'check.png').as_posix()
        css=f'''
            QWidget {{font-family: 'Segoe UI'; font-size: 13px; color: {text};}}
            QMainWindow, QScrollArea, QStackedWidget {{background: {bg};}}
            QScrollArea > QWidget > QWidget {{background: {bg};}}
            #sidebar {{background: {side}; border-right: 1px solid {border};}}
            #brand {{font-size: 29px; font-weight: 700; color: #8573ff;}}
            #heading {{font-size: 28px; font-weight: 600;}}
            #cardTitle {{font-size: 17px; font-weight: 600;}}
            #muted {{color: {muted}; font-size: 12px;}}
            #card {{background: {surface}; border: 1px solid {border}; border-radius: 12px;}}
            #card QWidget {{background: transparent;}}
            QPushButton, #card QPushButton {{background: {surface}; border: 1px solid {border}; padding: 9px 14px; border-radius: 7px;}}
            QPushButton:hover, #card QPushButton:hover {{background: {hover}; border-color: #8573ff;}}
            QPushButton:pressed, #card QPushButton:pressed {{background: {'#40356d' if dark else '#d5cffb'}; border-color: #6757e8;}}
            QPushButton:focus, #card QPushButton:focus {{border-color: #8573ff;}}
            QPushButton#primary, #card QPushButton#primary {{background: #6757e8; color: white; border-color: #6757e8;}}
            QPushButton#primary:hover, #card QPushButton#primary:hover {{background: #7868f2;}}
            QPushButton#primary:pressed, #card QPushButton#primary:pressed {{background: #5140cb; border-color: #5140cb;}}
            QPushButton:disabled, QPushButton#primary:disabled, #card QPushButton:disabled, #card QPushButton#primary:disabled {{background: {border}; color: {muted}; border-color: {border};}}
            QLineEdit, QComboBox, QSpinBox, QTimeEdit {{background: {surface}; border: 1px solid {border}; border-radius: 7px; padding: 8px; selection-background-color: #6757e8;}}
            QLineEdit:focus {{border-color: #8573ff;}}
            QComboBox QAbstractItemView {{background: {surface}; color: {text}; selection-background-color: {hover}; border: 1px solid {border};}}
            #searchInput {{font-size: 18px; padding: 16px; border-radius: 11px;}}
            QListWidget {{background: {surface}; border: 1px solid {border}; border-radius: 8px; outline: none;}}
            QListWidget::item {{padding: 7px; border-radius: 6px;}}
            QListWidget::item:selected {{background: {hover}; color: {text};}}
            #navigation {{background: transparent; border: none; font-size: 14px;}}
            #navigation::item {{padding: 13px 10px;}}
            #navigation::item:selected {{background: {hover}; color: #8573ff;}}
            #results {{background: transparent; border: none;}}
            #results::item {{background: {surface}; border: 1px solid {border}; padding: 0;}}
            #results::item:selected {{border-color: #8573ff; background: {hover};}}
            #resultTitle {{font-weight: 600; font-size: 14px;}}
            #snippet {{color: {muted}; font-size: 12px;}}
            #quickSearch {{background: transparent; border: none;}}
            #quickSearch QLineEdit, #quickSearch QComboBox {{background: {'rgba(25, 16, 34, 110)' if dark else 'rgba(255, 255, 255, 130)'}; border-color: rgba(138, 115, 168, 45);}}
            #quickSearch #searchInput {{border: none; border-radius: 8px; padding: 14px;}}
            #quickSearch QPushButton {{background: {'rgba(65, 43, 80, 115)' if dark else 'rgba(248, 242, 255, 150)'}; border-color: rgba(138, 115, 168, 35);}}
            #quickSearch QPushButton:hover {{background: {'rgba(97, 65, 120, 165)' if dark else 'rgba(225, 209, 248, 180)'};}}
            #quickSearch QPushButton:pressed {{background: {'rgba(110, 74, 141, 190)' if dark else 'rgba(208, 185, 239, 200)'};}}
            #quickSearch QListWidget {{background: transparent;}}
            #quickSearch QListWidget::item {{background: {'rgba(32, 24, 52, 185)' if dark else 'rgba(255, 255, 255, 195)'};}}
            #quickSearch QListWidget::item:selected {{background: {'rgba(83, 57, 124, 210)' if dark else 'rgba(226, 215, 255, 215)'}; border-color: #8573ff;}}
            QPlainTextEdit, QTextEdit {{background: {surface}; color: {text}; border: 1px solid {border}; border-radius: 7px; selection-background-color: #6757e8;}}
            QCheckBox {{spacing: 8px; padding: 3px;}}
            QCheckBox::indicator {{width: 16px; height: 16px; border: 1px solid {border}; border-radius: 4px; background: {surface};}}
            QCheckBox::indicator:checked {{background: #6757e8; border-color: #6757e8; image: url("{tick_icon}");}}
            QProgressBar {{border: 1px solid {border}; border-radius: 6px; text-align: center; height: 18px;}}
            QProgressBar::chunk {{background: #6757e8; border-radius: 5px;}}
            QScrollBar:vertical {{background: transparent; width: 10px; margin: 2px;}}
            QScrollBar:horizontal {{background: transparent; height: 10px; margin: 2px;}}
            QScrollBar::handle:vertical {{background: #8573ff; border-radius: 4px; min-height: 32px;}}
            QScrollBar::handle:horizontal {{background: #8573ff; border-radius: 4px; min-width: 32px;}}
            QScrollBar::handle:hover {{background: #6757e8;}}
            QScrollBar::add-line, QScrollBar::sub-line {{width: 0px; height: 0px; border: none;}}
            QScrollBar::add-page, QScrollBar::sub-page {{background: transparent;}}
            QSplitter::handle {{background: {border}; width: 4px; height: 4px; border-radius: 2px;}}
            #quickSearch QSplitter::handle {{background: rgba(133, 115, 255, 90);}}
            QMenu {{background: {surface}; border: 1px solid {border}; padding: 6px;}}
            QMenu::item {{padding: 8px 20px;}}
            QMenu::item:selected {{background: {hover};}}
        '''
        QApplication.instance().setStyleSheet(css)
        if hasattr(self,'quick'):self.quick.refresh_material()

    def error(self,text):
        self.set_status(text)
        QMessageBox.warning(self,'VectorDash',text)

    def set_status(self,text):
        self.status.setText(text)
        if self.indexing:
            self.index_status.setText(text)
            self.source_status.setText(text)
        self.logger.log(logging.WARNING if text.startswith('Skipped ') else logging.ERROR if 'failed' in text.lower() else logging.DEBUG if text.startswith('Reading ') else logging.INFO,text)

    def show_main(self,index=None):
        if index is not None:
            self.navigation.setCurrentRow(index)
        self.showNormal()
        self.raise_()
        self.activateWindow()
        if index in (None,0):
            self.query.setFocus()

    def toggle_quick(self):
        if self.quick.isVisible():
            self.quick.hide()
        else:
            self.quick.summon()

    def accept_ipc(self,server):
        socket=server.nextPendingConnection()
        if socket:
            def received():
                if socket.readAll().data()!=b'startup':
                    self.quick.summon()
                socket.disconnectFromServer()
            socket.readyRead.connect(received)
            if socket.bytesAvailable():
                received()
            socket.disconnected.connect(socket.deleteLater)

    def add_folder(self):
        self.add_source(QFileDialog.getExistingDirectory(self,'Choose a folder'))

    def add_source(self,path):
        if path and path.casefold() not in [self.sources.item(i).text().casefold() for i in range(self.sources.count())]:
            self.sources.addItem(str(Path(path).absolute()))
            self.settings['roots']=[self.sources.item(i).text() for i in range(self.sources.count())]
            self.store.save()

    def remove_source(self):
        index=self.sources.currentRow()
        if index>=0:
            self.sources.takeItem(index)
            self.settings['roots']=[self.sources.item(i).text() for i in range(self.sources.count())]
            self.store.save()
            self.set_status('Source removed. Update the index to remove its results.')

    def start_index(self,force=False,scheduled=False,slot=''):
        if self.installing:
            self.set_status('Finish setup before indexing')
            return
        if self.indexing:
            self.set_status('An indexing task is already running')
            return
        if not scheduled and not self.save(quiet=True):
            return
        if not self.settings['roots'] and not (self.settings.get('code_enabled') and self.settings.get('code_roots')):
            self.set_status('Add a folder, drive or enabled code project before indexing')
            return
        self.engine.cancel.clear()
        self.engine.paused.clear()
        self.indexing=True
        self.source_progress.setRange(0,0)
        self.automatic_active=scheduled
        snapshot=dict(self.settings)
        self.set_status('Checking selected sources…')
        started=time.monotonic()
        def condition():
            if scheduled:
                rules=self.settings
                if rules['schedule_mode']=='Manual':
                    raise InterruptedError('Automatic schedule disabled; previous index retained')
                if time.monotonic()-started>rules['max_run_minutes']*60:
                    raise InterruptedError('Automatic run reached its time limit; previous index retained')
                if time.monotonic()-self.metrics_time>45:
                    return False
                if rules['schedule_mode'] in ('When idle','Time + idle'):
                    return idle_allowed(rules,self.metrics)[0]
                if rules['ac_only']:
                    return self.metrics.get('on_ac') is True
            return True
        def work():
            try:
                result=self.engine.index(snapshot,self.events.status.emit,condition,force)
                self.events.indexed.emit(result,slot,True,'Scheduled' if scheduled else 'Manual')
            except Exception as error:
                self.events.indexed.emit(str(error),slot,False,'Scheduled' if scheduled else 'Manual')
        self.launch(work)

    def index_finished(self,message,slot,success,origin):
        self.indexing=False
        self.source_progress.setRange(0,100)
        self.source_progress.setValue(100 if success else 0)
        self.source_status.setText(message)
        self.index_count.setText(f'{self.engine.indexed_file_count():,} indexed files')
        if success:
            self.launch(self.warm_search_cache)
        self.automatic_active=False
        self.engine.paused.clear()
        self.index_status.setText(message)
        self.set_status(message)
        if not success:
            self.logger.error('Indexing stopped: %s',message)
        now=datetime.now().isoformat(timespec='seconds')
        if success:
            self.settings['last_success']=now
            if slot:
                self.settings['last_schedule_slot']=slot
            self.last_status.setText('Last successful update: '+now)
        else:
            self.schedule_retry=time.monotonic()+300
        entry=f'{now} · {origin} · {"Complete" if success else "Stopped/failed"} · {message}'
        self.settings['schedule_history']=[entry]+self.settings.get('schedule_history',[])[:19]
        self.history.clear()
        self.history.addItems(self.settings['schedule_history'])
        self.store.save()
        if self.query.text():
            self.request_search(self.query.text())

    def pause_index(self):
        if not self.indexing:
            self.set_status('No indexing task running')
            return
        if self.engine.paused.is_set():
            self.engine.paused.clear()
            self.set_status('Indexing resumed')
        else:
            self.engine.paused.set()
            self.set_status('Indexing paused; search remains available')

    def query_changed(self):
        self.results.clear()
        self.search_sequence+=1
        self.search_timer.start()

    def request_search(self,query):
        self.search_sequence+=1
        self.pending_search=(self.search_sequence,query,dict(self.settings))
        if not self.search_running:
            self.run_search()

    def run_search(self):
        if not self.pending_search:
            return
        sequence,query,settings=self.pending_search
        self.pending_search=None
        self.search_running=True
        self.search_hint.setText('Searching…')
        self.quick.hint.setText('Searching…')
        def work():
            try:
                self.events.searched.emit(sequence,self.engine.search(query,settings),'')
            except Exception as error:
                self.events.searched.emit(sequence,[],str(error))
        self.launch(work)

    def search_finished(self,sequence,results,error):
        self.search_running=False
        if sequence==self.search_sequence:
            self.results.populate(results)
            self.quick.results.populate(results)
            self.search_hint.setText(error or f'{len(results)} results · Enter to open')
            self.quick.hint.setText(error or f'{len(results)} results · ↑ ↓ select · Enter open · Esc dismiss')
            if error:
                self.set_status(error)
        self.run_search()

    def open_file(self,path):
        if not path:
            return
        try:
            if isinstance(path,SearchResult):
                result=path
                from content import CODE
                if Path(result.path).suffix.lower() in CODE and result.kind!='code':
                    result.kind='code'
                    result.line=result.end_line=1
                if result.page or result.seconds>=0 or result.kind in ('image','code'):
                    self.show_large_preview(result)
                    return
                path=result.path
            os.startfile(path)
            self.quick.hide()
        except Exception as error:
            self.error(str(error))

    def preview_result(self,result):
        if not result:
            return
        try:
            if not isinstance(result,SearchResult):
                result=SearchResult(0,str(result),'')
            self.show_large_preview(result)
        except Exception as error:
            self.error('Preview unavailable: '+str(error))

    def show_large_preview(self,result):
        quick_active=self.quick.isVisible()
        if quick_active:self.quick.preview_open=True
        try:
            preview=Preview(result,self.quick if quick_active else self)
            self.large_preview=preview
            def finished(*args):
                if quick_active:
                    self.quick.preview_open=False
                    if self.shutting_down:return
                    self.quick.show();self.quick.raise_();self.quick.activateWindow();self.quick.results.setFocus()
                elif not self.shutting_down:
                    self.raise_();self.activateWindow();self.results.setFocus()
            preview.finished.connect(finished)
            preview.show();preview.raise_();preview.activateWindow()
        except Exception:
            self.quick.preview_open=False
            raise

    def copy_file(self,path):
        if not path:
            return
        if not Path(path).is_file():
            self.set_status('This file is no longer available to copy.')
            return
        mime=QMimeData()
        mime.setUrls([QUrl.fromLocalFile(str(Path(path).resolve()))])
        # Windows drop effect COPY; Qt supplies CF_HDROP for these file URLs.
        mime.setData('application/x-qt-windows-mime;value="Preferred DropEffect"',(1).to_bytes(4,'little'))
        QApplication.clipboard().setMimeData(mime)
        self.set_status('File copied. Paste it in Explorer or another app that accepts files.')

    def reveal(self):
        if path:=self.results.selected_path():
            try:
                reveal_in_explorer(path)
            except OSError as error:
                self.error(str(error))

    def copy_path(self):
        if path:=self.results.selected_path():
            QApplication.clipboard().setText(path)
            self.set_status('Path copied')

    def browse(self,key):
        path,_=QFileDialog.getOpenFileName(self,'Choose '+key,'','Executable (*.exe)' if key in ('server','ocr_path','ffmpeg_path') else 'GGUF model (*.gguf)')
        if path:
            self.widgets[key].setText(path)

    def refresh_devices(self):
        path=self.widgets['server'].text().strip()
        self.set_status('Detecting inference devices…')
        def work():
            try:
                result=subprocess.run([path,'--list-devices'],capture_output=True,text=True,timeout=30,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
                if result.returncode:
                    raise RuntimeError((result.stderr or result.stdout)[-1500:])
                output=result.stdout+'\n'+result.stderr
                self.events.devices.emit(parse_devices(output),output)
            except Exception as error:
                self.events.devices.emit([],str(error))
        self.launch(work)

    def devices_finished(self,devices,output):
        previous=self.devices.currentData()
        self.devices.clear()
        self.devices.addItem('CPU / system RAM (no GPU offload)','')
        for identifier,description in devices:
            self.devices.addItem(f'{identifier} — {description}',identifier)
        index=self.devices.findData(previous)
        self.devices.setCurrentIndex(max(0,index))
        self.set_status(f'{len(devices)} device(s) detected. Select an adapter and save.' if devices else 'No GPU devices detected: '+output[-500:])

    def start_model(self):
        if self.indexing:
            self.set_status('Stop indexing before changing the running model')
            return
        if not self.save(quiet=True):
            return
        if self.settings.get('server_mode')=='Remote':
            self.probe_model()
            return
        if self.model_process and self.model_process.poll() is None:
            if self.model_process.args==command(self.settings):
                self.set_status('The model is already running')
                return
            self.stop_model()
        try:
            if not Path(self.settings['server']).is_file() or not Path(self.settings['model']).is_file():
                raise ValueError('Choose or download a runtime and model first.')
            if self.settings.get('projector') and not Path(self.settings['projector']).is_file():
                raise ValueError('The selected vision/audio encoder does not exist. Download it or choose its mmproj GGUF file.')
            with net_socket.socket() as probe:
                occupied=probe.connect_ex(('127.0.0.1',8787))==0
            if occupied:
                try:
                    verify_server_model(dict(self.settings,semantic=True))
                    self.attach_local_server()
                    if self.settings.get('projector') and (self.settings.get('pdf_visuals',False) or any(self.settings.get(k,True) for k in ('index_images','index_audio','index_video'))):
                        with urllib.request.urlopen('http://127.0.0.1:8787/props',timeout=3) as response:
                            modes=json.load(response).get('modalities',{})
                        expected=command(self.settings)
                        actual=self.attached_server.cmdline() if self.attached_server else []
                        budget_changed=isinstance(actual,(list,tuple)) and bool(actual) and any(flag not in actual or actual[actual.index(flag)+1]!=expected[expected.index(flag)+1] for flag in ('--image-min-tokens','--image-max-tokens'))
                        if budget_changed or not (modes.get('vision') and modes.get('audio')):
                            if not self.attached_server:
                                raise ValueError('The existing server is text-only and cannot be managed by VectorDash. Restart it with the selected encoder.')
                            self.stop_model()
                        else:
                            self.loaded_modalities=modes
                            self.model_status('Model: loaded · text + vision + audio ready')
                            return
                    else:
                        self.probe_model()
                        return
                except Exception as error:
                    raise ValueError('Port 8787 is already occupied. Stop the existing server before starting a new model. '+str(error)) from error
            self.model_log=open(self.store.home/'model.log','w',encoding='utf-8')
            self.model_process=process_lifecycle.popen(command(self.settings),stdout=self.model_log,stderr=self.model_log,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
            self.model_state.setText('Loading the model…')
            process=self.model_process
            def wait_ready():
                for _ in range(90):
                    if self.model_process is not process:
                        return
                    if process.poll() is not None:
                        self.events.model.emit('Model exited. Open the data folder and inspect model.log.')
                        return
                    try:
                        with urllib.request.urlopen('http://127.0.0.1:8787/health',timeout=1) as response:
                            if json.load(response).get('status')=='ok':
                                self.events.model.emit('loaded · local embedding server ready')
                                self.probe_model()
                                return
                    except Exception:
                        pass
                    time.sleep(1)
                self.events.model.emit('Model still loading or port unavailable. Inspect model.log.')
            self.launch(wait_ready)
        except (OSError,ValueError) as error:
            self.error(str(error))

    def model_status(self,message):
        previous=self.model_state.text()
        self.model_loaded=message.startswith('loaded') or 'test passed' in message.lower() or 'Model: loaded' in message
        if not self.model_loaded:
            self.loaded_modalities={}
        self.model_state.setText(message)
        if previous!=message:
            self.set_status(message)
        self.update_tool_state()

    def check_model_exit(self):
        if self.model_process and self.model_process.poll() is not None:
            code=self.model_process.returncode
            self.model_process=None
            if self.model_log:
                self.model_log.close()
                self.model_log=None
            self.model_status(f'Model process exited ({code}). See model.log for details.')

    def stop_model(self):
        self.model_generation+=1
        if self.indexing:
            self.engine.cancel.set()
        self.engine.clear_query_cache()
        if self.attached_server:
            try:
                self.attached_server.terminate()
                self.attached_server.wait(timeout=3)
            except psutil.TimeoutExpired:
                try: self.attached_server.kill()
                except psutil.Error: pass
            except psutil.Error:
                pass
            self.attached_server=None
        process=self.model_process
        self.model_process=None
        if process and process.poll() is None:
            try:
                root=psutil.Process(process.pid)
                children=root.children(recursive=True)
                for child in children:
                    try: child.terminate()
                    except psutil.Error: pass
                root.terminate()
                _,alive=psutil.wait_procs(children+[root],timeout=3)
                for child in alive:
                    try: child.kill()
                    except psutil.Error: pass
                process.wait(timeout=3)
            except (psutil.Error,subprocess.TimeoutExpired):
                if process.poll() is None:
                    process.kill()
        if self.model_log:
            self.model_log.close()
            self.model_log=None
        self.model_status('Model: stopped' if process else 'No local model started by VectorDash is running')

    def test_model(self):
        if not self.save(quiet=True):
            return
        snapshot=dict(self.settings,semantic=True)
        self.model_state.setText('Testing a local embedding request…')
        def work():
            try:
                verify_server_model(snapshot)
                with self.engine.inference:
                    vector=embed('task: search result | query: A document about Windows desktop search',request=lambda text:embedding_request(text,snapshot))
                with urllib.request.urlopen(urllib.request.Request(server_url(snapshot)+'/props',headers={'Authorization':'Bearer '+snapshot['remote_key']} if snapshot.get('remote_key') else {}),timeout=5) as response:
                    modalities=json.load(response).get('modalities',{})
                self.loaded_modalities=modalities
                ready=modalities.get('vision') and modalities.get('audio')
                if snapshot.get('server_mode')!='Remote' and snapshot.get('projector') and not ready:
                    raise ValueError('The running server has no vision/audio encoder. Stop it and start the model with the selected encoder.')
                self.events.model.emit(f'Embedding test passed · {len(vector)} dimensions · '+('text + vision + audio ready' if ready else 'text only; download/select an encoder and restart for media search'))
            except Exception as error:
                self.events.model.emit('Embedding test failed: '+str(error))
        self.launch(work)

    def load_catalog(self):
        self.set_status('Loading official runtime and model catalogs…')
        def work():
            try:
                runtimes,models=catalogs()
                self.events.catalog.emit(runtimes,models,'')
            except Exception as error:
                self.events.catalog.emit([],[],str(error))
        self.launch(work)

    def catalog_finished(self,runtimes,models,error):
        if error:
            self.error('Catalog unavailable: '+error)
            return
        selected_quant=self.quant_choice.currentData() or 'Q8_0'
        self.runtimes,self.models=runtimes,models
        self.runtime_choice.clear()
        self.quant_choice.clear()
        for item in runtimes:
            self.runtime_choice.addItem(f'{item["label"]} · {item["size"]/1024**2:.0f} MB')
        for item in models:
            self.quant_choice.addItem(f'{item["label"]} · {item["size"]/1024**2:.0f} MB',item['label'])
        self.quant_choice.setCurrentIndex(next((i for i,item in enumerate(models) if item['label']==selected_quant),0))
        preferred=next((i for i,item in enumerate(runtimes) if item['backend']=='vulkan'),0)
        if self.settings['mode']=='Dedicated GPU' and self.settings['device'].upper().startswith('CUDA'):
            preferred=next((i for i,item in enumerate(runtimes) if item['backend'].startswith('cuda-12')),preferred)
        elif self.settings['mode']=='CPU / system RAM':
            preferred=next((i for i,item in enumerate(runtimes) if item['backend']=='cpu'),preferred)
        self.runtime_choice.setCurrentIndex(preferred)
        self.catalog_summary()
        self.set_status('Downloads available. Select a runtime and quantization.')

    def catalog_summary(self):
        r=self.runtime_choice.currentIndex()
        q=self.quant_choice.currentIndex()
        if 0<=r<len(self.runtimes) and 0<=q<len(self.models):
            total=self.runtimes[r]['size']+self.models[q]['size']+self.models[q].get('projector',{}).get('size',0)
            self.download_summary.setText(f'Total download: {total/1024**2:.0f} MB. Model SHA-256 verified. Choose the runtime for your hardware; existing installations are retained.')

    def component_installed(self,key):
        try:
            if key=='ocr':
                program=ocr_program(self.settings)
                if not program:
                    return False
                data=Path(self.settings['ocr_data']) if self.settings.get('ocr_data') else Path(program).parent/'tessdata'
                return all((data/(code+'.traineddata')).is_file() for code in self.settings.get('ocr_languages','eng').split('+'))
            if key=='ffmpeg':
                return Path(ffmpeg(self.settings)).is_file()
            path=Path(self.settings.get(key,''))
            installed=path.is_file()
            if key in ('model','projector') and installed:
                quant=self.quant_choice.currentData() if hasattr(self,'quant_choice') else None
                if quant and any(token in path.name.upper() for token in ('Q8_0','BF16')):
                    installed=quant in path.name.upper()
            return installed
        except Exception:
            return False

    def update_tool_state(self,*args):
        if not hasattr(self,'setup_choices'):
            return
        for key,status in self.component_status.items():
            installed=self.component_installed(key)
            if self.settings.get('server_mode')=='Remote' and key in ('server','model','projector'):
                installed=self.model_loaded
            loaded=installed and (self.model_loaded if key in ('server','model') else bool(self.loaded_modalities.get('vision') or self.loaded_modalities.get('audio')) if key=='projector' else self.tool_ready.get(key,False))
            text,color=('Loaded' if key in ('server','model','projector') else 'Ready','#16834a') if loaded else ('Installed · not loaded','#aa7900') if installed else ('Not installed','#c73545')
            status.setText(text)
            status.setStyleSheet('color: '+color+'; font-weight: 600;')
        selected=[key for key,choice in self.setup_choices.items() if choice.isChecked()]
        ready=all(self.component_installed(key) for key in set(selected)|{'server','model'})
        self.setup_button.setText('Start' if ready else 'Download, install and start')
        loaded=ready and self.model_loaded and all(('Loaded' if key in ('server','model','projector') else 'Ready')==self.component_status[key].text() for key in selected)
        self.setup_button.setEnabled(not self.installing and not loaded)
        self.setup_stop.setEnabled(self.settings.get('server_mode')!='Remote' and (self.model_loaded or bool(self.model_process) or bool(self.attached_server)))
        self.setup_cancel.setVisible(self.installing)
        self.setup_progress.setVisible(self.installing)
        self.download_bar.setVisible(self.installing)
        if hasattr(self,'tool_state'):
            self.tool_state.setText('OCR: '+self.component_status['ocr'].text()+' · Media decoder: '+self.component_status['ffmpeg'].text())

    def refresh_component_status(self):
        if not self.save(quiet=True):
            return
        self.probe_model()
        snapshot=dict(self.settings)
        def work():
            ready={}
            for key,args in [('ocr',[ocr_program(snapshot),'--version']),('ffmpeg',[ffmpeg(snapshot),'-version'])]:
                try:
                    if args[0]:
                        result=process_lifecycle.run(args,capture_output=True,timeout=10,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
                        ready[key]=result.returncode==0
                        if key=='ocr' and ready[key]:
                            languages=[args[0],'--list-langs']
                            if snapshot.get('ocr_data'):
                                languages+=['--tessdata-dir',snapshot['ocr_data']]
                            packs=process_lifecycle.run(languages,capture_output=True,text=True,timeout=10,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
                            ready[key]=packs.returncode==0 and all(code in packs.stdout.splitlines() for code in snapshot.get('ocr_languages','eng').split('+'))
                except Exception:
                    ready[key]=False
            if not self.shutting_down:
                self.events.tools_ready.emit(ready)
        self.launch(work)

    def tools_status_finished(self,ready):
        self.tool_ready=ready
        self.update_tool_state()

    def install_tool(self,key):
        if self.installing or self.indexing:
            self.set_status('Finish the current download/indexing task before setting up tools')
            return
        if not self.save(quiet=True):
            return
        snapshot=dict(self.settings)
        self.download_cancel.clear()
        self.installing=True
        self.update_tool_state()
        self.download_bar.setValue(0)
        self.set_status('Setting up '+('OCR and selected languages' if key=='ocr' else 'included FFmpeg'))
        def work():
            try:
                paths=install_ocr(snapshot,self.store.home,self.download_cancel,self.events.progress.emit) if key=='ocr' else install_ffmpeg(self.store.home,self.download_cancel,self.events.progress.emit)
                self.events.installed.emit('tools',json.dumps(paths),'')
            except Exception as error:
                self.events.installed.emit('tools','',str(error))
        self.launch(work)

    def install(self,key):
        if self.installing:
            self.set_status('A download is already running')
            return
        quant=self.quant_choice.currentData() or 'Q8_0'
        runtime_index=self.runtime_choice.currentIndex()
        model_index=self.quant_choice.currentIndex()
        available_runtimes=list(self.runtimes)
        available_models=list(self.models)
        mode=self.widgets['mode'].currentText()
        device=self.devices.currentData() or ''
        self.download_cancel.clear()
        self.installing=True
        self.update_tool_state()
        self.download_bar.setValue(0)
        self.set_status('Preparing '+('vision/audio encoder' if key=='projector' else 'download'))
        def work():
            try:
                runtimes,models=available_runtimes,available_models
                if not runtimes or not models:
                    runtimes,models=catalogs()
                    self.events.catalog.emit(runtimes,models,'')
                    preferred='cpu' if mode=='CPU / system RAM' else ('cuda-' if str(device).upper().startswith('CUDA') else 'vulkan')
                    runtime_index_local=next((i for i,item in enumerate(runtimes) if item['backend'].startswith(preferred)),0)
                    model_index_local=next((i for i,item in enumerate(models) if item['label']==quant),-1)
                    if model_index_local<0:
                        raise ValueError('The selected quantization is unavailable. Load the catalog and select an available model.')
                else:
                    runtime_index_local=runtime_index
                    model_index_local=model_index
                item=runtimes[runtime_index_local] if key=='server' else models[model_index_local]
                if key=='projector':
                    item=item.get('projector')
                    if not item:
                        raise ValueError('No matching vision/audio encoder is available for this model.')
                function=install_runtime if key=='server' else install_model
                path=function(item,self.store.home,self.download_cancel,self.events.progress.emit)
                if key=='model' and item.get('projector'):
                    encoder=install_model(item['projector'],self.store.home,self.download_cancel,self.events.progress.emit)
                    path=json.dumps({'model':path,'projector':encoder})
                self.events.installed.emit(key,path,'')
            except Exception as error:
                self.events.installed.emit(key,'',str(error))
        self.launch(work)

    def download_progress(self,title,done,total):
        self.download_bar.setValue(round(done/total*100) if total else 0)
        self.setup_progress.setValue(self.download_bar.value())
        self.set_status(f'{title}: {done/1024**2:.0f} / {total/1024**2:.0f} MB' if total else title)

    def install_finished(self,key,path,error):
        self.installing=False
        self.update_tool_state()
        if error:
            self.set_status(error)
            return
        paths=json.loads(path) if path.startswith('{') else {key:path}
        for name,value in paths.items():
            if name=='device':
                self.devices.clear()
                self.devices.addItem(value,value)
            else:
                self.widgets[name].setText(value)
            self.settings[name]=value
        self.store.save()
        self.download_bar.setValue(100)
        self.setup_progress.setValue(100)
        self.set_status('Installed. Refresh devices, select hardware, then start/test the model.' if key=='server' else 'Model downloaded and verified. Start/test the model, then update your index.')
        self.update_tool_state()
        if key=='tools':
            self.set_status('Tools installed, verified and configured. Update the index to apply OCR changes.')
        if key=='setup':
            self.settings['semantic']=True
            self.store.save()
            self.start_model()
            self.refresh_component_status()
        if key=='server':
            self.refresh_devices()

    def setup_all(self):
        if self.installing or self.indexing:
            self.set_status('Wait for the current setup or indexing task to finish')
            return
        selection={key:choice.isChecked() for key,choice in self.setup_choices.items()}
        self.widgets['ocr_enabled'].setChecked(selection['ocr'])
        self.widgets['index_images'].setChecked(selection['projector'])
        for key in ('index_audio','index_video'):
            self.widgets[key].setChecked(selection['projector'] and selection['ffmpeg'])
        if not self.save(quiet=True):
            return
        if self.settings.get('server_mode')=='Remote':
            self.probe_model()
            self.set_status('Remote models are installed on their host. Use local setup for downloads on this PC.')
            return
        if all(self.component_installed(key) for key in {key for key,value in selection.items() if value}|{'server','model'}):
            self.start_model()
            self.refresh_component_status()
            return
        for key in ('server','model'):
            if not selection[key] and not self.component_installed(key):
                self.set_status('Select the runtime and text model, or choose existing files before starting.')
                return
        if self.model_process:
            self.stop_model()
        snapshot=dict(self.settings)
        quant=self.quant_choice.currentData() or 'Q8_0'
        runtime_index=self.runtime_choice.currentIndex()
        selected=self.runtimes[runtime_index] if 0<=runtime_index<len(self.runtimes) else None
        self.installing=True
        self.update_tool_state()
        self.download_cancel.clear()
        self.set_status('Preparing one-click setup…')
        def work():
            paths={}
            try:
                need_model=any(selection[key] and not self.component_installed(key) for key in ('server','model','projector'))
                runtimes,models=catalogs() if need_model else ([],[])
                preferred='cpu' if snapshot['mode']=='CPU / system RAM' else ('cuda-' if str(snapshot['device']).upper().startswith('CUDA') else 'vulkan')
                runtime=(selected if isinstance(selected,dict) else next(item for item in runtimes if item['backend'].startswith(preferred))) if need_model else None
                model=next(item for item in models if item['label']==quant) if need_model else {}
                for key,item,installer in [('server',runtime,install_runtime),('model',model,install_model),('projector',model.get('projector'),install_model)]:
                    if not selection[key]:
                        if snapshot.get(key):
                            paths[key]=snapshot[key]
                        continue
                    if self.download_cancel.is_set():
                        raise InterruptedError('Setup cancelled')
                    if not Path(snapshot.get(key,'')).is_file() or (key!='server' and quant not in Path(snapshot.get(key,'')).name):
                        if not item:
                            raise ValueError('No downloadable component is available for '+key)
                        paths[key]=installer(item,self.store.home,self.download_cancel,self.events.progress.emit)
                    else:
                        paths[key]=snapshot[key]
                if snapshot['mode']!='CPU / system RAM' and not snapshot.get('device'):
                    result=process_lifecycle.run([paths['server'],'--list-devices'],capture_output=True,text=True,timeout=30,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
                    adapters=[item for item in parse_devices(result.stdout+'\n'+result.stderr) if not item[0].upper().startswith('CPU')]
                    if not adapters:
                        raise ValueError('No compatible GPU was found. Select CPU / system RAM and retry setup.')
                    if snapshot['mode']=='Integrated GPU':
                        integrated=[item for item in adapters if any(term in item[1].lower() for term in ('integrated','intel','radeon graphics','apu'))]
                        if not integrated:
                            raise ValueError('Select your integrated GPU from Refresh devices before starting the model.')
                        adapters=integrated
                    paths['device']=adapters[0][0]
                if selection['ffmpeg']:
                    paths.update(install_ffmpeg(self.store.home,self.download_cancel,self.events.progress.emit))
                if selection['ocr']:
                    paths.update(install_ocr(snapshot,self.store.home,self.download_cancel,self.events.progress.emit))
                self.events.installed.emit('setup',json.dumps(paths),'')
            except Exception as error:
                # Preserve successfully installed components so retry does not repeat work.
                if paths:
                    self.events.installed.emit('tools',json.dumps(paths),'')
                self.events.installed.emit('setup','',str(error))
        self.launch(work)

    def probe_model(self):
        if self.probing_model or self.shutting_down:
            return
        self.probing_model=True
        generation=self.model_generation
        snapshot=dict(self.settings,semantic=True)
        def work():
            try:
                verify_server_model(snapshot)
                request=urllib.request.Request(server_url(snapshot)+'/props',headers={'Authorization':'Bearer '+snapshot['remote_key']} if snapshot.get('remote_key') else {})
                with urllib.request.urlopen(request,timeout=3) as response:
                    props=json.load(response)
                modes=props.get('modalities',{})
                self.loaded_modalities=modes
                message='loaded · text: ready · images: '+('ready' if modes.get('vision') else 'unavailable')+' · audio: '+('ready' if modes.get('audio') else 'unavailable')
            except Exception as error:
                self.logger.debug('Model status check: %s',error)
                message='not loaded · '+(str(error) if isinstance(error,ValueError) else 'Start the local model or check the server connection.')
            finally:
                self.probing_model=False
            if not self.shutting_down and generation==self.model_generation:
                if snapshot.get('server_mode')!='Remote' and message.startswith('loaded'):
                    self.attach_local_server()
                self.events.model.emit(message)
        self.launch(work)

    def animate_index_icon(self):
        active=self.indexing and not self.engine.paused.is_set() and not self.engine.cancel.is_set()
        self.icon_phase+=.3
        icon=app_icon(active,self.icon_phase)
        self.setWindowIcon(icon)
        self.tray.setIcon(icon)
        self.tray.setToolTip('VectorDash · '+('Indexing' if active else 'Paused' if self.indexing else 'Ready'))

    def logs_page(self):
        layout=self.page('Logs','Indexing skips, setup messages and model status. Full history is saved in vectordash.log.')
        self.log_filter=ScrollSafeComboBox()
        self.log_filter.addItems(['All','Verbose','Info','Warning','Error'])
        self.log_filter.currentTextChanged.connect(self.render_logs)
        layout.addWidget(row(self.log_filter,button('Open log file',lambda:os.startfile(str(self.store.home/'vectordash.log'))),button('Open model output',lambda:self.open_model_log())))
        self.log_view=QPlainTextEdit()
        self.log_view.setReadOnly(True)
        self.log_view.setMaximumBlockCount(2000)
        layout.addWidget(self.log_view)
        if (self.store.home/'vectordash.log').exists():
            self.render_logs()

    def open_model_log(self):
        path=self.store.home/'model.log'
        if path.exists():
            os.startfile(str(path))
        else:
            self.set_status('No local model log yet. Start a local model first.')

    def append_log(self,level,message):
        self.log_records.append((level,datetime.now().strftime('%H:%M:%S')+' '+level+' '+message))
        self.log_records=self.log_records[-2000:]
        if hasattr(self,'log_view'):
            wanted={'Verbose':'DEBUG','Info':'INFO','Warning':'WARNING','Error':'ERROR'}.get(self.log_filter.currentText())
            if not wanted or level==wanted:
                self.log_view.appendPlainText(self.log_records[-1][1])

    def render_logs(self):
        wanted={'Verbose':'DEBUG','Info':'INFO','Warning':'WARNING','Error':'ERROR'}.get(self.log_filter.currentText())
        self.log_view.setPlainText('\n'.join(text for level,text in self.log_records if not wanted or level==wanted))
        bar=self.log_view.verticalScrollBar()
        bar.setValue(bar.maximum())

    def tick(self):
        self.probe_model()
        self.next_status.setText('Next run: '+next_run(self.settings,datetime.now()))
        if self.collecting:
            return
        self.collecting=True
        check_gpu=self.settings['check_gpu']
        def work():
            try:
                cpu=psutil.cpu_percent(interval=.2)
                battery=psutil.sensors_battery()
                metrics={'cpu':cpu,'gpu':gpu_utilization() if check_gpu else None,'idle':idle_seconds(),'on_ac':battery is None or battery.power_plugged}
            except Exception:
                metrics={'cpu':None,'gpu':None,'idle':None,'on_ac':None}
            self.events.metrics.emit(metrics)
        self.launch(work)

    def metrics_finished(self,metrics):
        self.collecting=False
        self.metrics=metrics
        self.metrics_time=time.monotonic()
        def number(value,suffix):
            return f'{value:.0f}{suffix}' if value is not None else 'unavailable'
        self.telemetry.setText(f'CPU {number(metrics["cpu"],"%")} · GPU {number(metrics["gpu"],"%") if self.settings["check_gpu"] else "not required"} · User idle {number(metrics["idle"],"s")} · Power {"AC" if metrics["on_ac"] else "battery/unknown"}')
        due,slot,reason=schedule_due(self.settings,datetime.now(),metrics)
        if self.indexing:
            allowed,condition=idle_allowed(self.settings,metrics)
            self.schedule_status.setText('Indexing in progress · '+(condition if self.settings['schedule_mode'] in ('When idle','Time + idle') else 'running'))
        else:
            self.schedule_status.setText(reason)
        if due and not self.indexing and time.monotonic()>=self.schedule_retry and self.settings['roots']:
            self.start_index(scheduled=True,slot=slot)

    def changeEvent(self,event):
        super().changeEvent(event)
        if event.type()==QEvent.Type.WindowStateChange and self.isMinimized() and self.settings['minimize_behavior']=='Tray' and QSystemTrayIcon.isSystemTrayAvailable():
            QTimer.singleShot(0,self.hide)

    def closeEvent(self,event):
        if self.shutting_down:
            event.accept()
            return
        behavior=self.settings['close_behavior']
        if behavior=='Exit':
            event.ignore()
            self.exit_app()
        elif behavior=='Hide to tray' and QSystemTrayIcon.isSystemTrayAvailable():
            self.hide()
            event.ignore()
        else:
            self.showMinimized()
            event.ignore()

    def attach_local_server(self):
        if self.model_process or self.attached_server or self.shutting_down:
            return
        expected=Path(self.settings.get('server','')).resolve()
        model=self.settings.get('model','')
        if not model:
            return
        for process in psutil.process_iter(['exe','cmdline']):
            try:
                if not process.info['exe'] or Path(process.info['exe']).resolve()!=expected:
                    continue
                args=process.info['cmdline'] or []
                matching=any(args[i] in ('-m','--model') and os.path.normcase(str(Path(args[i+1]).resolve()))==os.path.normcase(str(Path(model).resolve())) for i in range(len(args)-1))
                if matching and any(connection.laddr.port==8787 and connection.status==psutil.CONN_LISTEN for connection in process.net_connections(kind='tcp')):
                    self.attached_server=process
                    return
            except (psutil.Error,OSError):
                pass

    def cleanup(self):
        if self.shutting_down:
            return
        self.shutting_down=True
        process_lifecycle.close()
        self.clock.stop()
        self.process_timer.stop()
        self.icon_timer.stop()
        self.engine.cancel.set()
        self.engine.paused.clear()
        self.download_cancel.set()
        if self.mcp:self.mcp.stop();self.mcp=None
        self.hotkey.unregister()
        self.stop_model()
        if self.attached_server:
            try:
                self.attached_server.terminate()
                self.attached_server.wait(timeout=3)
            except psutil.TimeoutExpired:
                try: self.attached_server.kill()
                except psutil.Error: pass
            except psutil.Error:
                pass
            self.attached_server=None
        # Decoder, OCR, setup and helper processes belong to this app's process tree.
        children=psutil.Process().children(recursive=True)
        for child in children:
            try: child.terminate()
            except psutil.Error: pass
        _,alive=psutil.wait_procs(children,timeout=3)
        for child in alive:
            try: child.kill()
            except psutil.Error: pass
        self.tray.hide()
        self.quick.hide()
        self.logger.removeHandler(self.log_handler)
        self.logger.removeHandler(self.log_file_handler)
        self.log_file_handler.close()

    def exit_app(self):
        self.cleanup()
        self.close()
        QApplication.instance().quit()


def main():
    app=QApplication(sys.argv)
    app.setApplicationName('VectorDash')
    app.setOrganizationName('VectorDash')
    app.setStyle('Fusion')
    app.setQuitOnLastWindowClosed(False)
    if '--smoke-test' in sys.argv:
        from packaged_smoke import run_smoke
        return run_smoke(app)
    settings=Settings()
    name='VectorDash-'+hashlib.sha256(str(settings.home.resolve()).encode()).hexdigest()[:16]
    socket=QLocalSocket()
    socket.connectToServer(name)
    if socket.waitForConnected(400):
        socket.write(b'startup' if '--startup' in sys.argv else b'show')
        socket.waitForBytesWritten(500)
        socket.disconnectFromServer()
        return 0
    QLocalServer.removeServer(name)
    server=QLocalServer()
    if not server.listen(name):
        QMessageBox.critical(None,'VectorDash','Could not establish single-instance communication.')
        return 1
    window=Desktop(settings,server)
    hidden='--startup' in sys.argv and settings.values['start_hidden'] and QSystemTrayIcon.isSystemTrayAvailable()
    if not hidden:
        window.show()
    return app.exec()

if __name__=='__main__':
    raise SystemExit(main())
