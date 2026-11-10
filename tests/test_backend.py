"""End-to-end tests against the real backend and TUI; Python standard library only.
Run `cargo build` first, then `python3 -m unittest discover -s tests -v`.
"""
import concurrent.futures
import json
import os
from pathlib import Path
import signal
import socket
import sqlite3
import subprocess
import tempfile
import time
import unittest
import urllib.error
import urllib.request

BINARY = Path(__file__).resolve().parents[1] / 'target/debug/tama-buddy'


class BackendTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='tama-backend-')
        self.addCleanup(self.directory.cleanup)
        self.db = Path(self.directory.name) / 'pet.sqlite3'
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            self.port = sock.getsockname()[1]
        self.url = f'http://127.0.0.1:{self.port}'
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        self.proc = None
        self.addCleanup(self.stop)
        self.start()

    def start(self):
        self.log = tempfile.TemporaryFile()
        self.addCleanup(self.log.close)
        self.proc = subprocess.Popen(
            [str(BINARY), '--server', '--db', str(self.db), '--bind', f'127.0.0.1:{self.port}'],
            stdin=subprocess.DEVNULL, stdout=self.log, stderr=self.log,
        )
        for _ in range(100):
            if self.proc.poll() is not None:
                self.log.seek(0)
                self.fail(self.log.read().decode())
            try:
                if self.request('/health')[0] == 200:
                    return
            except (OSError, urllib.error.URLError):
                time.sleep(0.05)
        self.fail('Backend did not become ready')

    def stop(self):
        if self.proc and self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
                self.fail('Backend failed to stop gracefully')
            self.assertEqual(self.proc.returncode, 0)

    def request(self, path, body=None, raw=None):
        data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
        req = urllib.request.Request(self.url + path, data=data,
                                     headers={'Content-Type': 'application/json'})
        try:
            response = self.opener.open(req, timeout=3)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            return response.status, json.load(response)

    def test_embedded_web_assets(self):
        total = 0
        for path, content_type, expected in [
            ('/', 'text/html', b'<title>Tama Buddy'),
            ('/assets/app.css', 'text/css', b'prefers-reduced-motion'),
            ('/assets/app.js', 'text/javascript', b'/api/pet/actions'),
            ('/favicon.svg', 'image/svg+xml', b'<svg'),
        ]:
            with self.opener.open(self.url + path, timeout=3) as response:
                self.assertEqual(response.status, 200)
                self.assertEqual(response.headers.get_content_type(), content_type)
                self.assertEqual(response.headers['Cache-Control'], 'no-cache')
                content = response.read()
                self.assertIn(expected, content)
                total += len(content)
        self.assertLess(total, 65_536, 'The buildless UI should stay lightweight')

    def test_api_validation_concurrency_and_restart(self):
        self.assertIsNone(self.request('/api/pet')[1]['pet'])
        self.assertEqual(self.request('/api/pet/actions', {'action': 'feed'})[0], 404)
        self.assertEqual(self.request('/api/pet', {'name': 'a' * 21})[0], 400)
        self.assertEqual(self.request('/api/pet', raw=b'{')[0], 400)
        self.assertEqual(self.request('/api/pet/actions', {'action': 'unknown'})[0], 422)
        self.assertEqual(self.request('/missing')[0], 404)
        # Concurrent hatch requests cannot replace a living buddy.
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            responses = list(pool.map(lambda _: self.request('/api/pet', {'name': "Nori ' もち"}), range(4)))
        self.assertEqual(sorted(status for status, _ in responses), [201, 409, 409, 409])
        status, data = self.request('/api/pet/actions', {'action': 'feed'})
        self.assertEqual(status, 200)
        self.assertEqual(data['pet']['fullness'], 100)
        # Cooldowns are server-authoritative, even across clients.
        self.assertIn('cooldown', self.request('/api/pet/actions', {'action': 'play'})[1]['message'])
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            responses = list(pool.map(lambda _: self.request('/api/pet/actions', {'action': 'sleep'}), range(12)))
        self.assertTrue(all(status == 200 for status, _ in responses))
        expected = self.request('/api/pet')[1]['pet']
        self.assertFalse(expected['sleeping'])
        self.stop()
        self.start()
        self.assertEqual(self.request('/api/pet')[1]['pet'], expected)
        result = subprocess.run([str(BINARY), '--status', '--url', self.url],
                                capture_output=True, text=True, check=True)
        self.assertIn("Nori ' もち", result.stdout)

    def test_simulation_runs_without_tui_and_does_not_double_tick(self):
        self.request('/api/pet', {'name': 'Mochi'})
        # Move the saved clock back ten minutes; do not issue a request to tick it.
        with sqlite3.connect(self.db) as con:
            con.execute('UPDATE pet SET last_tick=last_tick-600')
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            with sqlite3.connect(self.db) as con:
                fullness = con.execute('SELECT fullness FROM pet').fetchone()[0]
            if fullness == 78:
                break
            time.sleep(0.05)
        self.assertEqual(fullness, 78, 'Backend must tick even with no connected client')
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            results = list(pool.map(lambda _: self.request('/api/pet'), range(12)))
        self.assertTrue(all(data['pet']['fullness'] == 78 for _, data in results))
        # Verify death, then replacement, without waiting for real-world neglect.
        with sqlite3.connect(self.db) as con:
            con.execute('UPDATE pet SET health=1,fullness=0,last_tick=last_tick-600')
        self.assertEqual(self.request('/api/pet')[1]['pet']['health'], 0)
        self.assertEqual(self.request('/api/pet', {'name': 'New buddy'})[0], 201)
        self.assertEqual(self.request('/api/pet')[1]['pet']['health'], 100)

    def test_bond_points_and_growth_are_exposed_and_saved(self):
        status, data = self.request('/api/pet', {'name': 'Mochi'})
        self.assertEqual(status, 201)
        self.assertEqual(data['progression']['bond_level'], 1)
        self.assertEqual(data['progression']['next_stage'], 'Child')
        self.assertEqual(data['progression']['care_points'], 0)
        with sqlite3.connect(self.db) as con:
            con.execute('UPDATE pet SET born=born-3600,care_points=36')
        status, data = self.request('/api/pet/actions', {'action': 'feed'})
        self.assertEqual(status, 200)
        self.assertEqual(data['pet']['care_points'], 40)
        self.assertEqual(data['progression']['bond_level'], 2)
        self.assertEqual(data['progression']['stage'], 'Child')
        self.assertEqual(data['progression']['bond_progress'], 0)
        self.assertTrue(data['progression']['achievements'][1]['unlocked'])
        self.assertIn('bond level 2', data['message'])
        self.request('/api/pet/actions', {'action': 'sleep'})
        self.assertEqual(self.request('/api/pet')[1]['pet']['care_points'], 40)
        self.stop()
        self.start()
        self.assertEqual(self.request('/api/pet')[1]['progression']['care_points'], 40)

    @unittest.skipUnless(os.name == 'posix', 'PTY test needs a Unix terminal')
    def test_tui_uses_api_and_quitting_keeps_backend_alive(self):
        import fcntl
        import pty
        import select
        import struct
        import termios
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 32, 90, 0, 0))
        process = subprocess.Popen([str(BINARY), '--url', self.url], stdin=slave,
                                   stdout=slave, stderr=slave,
                                   env={**os.environ, 'TERM': 'xterm-256color'})
        os.close(slave)
        output = bytearray()

        def drain(seconds):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if select.select([master], [], [], 0.05)[0]:
                    try:
                        output.extend(os.read(master, 65536))
                    except OSError:
                        break
        try:
            drain(0.4)
            for key in [b'Nori', b'\r', b'f', b'q']:
                os.write(master, key)
                drain(0.3)
            process.wait(timeout=5)
            self.assertEqual(process.returncode, 0, output.decode(errors='replace'))
            self.assertIn(b'WELLBEING', output)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
        self.assertIsNone(self.proc.poll())
        data = self.request('/api/pet')[1]
        self.assertEqual(data['pet']['name'], 'Nori')
        self.assertEqual(data['pet']['fullness'], 100)


if __name__ == '__main__':
    unittest.main()
