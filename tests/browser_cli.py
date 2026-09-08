#!/usr/bin/env python3
"""End-to-end smoke test with no Python dependencies: --browser PATH [--aditor PATH].
Uses headless Chrome with a temporary profile; does not access the user's browser/profile.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument('--browser', required=True)
parser.add_argument('--aditor', default='target/debug/aditor')
args = parser.parse_args()
aditor = str(Path(args.aditor).resolve())

with tempfile.TemporaryDirectory(prefix='aditor-cli-test-') as directory:
    root = Path(directory)
    profile = root / 'profile'
    env = dict(os.environ, XDG_CACHE_HOME=str(root / 'cache'))
    with (root / 'chrome.log').open('w') as log:
        browser = subprocess.Popen([args.browser, '--headless=new', '--no-first-run',
            '--no-default-browser-check', '--disable-background-networking',
            '--remote-debugging-port=0', f'--user-data-dir={profile}',
            '--window-size=800,600', 'about:blank'], stdout=log, stderr=log)
        active = []
        try:
            deadline = time.monotonic() + 20
            port_file = profile / 'DevToolsActivePort'
            while not port_file.exists():
                if browser.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError((root / 'chrome.log').read_text())
                time.sleep(.1)
            port = port_file.read_text().splitlines()[0]

            def run(*command, ok=True):
                result = subprocess.run([aditor, *command], env=env, text=True,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=40)
                if (result.returncode == 0) != ok:
                    logs = {p.name: p.read_text()[-5000:] for p in (root / 'cache' / 'aditor' / 'rec').glob('*.log')}
                    raise AssertionError((command, result.stdout, result.stderr, logs))
                return json.loads(result.stdout) if ok else result

            def page(title, color, content=""):
                html = f'<title>Loading</title><body style="margin:0;background:{color}">{content}<script>document.title={json.dumps(title)}</script></body>'
                url = 'data:text/html,' + urllib.parse.quote(html)
                request = urllib.request.Request(f'http://127.0.0.1:{port}/json/new?{urllib.parse.quote(url, safe="")}', method='PUT')
                with urllib.request.urlopen(request, timeout=5) as response:
                    target = json.load(response)['id']
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    with urllib.request.urlopen(f'http://127.0.0.1:{port}/json/list', timeout=5) as response:
                        ready = any(t['id'] == target and t.get('title') == title for t in json.load(response))
                    if ready:
                        return target
                    time.sleep(.05)
                raise RuntimeError('test page did not finish loading')

            red = page('Aditor red target', 'red')
            page('Aditor blue foreground', 'blue')
            tabs = run('tabs', '--cdp-port', port, '--json')
            assert any(t['id'] == red and t['title'] == 'Aditor red target' for t in tabs)
            png = root / 'tab.png'
            shot = run('screenshot', '--tab', red, '--cdp-port', port, '-o', str(png), '--json')
            assert shot['format'] == 'png' and png.read_bytes().startswith(b'\x89PNG\r\n\x1a\n')
            # Verify that the frame is from the red tab, even with another tab opened afterward.
            pixel = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(png),
                '-vf', 'scale=1:1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'])
            assert pixel[0] > 240 and pixel[1] < 15 and pixel[2] < 15, pixel
            run('screenshot', '--tab', red, '--cdp-port', port, '-o', str(png), '--json', ok=False)
            run('print', '--tab', red, '--cdp-port', port, '-o', str(png), '--yes', '--json')
            video = root / 'timed.mp4'
            result = run('record', '--tab', red, '--cdp-port', port, '--duration', '1.5',
                '--fps', '10', '--codec', 'h264-sw', '-o', str(video), '--json')
            assert 1.4 <= result['duration'] <= 1.7, result
            assert result['size_bytes'] > 0
            pixel = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(video),
                '-frames:v', '1', '-vf', 'scale=1:1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'])
            assert pixel[0] > 230 and pixel[1] < 20 and pixel[2] < 20, pixel
            session = run('record', '--tab', red, '--cdp-port', port, '--fps', '10',
                '--codec', 'h264-sw', '-o', str(root / 'background.mp4'), '--json')
            active.append(session['id'])
            time.sleep(.3)
            result = run('stop', session['id'], '--json')
            active.remove(session['id'])
            assert result[0]['duration'] >= 1 and result[0]['size_bytes'] > 0, result
            run('screenshot', '--tab', 'missing', '--cdp-port', port, '-o', str(root / 'missing.png'), '--json', ok=False)
            assert not (root / 'missing.png').exists()
            run('record', '--tab', 'missing', '--cdp-port', port, '-o', str(root / 'bad-target.mp4'), '--json', ok=False)
            run('record', '--tab', red, '--cdp-port', port, '--codec', 'h264-sw', '--preset', 'invalid-preset',
                '-o', str(root / 'bad-encoder.mp4'), '--json', ok=False)
            assert not list((root / 'cache' / 'aditor' / 'rec').glob('*.json'))
            run('record', '--tab', red, '--audio', '--dry-run', '--json', ok=False)
            run('record', '--tab', red, '--screen', '0', '--dry-run', '--json', ok=False)
            run('record', '--duration', 'NaN', '--dry-run', '--json', ok=False)
            plan = run('screenshot', '--tab', 'offline-id', '--cdp-port', '1',
                '-o', str(root / 'not-created' / 'dry.png'), '--dry-run', '--json')
            assert plan['dry_run'] and not (root / 'not-created').exists()
            # Crop outside the viewport, with animated content inside a fixed container.
            element = page('Element capture', 'blue', """<style>
                #target {position:absolute;top:1200px;left:20px;width:200px;height:100px;background:lime;overflow:hidden} #target span {display:block;width:10px;height:10px;background:#00f000;animation:move .4s linear infinite alternate}
                @keyframes move {to {transform:translateX(180px)}}
                </style><div id="target" data-label="it's quoted"><span></span></div>
                <div class="many"></div><div class="many"></div><div id="hidden" style="display:none"></div>""")
            selector = "[data-label=\"it's quoted\"]"
            crop = root / 'element.png'
            result = run('print', '--tab', element, '--cdp-port', port, '--selector', selector, '-o', str(crop), '--json')
            assert result['selector'] == selector

            def assert_element(file, video=False):
                meta = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-select_streams', 'v:0',
                    '-show_entries', 'stream=width,height', '-of', 'json', str(file)]))['streams'][0]
                assert (meta['width'], meta['height']) == (200, 100), meta
                pixels = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(file),
                    '-vf', 'scale=1:1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'])
                assert pixels and (not video or len(pixels) >= 9)
                for i in range(0, len(pixels), 3):
                    r, g, b = pixels[i:i+3]
                    assert r < 25 and g > 230 and b < 25, (file, r, g, b)

            assert_element(crop)
            result = run('record', '--tab', element, '--cdp-port', port, '--selector', '#target',
                '--duration', '1', '--fps', '5', '--codec', 'h264-sw', '-o', str(root / 'element.mp4'), '--json')
            assert_element(root / 'element.mp4', video=True)
            session = run('record', '--tab', element, '--cdp-port', port, '--selector', '#target',
                '--fps', '5', '--codec', 'h264-sw', '-o', str(root / 'element-bg.mp4'), '--json')
            active.append(session['id'])
            run('stop', session['id'], '--json')
            active.remove(session['id'])
            assert_element(root / 'element-bg.mp4', video=True)
            for bad in ['#missing', '.many', '#hidden', '[', "#x'); throw new Error('injection');//"]:
                error = run('screenshot', '--tab', element, '--cdp-port', port, '--selector', bad,
                    '-o', str(root / 'bad-selector.png'), '--json', ok=False)
                assert '--selector' in error.stderr
                assert not (root / 'bad-selector.png').exists()
            run('record', '--tab', element, '--cdp-port', port, '--selector', '#missing',
                '-o', str(root / 'bad-selector.mp4'), '--json', ok=False)
            assert not list((root / 'cache' / 'aditor' / 'rec').glob('*.json'))
            plan = run('screenshot', '--tab', 'offline', '--selector', '#target', '--cdp-port', '1',
                '-o', str(root / 'plan.png'), '--dry-run', '--json')
            assert plan['selector'] == '#target' and not (root / 'plan.png').exists()

            resized = page('Resize during recording', 'blue', """<div id="resize" style="width:200px;height:100px;background:lime"></div>
                <script>setTimeout(()=>document.getElementById('resize').style.width='220px',1800)</script>""")
            error = run('record', '--tab', resized, '--cdp-port', port, '--selector', '#resize', '--duration', '3',
                '--fps', '5', '--codec', 'h264-sw', '-o', str(root / 'resized.mp4'), '--json', ok=False)
            assert 'changed size' in error.stderr, error.stderr
            assert_element(root / 'resized.mp4', video=True)

            # Closing the tab produces an error; stop cleans up the ended session without reporting success.
            broken = run('record', '--tab', red, '--cdp-port', port, '--fps', '10',
                '--codec', 'h264-sw', '-o', str(root / 'closed.mp4'), '--json')
            active.append(broken['id'])
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/json/close/{red}', timeout=5) as response:
                response.read()
            done = root / 'cache' / 'aditor' / 'rec' / f"{broken['id']}.done"
            deadline = time.monotonic() + 20
            while not done.exists() and time.monotonic() < deadline:
                time.sleep(.1)
            assert done.exists(), 'worker did not detect the closed tab'
            run('stop', broken['id'], '--json', ok=False)
            active.remove(broken['id'])
            assert not list((root / 'cache' / 'aditor' / 'rec').glob('*.json'))
            print('PASS: tab listing, isolated tab, PNG, overwrite, timed MP4, background/stop, selector PNG/video, errors, and dry-run')
        finally:
            for session in active:
                subprocess.run([aditor, 'stop', session, '--json'], env=env,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=40)
            browser.terminate()
            try:
                browser.wait(timeout=10)
            except subprocess.TimeoutExpired:
                browser.kill()
                browser.wait()
