from PyQt6.QtWidgets import (
    QVBoxLayout, QHBoxLayout, QLabel, QProgressBar, QWidget, QSizePolicy,
    QPushButton, QMainWindow, QSplitter, QTabWidget, QScrollArea, QApplication
)
from PyQt6.QtGui import QFont, QPixmap, QIcon
from PyQt6.QtCore import Qt, pyqtSignal
import sys
import time


class ProgressBarWidget(QWidget):
    """
    Widget personnalisé pour une barre de progression mise à jour dynamiquement par des signaux.
    """

    # Signal pour indiquer que la progression a atteint 100 %
    progress_completed_signal = pyqtSignal(str, str)  # (prefix_text, suffix_text)

    def __init__(self, update_progress_signal, update_total_signal, update_prefix_signal, update_item_type_signal,
                 parent=None):
        super().__init__(parent)

        # Layout principal pour organiser les composants de la barre
        layout = QHBoxLayout()
        layout.setContentsMargins(0, 0, 0, 0)  # Pas de marges intérieures
        layout.setSpacing(5)  # Espacement entre les éléments

        # Préfixe de la barre de progression
        self.progress_prefix = QLabel("Début...")
        self.progress_prefix.setFont(QFont("Arial", 12))  # Taille de texte ajustée à 12
        self.progress_prefix.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)
        layout.addWidget(self.progress_prefix)

        # Barre de progression
        self.progress_bar = QProgressBar()
        self.progress_bar.setMinimum(0)
        self.progress_bar.setMaximum(100)
        self.progress_bar.setValue(0)
        self.progress_bar.setTextVisible(False)  # Texte désactivé pour la barre elle-même
        self.progress_bar.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)

        # Stylesheet : rendre la barre épaisse
        self.progress_bar.setStyleSheet("""
            QProgressBar {
                height: 24px;  /* Hauteur ajustée à celle du texte */
                border: 1px solid #000;
                border-radius: 4px;  /* Coins arrondis */
                background: #e0e0e0;  /* Couleur de fond (gris clair) */
            }
            QProgressBar::chunk {
                background-color: #3b8dff;  /* Couleur de progression (bleu) */
                border-radius: 4px;
            }
        """)
        layout.addWidget(self.progress_bar)

        # Suffixe contenant les statistiques
        self.progress_suffix = QLabel("0.00%")
        self.progress_suffix.setFont(QFont("Arial", 12))  # Taille de texte ajustée à 12
        self.progress_suffix.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        self.progress_suffix.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)
        layout.addWidget(self.progress_suffix)

        # Appliquer le layout au widget
        self.setLayout(layout)

        # Variables pour gérer l’état
        self.total_items = 100  # Valeur par défaut
        self.start_time = time.time()
        self.item_type = "items"

        # Connexion des signaux
        update_progress_signal.connect(self.update_progress_bar)
        update_total_signal.connect(self.update_total_items)
        update_prefix_signal.connect(self.update_progress_prefix)
        update_item_type_signal.connect(self.update_item_type)

    def update_item_type(self, item_type):
        """Met à jour dynamiquement l'unité des items pour l'affichage."""
        self.item_type = item_type

    def update_progress_prefix(self, prefix_text):
        """Met à jour dynamiquement le texte du préfixe."""
        self.progress_prefix.setText(prefix_text)

    def update_progress_bar(self, progress):
        """Met à jour la valeur de la barre et les statistiques affichées."""
        self.progress_bar.setValue(progress)

        # Calcul des progrès (pourcentage, temps estimé, vitesse)
        progress_percent = (progress / self.total_items) * 100 if self.total_items > 0 else 0
        elapsed_time = time.time() - self.start_time
        items_per_second = progress / elapsed_time if elapsed_time > 0 else 0
        remaining_items = self.total_items - progress
        estimated_time_left = remaining_items / items_per_second if items_per_second > 0 else 0

        # Mettre à jour le suffixe de progression
        self.progress_suffix.setText(
            f"{progress_percent:.2f}% | {progress}/{self.total_items} {self.item_type} "
            f"[{elapsed_time:.2f}s < {estimated_time_left:.2f}s, {items_per_second:.2f} {self.item_type}/s]"
        )

        # Vérifier si la barre atteint 100 %
        if progress >= self.total_items:
            # Envoyer un signal vers la ProgressWindow pour ajouter un rapport
            self.progress_completed_signal.emit(self.progress_prefix.text(), self.progress_suffix.text())

    def update_total_items(self, total, completed=0):
        """Met à jour le nombre total d'items et réinitialise si nécessaire."""
        self.total_items = total
        self.progress_bar.setMaximum(total)
        self.update_progress_bar(completed)


class ProgressWindow(QMainWindow):
    """
    Fenêtre principale avec un onglet Progress (en bas) et un onglet dynamique Report (en haut).
    """

    # Signaux PyQt pour mettre à jour la barre de progression
    update_progress_signal = pyqtSignal(int)  # Progrès actuel
    update_total_signal = pyqtSignal(int, int)  # Total et étape actuelle
    update_prefix_signal = pyqtSignal(str)  # Texte de préfixe
    update_item_type_signal = pyqtSignal(str)  # Type d'items (fichiers, étapes, etc.)

    def __init__(self, parent=None):
        super().__init__(parent)

        # Titre et icône de la fenêtre
        self.setWindowTitle("FragHub 1.2.0")
        self.setWindowIcon(QIcon("./GUI/assets/FragHub_icon.png"))
        self.setGeometry(100, 100, 1280, 720)

        # Ajout de l'icône en haut
        banner = QLabel()
        pixmap = QPixmap("./GUI/assets/FragHub_icon.png").scaled(
            130, 130, Qt.AspectRatioMode.KeepAspectRatio, Qt.TransformationMode.SmoothTransformation
        )
        banner.setPixmap(pixmap)
        banner.setAlignment(Qt.AlignmentFlag.AlignCenter)

        # Splitter pour diviser l'espace entre les deux widgets
        splitter = QSplitter(Qt.Orientation.Vertical)

        # Premier QTabWidget (haut) avec un onglet "Report"
        self.top_tab_widget = QTabWidget()

        # Création de l'onglet "Report"
        self.report_tab = QWidget()
        self.report_layout = QVBoxLayout()
        self.report_widget = QWidget()
        self.report_content = QVBoxLayout()
        self.report_widget.setLayout(self.report_content)

        # Ajouter un espace extensible pour gérer le contenu dynamique
        self.report_content.addStretch()

        # Régler la taille minimum pour activer le scroll
        self.report_widget.setMinimumWidth(1)

        # Ajout d'une zone défilable pour le rapport
        self.report_scroll = QScrollArea()
        self.report_scroll.setWidgetResizable(True)
        self.report_scroll.setWidget(self.report_widget)
        self.report_layout.addWidget(self.report_scroll)

        self.report_tab.setLayout(self.report_layout)
        self.top_tab_widget.addTab(self.report_tab, "Report")

        splitter.addWidget(self.top_tab_widget)

        # Onglet "Progress" (en bas)
        self.bottom_tab_widget = QTabWidget()
        self.progress_tab = QWidget()
        self.progress_layout = QVBoxLayout()

        # Widget de barre de progression
        self.progress_bar_widget = ProgressBarWidget(
            self.update_progress_signal,
            self.update_total_signal,
            self.update_prefix_signal,
            self.update_item_type_signal
        )
        self.progress_bar_widget.progress_completed_signal.connect(self.add_to_report)  # Connexion au rapport
        self.progress_layout.addWidget(self.progress_bar_widget)

        self.progress_tab.setLayout(self.progress_layout)
        self.bottom_tab_widget.addTab(self.progress_tab, "Progress")

        splitter.addWidget(self.bottom_tab_widget)

        # Configurer les tailles du splitter
        self.bottom_tab_widget.setMinimumHeight(60)
        splitter.setStretchFactor(0, 3)
        splitter.setStretchFactor(1, 0)

        # Layout principal
        main_layout = QVBoxLayout()
        main_layout.addWidget(banner)
        main_layout.addWidget(splitter)

        # Bouton STOP
        self.stop_button = QPushButton("STOP")
        self.stop_button.setFixedSize(120, 40)
        self.stop_button.setStyleSheet("background-color: red; color: white; font-weight: bold; font-size: 12px;")
        self.stop_button.clicked.connect(QApplication.quit)  # Quitte entièrement le programme

        # Centrer le bouton
        button_layout = QHBoxLayout()
        button_layout.addStretch()
        button_layout.addWidget(self.stop_button)
        button_layout.addStretch()
        main_layout.addLayout(button_layout)

        # Appliquer le layout principal
        container = QWidget()
        container.setLayout(main_layout)
        self.setCentralWidget(container)

    def add_to_report(self, prefix_text, suffix_text):
        """
        Ajoute le texte du résumé (prefix + suffix) à l'onglet "Report".
        """
        new_entry = QLabel(f"{prefix_text} - {suffix_text}")
        new_entry.setFont(QFont("Arial", 10))
        new_entry.setAlignment(Qt.AlignmentFlag.AlignCenter)

        # Ajouter avant l'espace extensible
        self.report_content.insertWidget(self.report_content.count() - 1, new_entry)

        # Forcer le scroll vers le bas
        self.report_scroll.verticalScrollBar().setValue(
            self.report_scroll.verticalScrollBar().maximum()
        )