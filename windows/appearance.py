"""Shared accent/material colors and Material Design icons."""
from pathlib import Path
from PySide6.QtCore import QByteArray,Qt
from PySide6.QtGui import QColor,QIcon,QPixmap,QPainter
from PySide6.QtSvg import QSvgRenderer

def color(value,fallback='#8573ff'):
    parsed=QColor(value)
    return parsed if parsed.isValid() else QColor(fallback)

def icon(name,tint='#8573ff'):
    svg=(Path(__file__).parent/'assets/mdi'/(name+'.svg')).read_text().replace('<path ',f'<path fill="{color(tint).name()}" ')
    renderer=QSvgRenderer(QByteArray(svg.encode()))
    pixels=QPixmap(24,24);pixels.fill(Qt.GlobalColor.transparent)
    painter=QPainter(pixels);renderer.render(painter);painter.end()
    return QIcon(pixels)

def popup_colors(settings,dark):
    tone=color(settings.get('popup_tone','#8a73a8'),'#8a73a8')
    h,s,_=tone.getHslF()[:3]
    strength=max(0,min(100,int(settings.get('popup_gradient',20))))/100
    transparency=max(0,min(90,int(settings.get('popup_transparency',25))))
    alpha=round(255*(100-transparency)/100)
    # Default material retains the existing violet appearance.
    if tone.name()=='#8a73a8':
        first=QColor(42,27,53) if dark else QColor(255,252,255)
        target=QColor(62,32,93) if dark else QColor(175,135,240)
    else:
        first=QColor.fromHslF(h,min(.55,s),.16 if dark else .98)
        target=QColor.fromHslF(h,max(.3,min(.7,s)),.25 if dark else .78)
    last=QColor.fromRgbF(*[first.getRgbF()[i]*(1-strength)+target.getRgbF()[i]*strength for i in range(3)])
    first.setAlpha(alpha);last.setAlpha(alpha)
    return first,last
