from setuptools import setup, find_packages

with open("README.md", "r", encoding="utf-8") as fh:
    long_description = fh.read()

setup(
    name="pptx-html-bridge",
    version="0.1.0",
    author="Liyulingyue",
    author_email="",
    description="Convert PPTX files to HTML",
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="https://github.com/Liyulingyue/pptx-html-bridge",
    packages=find_packages(),
    classifiers=[
        "Programming Language :: Python :: 3",
        "License :: OSI Approved :: MIT License",
        "Operating System :: OS Independent",
    ],
    python_requires=">=3.6",
    install_requires=[
        "python-pptx",
        "lxml",
    ],
    entry_points={
        "console_scripts": [
            "pptx-to-html=pptx_html_bridge.converter:main",
        ],
    },
)