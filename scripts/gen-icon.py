#!/usr/bin/env python3
"""Render the Kestrel SVG icon to a 256px PNG using headless Chromium."""
import base64, subprocess, sys, pathlib

svg = pathlib.Path("assets/icons/io.kestrel.Browser.svg").read_text()
html = f"<!doctype html><body style='margin:0'><img style='width:256px;height:256px;display:block' src='data:image/svg+xml;base64,{base64.b64encode(svg.encode()).decode()}'></body>"
pathlib.Path("/tmp/icon.html").write_text(html)

# Playwright chromium headless screenshot
import glob
chromes = glob.glob("/home/z/.cache/ms-playwright/chromium-*/chrome-linux/chrome") + \
          glob.glob("/home/z/.cache/ms-playwright/chromium-*/chrome-linux64/chrome") + \
          glob.glob("/home/z/.agent-browser/browsers/chrome-*/chrome-linux64/chrome")
if not chromes:
    print("no chromium found", file=sys.stderr); sys.exit(1)
chrome = sorted(chromes)[-1]
subprocess.run([
    chrome, "--headless", "--no-sandbox", "--disable-gpu",
    "--screenshot=assets/icons/io.kestrel.Browser.png",
    "--window-size=256,256", "file:///tmp/icon.html"
], check=True, capture_output=True)
print("icon PNG written")
