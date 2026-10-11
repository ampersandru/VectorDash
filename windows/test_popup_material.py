import unittest
from unittest.mock import Mock
from PySide6.QtWidgets import QApplication,QWidget,QLineEdit
from PySide6.QtCore import Qt,QEvent,QPointF
from PySide6.QtGui import QMouseEvent
from desktop import QuickSearch,ScrollSafeComboBox

class PopupMaterialTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.app=QApplication.instance() or QApplication([])

    def popup(self):
        owner=QWidget();owner.settings={'theme':'Dark','popup_transparency':20,'popup_gradient':20}
        owner.open_file=Mock();owner.preview_result=Mock();owner.request_search=Mock();owner.search_sequence=0;owner.pending_search='old';owner.shutting_down=False
        def controls(layout):
            category=ScrollSafeComboBox();category.addItems(['All files','Documents'])
            excludes=QLineEdit();layout.addWidget(category);layout.addWidget(excludes);return category,excludes
        owner.filter_controls=controls
        quick=QuickSearch(owner)
        self.addCleanup(owner.close);self.addCleanup(quick.close)
        return owner,quick

    def test_empty_popup_collapses_and_query_expands(self):
        owner,quick=self.popup()
        self.assertTrue(quick.split.isHidden());self.assertTrue(quick.footer.isHidden());self.assertLess(quick.height(),260)
        quick.input.setText('report');quick.timer.stop()
        self.assertFalse(quick.split.isHidden());self.assertFalse(quick.footer.isHidden());self.assertEqual(quick.height(),570)
        quick.input.clear()
        self.assertTrue(quick.split.isHidden());self.assertLess(quick.height(),260)
        self.assertIsNone(owner.pending_search);self.assertFalse(quick.timer.isActive())

    def test_paint_opacity_changes_and_does_not_accumulate(self):
        owner,quick=self.popup();quick.input.setText('report');quick.timer.stop()
        first=quick.grab().toImage().pixelColor(10,200).alpha()
        owner.settings['popup_transparency']=80
        second=quick.grab().toImage().pixelColor(10,200).alpha()
        self.assertGreater(first,second+100)
        for _ in range(5):
            quick.update();self.app.processEvents()
        self.assertEqual(second,quick.grab().toImage().pixelColor(10,200).alpha())

    def test_dropdown_hover_tracks_row_and_clears_on_leave(self):
        combo=ScrollSafeComboBox();combo.addItems(['All','Photos','Documents']);combo.resize(200,40);combo.show();combo.showPopup();self.app.processEvents()
        view=combo.view();point=QPointF(view.visualRect(view.model().index(2,0)).center())
        event=QMouseEvent(QEvent.Type.MouseMove,point,point,Qt.MouseButton.NoButton,Qt.MouseButton.NoButton,Qt.KeyboardModifier.NoModifier)
        QApplication.sendEvent(view.viewport(),event)
        self.assertEqual(view.property('hoverRow'),2)
        QApplication.sendEvent(view.viewport(),QEvent(QEvent.Type.Leave))
        self.assertEqual(view.property('hoverRow'),-1)
        combo.hidePopup();combo.close()

if __name__=='__main__':unittest.main()
