#!/usr/bin/env python3
"""Focused offline tests. Never contact a router, execute SSH or run unlock.

Run from the repository: python3 tests/enable_ssh_test.py
All inputs are synthetic. curl and the port probe are replaced by local mocks.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "enable-ssh.sh"
TOKEN = "a" * 32
SALT = "6d2df50a-250f-4a30-a5e6-d44fb0960aa0"

CURL_MOCK = r"""
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
log = Path(os.environ['MOCK_LOG'])
with log.open('a') as stream:
    stream.write(json.dumps({'kind': 'curl', 'args': args}) + '\n')
if args[-1].endswith('/init_info'):
    if os.environ.get('INFO_FAIL') == '1':
        print('RAW_SECRET_ERROR', file=sys.stderr)
        sys.exit(22)
    body = os.environ.get('INFO_BODY', '{"code":0,"hardware":"RN02","romversion":"1.0.43"}')
else:
    records = [json.loads(line) for line in log.read_text().splitlines()]
    step = sum(r['kind'] == 'curl' and r['args'][-1].endswith('/start_binding') for r in records)
    if str(step) == os.environ.get('FAIL_STEP'):
        print('RAW_SECRET_ERROR', file=sys.stderr)
        sys.exit(28)
    body = os.environ.get('POST_BODY', '{ "code" : 0, "private": "RAW_SECRET_RESPONSE" }')
print(body, end='')
print('\n' + os.environ.get('HTTP_STATUS', '200'), end='')
"""
PYTHON_MOCK = r"""
import json, os, sys
from pathlib import Path
if len(sys.argv) > 2 and 'import socket, sys, time' in sys.argv[2]:
    with Path(os.environ['MOCK_LOG']).open('a') as stream:
        stream.write(json.dumps({'kind': 'probe', 'host': sys.argv[3]}) + '\n')
    sys.exit(int(os.environ.get('PROBE_FAIL', '0')))
os.execv(os.environ['REAL_PYTHON'], [os.environ['REAL_PYTHON']] + sys.argv[1:])
"""
OPENSSL_MOCK = r"""
import hashlib, os, sys
from pathlib import Path
assert sys.argv[1:] == ['dgst', '-md5', '-r'], sys.argv
mode = os.environ.get('MD5_MODE', 'normal')
if mode == 'fail':
    print('secret-openssl-error', file=sys.stderr)
    sys.exit(1)
digest = hashlib.md5(sys.stdin.buffer.read()).hexdigest()
if mode == 'uppercase':
    print(digest.upper() + ' *stdin')
elif mode == 'legacy':
    print('MD5(stdin)= ' + digest)
elif mode == 'short':
    print(digest[:8] + ' *stdin')
elif mode == 'invalid':
    print('z' * 32 + ' *stdin')
elif mode == 'extra':
    print(digest + ' *stdin\nextra')
else:
    print(digest + ' *stdin')
"""


class EnableSshTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)
        self.log = self.dir / 'calls.jsonl'
        for name, code in [('curl', CURL_MOCK), ('python3', PYTHON_MOCK), ('openssl', OPENSSL_MOCK)]:
            target = self.dir / name
            target.write_text('#!' + sys.executable + '\n' + code)
            target.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.dir) + os.pathsep + os.environ['PATH'],
                        MOCK_LOG=str(self.log), REAL_PYTHON=sys.executable)
        # Ignore external proxy settings, agents and shell environment overrides.
        for key in ('BASH_ENV', 'ENV', 'SHELLOPTS'):
            self.env.pop(key, None)

    def run_script(self, args=(), inputs='', **settings):
        env = dict(self.env, **settings)
        return subprocess.run(['/bin/sh', str(SCRIPT), *args], input=inputs,
                              text=True, capture_output=True, env=env, timeout=10)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def interactive(self, extra='ENABLE\nn\n', ip='192.0.2.1', token=TOKEN, **settings):
        return self.run_script(inputs=f'{ip}\n{token}\n{extra}', **settings)

    def assert_no_secrets(self, result):
        for secret in (TOKEN, 'RAW_SECRET_ERROR', 'RAW_SECRET_RESPONSE'):
            self.assertNotIn(secret, result.stdout + result.stderr)

    def test_help_and_dry_run_are_offline_and_version_limited(self):
        self.assertEqual(self.run_script(['--help']).returncode, 0)
        for version in ('1.0.42', '1.0.43'):
            result = self.run_script(['--dry-run', '--firmware', version])
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('device/version not verified', result.stdout)
            self.assertIn('No requests sent', result.stdout)
        result = self.run_script(['--dry-run', '--firmware', '1.0.64'])
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_fixed_initial_algorithm_for_slash_and_no_slash_and_crlf(self):
        for sn in ('SYNTHETIC/ABC123', 'SYNTHETIC123', '\r\nSYNTHETIC/ABC123\r\n'):
            expected = hashlib.md5((sn.strip('\r\n') + SALT).encode()).hexdigest()[:8]
            result = self.run_script(['--calculate-password', sn])
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), expected)
            self.assertIn('current password may have changed', result.stderr)
        self.assertEqual(self.calls(), [])

    def test_hash_format_is_strict_and_failure_is_safe(self):
        expected = hashlib.md5(('SYNTHETIC/ABC123' + SALT).encode()).hexdigest()[:8]
        result = self.run_script(['--calculate-password', 'SYNTHETIC/ABC123'], MD5_MODE='uppercase')
        self.assertEqual(result.stdout.strip(), expected)
        for mode in ('legacy', 'short', 'invalid', 'extra', 'fail'):
            result = self.run_script(['--calculate-password', 'SYNTHETIC/ABC123'], MD5_MODE=mode)
            self.assertNotEqual(result.returncode, 0, mode)
            self.assertEqual(result.stdout, '')
            self.assertNotIn('secret-openssl-error', result.stderr)

    def test_invalid_serial_numbers_rejected(self):
        for sn in ('', 'A B', 'A/B/C', '/ABC', 'ABC/', 'A\nB', 'é', 'x' * 65, 'ABC;touch'):
            result = self.run_script(['--calculate-password', sn])
            self.assertNotEqual(result.returncode, 0, repr(sn))
            self.assertEqual(result.stdout, '')

    def test_both_versions_send_only_the_four_fixed_requests(self):
        expected = [
            "uid=1234&key=1234'%0Anvram%20set%20ssh_en%3D1'",
            "uid=1234&key=1234'%0Anvram%20commit'",
            "uid=1234&key=1234'%0Ased%20-i%20's%2Fchannel%3D.*%2Fchannel%3D%22debug%22%2Fg'%20%2Fetc%2Finit.d%2Fdropbear'",
            "uid=1234&key=1234'%0A%2Fetc%2Finit.d%2Fdropbear%20start'",
        ]
        for version in ('1.0.42', '1.0.43'):
            self.log.unlink(missing_ok=True)
            result = self.interactive(INFO_BODY=json.dumps({'code': 0, 'hardware': 'RN02', 'romversion': version}))
            self.assertEqual(result.returncode, 0, result.stderr)
            calls = self.calls()
            http = [call['args'] for call in calls if call['kind'] == 'curl']
            self.assertEqual(len(http), 5)
            self.assertTrue(http[0][-1].endswith('/init_info'))
            self.assertEqual([a[a.index('--data') + 1] for a in http[1:]], expected)
            for args in http:
                self.assertEqual(args[0], '-q')
                self.assertIn('--connect-timeout', args)
                self.assertIn('--max-time', args)
                self.assertIn('--max-filesize', args)
                self.assertIn('--noproxy', args)
                self.assertNotIn('--location', args)
            self.assertEqual(calls[-1], {'kind': 'probe', 'host': '192.0.2.1'})
            self.assertIn('SSH transport detected', result.stdout)
            self.assertIn('Root login and persistence are not verified', result.stdout)
            self.assert_no_secrets(result)

    def test_unknown_or_wrong_hardware_and_version(self):
        for hw, version in [('RN02', '1.0.64'), ('RA01', '1.0.43')]:
            result = self.interactive(INFO_BODY=json.dumps({'code': 0, 'hardware': hw, 'romversion': version}))
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(len(self.calls()), 1)
            self.log.unlink()
        result = self.interactive(extra='RN02\n1.0.42\nENABLE\nn\n', INFO_FAIL='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('user-confirmed RN02 1.0.42', result.stdout)
        self.assert_no_secrets(result)

    def test_unknown_information_requires_confirmation_and_no_override(self):
        for body in ('{}', '<html>login</html>', '{"code":0,"hardware":"RN02"}'):
            self.log.unlink(missing_ok=True)
            result = self.interactive(extra='RN02\n1.0.64\nENABLE\nn\n', INFO_BODY=body)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(len(self.calls()), 1)
        result = self.interactive(extra='RN02\n1.0.42\nENABLE\nn\n', INFO_BODY='{"code":0,"romversion":"1.0.43"}')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('conflicts', result.stderr)

    def test_failure_at_each_step_stops_without_probe(self):
        for step in range(1, 5):
            self.log.unlink(missing_ok=True)
            result = self.interactive(FAIL_STEP=str(step))
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(len(self.calls()), 1 + step)
            self.assertIn(f'Step {step}', result.stderr)
            self.assertIn('Earlier changes may remain', result.stderr)
            self.assert_no_secrets(result)

    def test_json_parsing_is_not_a_grep_and_rejects_http_redirects(self):
        for body in ('{"code":1,"other":{"code":0}}', '{"code":false}', '{"code":"0"}',
                     '{"code":0,"code":1}', '{"code":0} junk', 'not json'):
            self.log.unlink(missing_ok=True)
            result = self.interactive(POST_BODY=body)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(len(self.calls()), 2)
            self.assert_no_secrets(result)
        self.log.unlink()
        result = self.interactive(HTTP_STATUS='302', extra='RN02\n1.0.43\nENABLE\nn\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(self.calls()), 2)

    def test_probe_failure_does_not_claim_ssh_enabled_or_login(self):
        result = self.interactive(PROBE_FAIL='1')
        self.assertEqual(result.returncode, 2)
        self.assertIn('SSH transport was not verified', result.stdout)
        self.assertNotIn('SSH transport detected', result.stdout)
        self.assertEqual(sum(c['kind'] == 'probe' for c in self.calls()), 1)
        self.assert_no_secrets(result)

    def test_calculation_is_explicit_and_does_not_set_password(self):
        sn = 'SYNTHETIC/ABC123'
        expected = hashlib.md5((sn + SALT).encode()).hexdigest()[:8]
        result = self.interactive(extra='ENABLE\ny\n' + sn + '\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(expected, result.stdout)
        self.assertIn('Initial root-password candidate only', result.stderr)
        self.assertEqual(len(self.calls()), 6)
        self.assert_no_secrets(result)

    def test_cancellation_and_bad_inputs_do_not_send_enable_requests(self):
        result = self.interactive(extra='no\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(self.calls()), 1)
        for ip in ('http://192.0.2.1', '192.0.2.1/24', 'localhost', '127.0.0.1', '224.0.0.1'):
            self.log.unlink(missing_ok=True)
            result = self.interactive(ip=ip)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(self.calls(), [])
        for token in ('short', 'a' * 31, 'z' * 32, 'a' * 32 + '/api/other'):
            result = self.interactive(token=token)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(self.calls(), [])

    def test_no_connection_tool_or_password_setting(self):
        script = SCRIPT.read_text()
        self.assertNotIn('ssh -o', script)
        self.assertNotIn('adminpassword', script)
        self.assertNotIn('passwd root', script)
        license_text = (ROOT / 'third_party/xiaomi-ssh/LICENSE').read_text()
        self.assertIn('MIT License', license_text)
        self.assertIn('Copyright (c) 2026', license_text)


if __name__ == '__main__':
    unittest.main(verbosity=2)
