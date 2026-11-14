"""Screenshot of the isochrone demo with the streets of Recife, cycling, from a
point by Praça do Derby.

    python3 site/figures/demo_screenshot.py SITE_DIR OUT.png

SITE_DIR is the built site (_site/, from site/build.sh), served here on a
local port. Needs playwright with Chromium."""

import functools
import http.server
import sys
import threading

from playwright.sync_api import sync_playwright

site, out = sys.argv[1], sys.argv[2]
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=site)
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
url = f"http://127.0.0.1:{server.server_address[1]}/demo/"
done = "document.querySelector('#status').textContent.includes('computed')"

with sync_playwright() as p:
    browser = p.chromium.launch(channel="chrome")
    page = browser.new_page(viewport={"width": 1100, "height": 900}, color_scheme="light")
    page.goto(url)
    page.wait_for_function(done, timeout=60_000)
    page.select_option("#city", "recife")
    page.wait_for_function(
        f"{done} && document.querySelector('#status').textContent.includes('-8.')",
        timeout=120_000,
    )
    page.check("input[value='BICYCLE']", force=True)
    page.wait_for_function(f"{done} && document.querySelector('#status').textContent.includes('bicycle')", timeout=60_000)
    box = page.locator("#map").bounding_box()
    page.mouse.move(box["x"] + box["width"] * 0.62, box["y"] + box["height"] * 0.52)
    page.mouse.wheel(0, -500)
    page.wait_for_timeout(2500)
    page.mouse.click(box["x"] + box["width"] * 0.62, box["y"] + box["height"] * 0.52)
    page.wait_for_timeout(3000)
    print(page.text_content("#status"))
    page.locator(".map-wrap").screenshot(path=out)
    browser.close()
server.shutdown()
