from PyQt6.QtWidgets import QWidget, QVBoxLayout, QPushButton, QSpacerItem, QSizePolicy, QLabel, QHBoxLayout
from PyQt6.QtGui import QFont, QIcon
from PyQt6.QtCore import QSize, Qt, pyqtSignal
from PyQt6.QtWidgets import QFileDialog
from scripts.GUI.utils.global_vars import parameters_dict  # Import the global dictionary
import sys
import os

# If the file is executed as a PyInstaller executable
if getattr(sys, 'frozen', False):
    BASE_DIR = sys._MEIPASS
else:
    # If the file is executed as a Python script
    BASE_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))

class OutputTab(QWidget):
    output_directory_changed = pyqtSignal(str)

    def __init__(self):
        super().__init__()
        self.layout = QVBoxLayout()

        # Add spacers for vertical centering
        self.layout.addSpacerItem(QSpacerItem(20, 40, QSizePolicy.Policy.Minimum, QSizePolicy.Policy.Expanding))

        # Create the directory selection button
        button = QPushButton()
        button.setIcon(QIcon(os.path.join(BASE_DIR,'./GUI/assets/directory.png')))
        button.setIconSize(QSize(128, 128))
        button.setFixedSize(140, 140)
        button.clicked.connect(self.browse_output_files)  # Connect the button to the function

        # Center the button
        button_layout = QHBoxLayout()
        button_layout.addWidget(button, alignment=Qt.AlignmentFlag.AlignCenter)
        self.layout.addLayout(button_layout)

        # Add the label below the button
        label = QLabel("Select output directory")
        label.setFont(QFont("Arial", 14, QFont.Weight.Bold))
        label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.layout.addWidget(label)

        # --- ADDITION ---
        # Label to display the selected folder path
        self.path_label = QLabel("No directory selected")
        self.path_label.setFont(QFont("Arial", 10))
        self.path_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.path_label.setWordWrap(True)  # Allows the path to wrap to multiple lines if needed
        self.layout.addWidget(self.path_label)
        # --- END OF ADDITION ---

        # Spacing and info button
        self.layout.addSpacerItem(QSpacerItem(20, 40, QSizePolicy.Policy.Minimum, QSizePolicy.Policy.Expanding))

        info_button_layout = QHBoxLayout()
        info_button_layout.addSpacerItem(QSpacerItem(40, 20, QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum))

        info_button = QPushButton("🛈")
        info_button.setFixedSize(30, 30)
        info_button.setToolTip("Create a new empty directory or Select an existing directory where FragHub has already written files")
        info_button_layout.addWidget(info_button, alignment=Qt.AlignmentFlag.AlignRight)
        self.layout.addLayout(info_button_layout)

        self.setLayout(self.layout)

    def browse_output_files(self):
        """Directory selection handler for the OUTPUT tab."""
        start_directory = os.path.abspath(os.sep)
        directory = QFileDialog.getExistingDirectory(
            self,
            "Choose a directory for OUTPUT",
            start_directory
        )
        if directory:
            parameters_dict["output_directory"] = directory
            self.output_directory_changed.emit(directory)

            # --- ADDITION ---
            # Update the label with the selected path
            self.path_label.setText(directory)
            # --- END OF ADDITION ---