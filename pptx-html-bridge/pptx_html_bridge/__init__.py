"""
PPTX to HTML Bridge

A library for converting PowerPoint (.pptx) files to HTML format.
"""

from .converter import (
    PPTXToHTMLConverter,
    convert_pptx_to_html,
    convert_pptx_directory,
    main
)

__version__ = "0.1.0"
__all__ = [
    "PPTXToHTMLConverter",
    "convert_pptx_to_html",
    "convert_pptx_directory",
    "main"
]