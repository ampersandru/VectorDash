"""Integrated window controls with native Windows dragging and edge resizing."""
import ctypes
from ctypes import wintypes
import os
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QFrame,QHBoxLayout,QPushButton
from appearance import icon

class WindowBar(QFrame):
    def __init__(self,owner):
        super().__init__(owner);self.owner=owner;self.custom_maximized=False;self.restore_geometry=None;self.setObjectName('windowBar');self.setFixedHeight(34)
        layout=QHBoxLayout(self);layout.setContentsMargins(8,0,8,0);layout.setSpacing(4);layout.addStretch()
        self.controls=[]
        for name,tip,action in [('window-minimize','Minimize',owner.showMinimized),('window-maximize','Maximize / restore',self.toggle_maximized),('close','Close',owner.close)]:
            control=QPushButton();control.setObjectName('windowControl');control.setToolTip(tip);control.setAccessibleName(tip);control.setFixedSize(38,28)
            control.clicked.connect(action);layout.addWidget(control);self.controls.append((control,name))
    def recolor(self,tint):
        for button,name in self.controls:button.setIcon(icon(name,tint))
    def toggle_maximized(self):
        if self.custom_maximized:
            self.custom_maximized=False
            if self.restore_geometry is not None:self.owner.setGeometry(self.restore_geometry)
        elif self.owner.isMaximized():
            self.owner.showNormal()
        else:
            self.restore_geometry=self.owner.geometry()
            self.custom_maximized=True
            self.owner.setGeometry(self.owner.screen().availableGeometry())
    def mousePressEvent(self,event):
        if event.button()==Qt.MouseButton.LeftButton:
            if self.custom_maximized:self.toggle_maximized()
            self.owner.windowHandle().startSystemMove()
        super().mousePressEvent(event)
    def mouseDoubleClickEvent(self,event):
        if event.button()==Qt.MouseButton.LeftButton:self.toggle_maximized()

def resize_hit(owner,message):
    if os.name!='nt' or getattr(getattr(owner,'window_bar',None),'custom_maximized',False):return None
    msg=wintypes.MSG.from_address(int(message))
    if msg.message!=0x84:return None
    # Avoid Qt state queries inside nativeEvent; they can re-enter message handling.
    user=ctypes.windll.user32
    user.IsZoomed.argtypes=[wintypes.HWND]
    if user.IsZoomed(msg.hWnd):return None
    rect=wintypes.RECT()
    user.GetWindowRect.argtypes=[wintypes.HWND,ctypes.POINTER(wintypes.RECT)]
    if not user.GetWindowRect(msg.hWnd,ctypes.byref(rect)):return None
    user.GetDpiForWindow.argtypes=[wintypes.HWND]
    edge=round(7*(user.GetDpiForWindow(msg.hWnd) or 96)/96)
    x=ctypes.c_short(msg.lParam&0xffff).value;y=ctypes.c_short((msg.lParam>>16)&0xffff).value
    left=x<rect.left+edge;right=x>=rect.right-edge;top=y<rect.top+edge;bottom=y>=rect.bottom-edge
    if top:return 13 if left else 14 if right else 12
    if bottom:return 16 if left else 17 if right else 15
    return 10 if left else 11 if right else None
