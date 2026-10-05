"""Synthetic filesystem/commands only: no device, SSH, network or secrets."""
import base64
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"


class RescueInstallTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.stage = self.base / "stage"
        self.bin = self.base / "mocks"
        self.stage.mkdir()
        self.bin.mkdir()
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"])
        for name in ["data", "tmp", "etc/dropbear", "etc/init.d", "etc/crontabs", "usr/sbin", "var/run", "proc/net", "sys/class/net/br-lan/bridge"]:
            (self.base / name).mkdir(parents=True, exist_ok=True)
        self.pub = self.stage / "key.pub"
        self.blob = base64.b64encode(b"synthetic-public-fixture").decode()
        self.pub.write_text("ssh-rsa " + self.blob + " fixture\n")
        self.write("etc/dropbear/dropbear_rsa_host_key", "synthetic-host-fixture")
        self.write("etc/dropbear/authorized_keys", "ssh-rsa b2xk existing-fixture\n")
        self.write("etc/crontabs/root", "0 * * * * /data/be6500panel/bootstrap.sh\n5 * * * * /factory/task\n")
        self.write("proc/net/tcp", "header\n")
        self.write("proc/net/tcp6", "header\n")
        self.mock("ip", 'printf "%s\\n" "2: br-lan inet ${MOCK_IP:-10.6.5.1}/24 scope global br-lan"')
        self.mock("flock", "exit 0")
        self.mock("sha256sum", 'if [ "$1" = -c ]; then shasum -a 256 -c -; else shasum -a 256 "$@"; fi')
        decode = "-D" if sys.platform == "darwin" else "--decode"
        self.mock("base64", f'if [ "$1" = -d ]; then /usr/bin/base64 {decode}; else /usr/bin/base64 "$@"; fi')
        self.mock("nc", 'if [ "${MOCK_BANNER:-yes}" = yes ]; then printf "SSH-2.0-synthetic\\n"; else exit 1; fi')
        self.mock("sleep", "exit 0")
        self.mock("crontab", f'if [ "$1" = -l ]; then cat "{self.base}/etc/crontabs/root"; else cp "$1" "{self.base}/etc/crontabs/root"; fi')
        self.write("usr/sbin/dropbear", '#!/bin/sh\nwhile [ "$#" -gt 0 ]; do if [ "$1" = -f ]; then shift; printf synthetic-generated-host > "$1"; exit 0; fi; shift; done\nexit 1\n')
        (self.base / "usr/sbin/dropbear").chmod(0o700)
        for name in ["router-rescue-setup.sh", "rescue-bootstrap.sh", "rescue-ssh.init"]:
            source = (SCRIPTS / name).read_text()
            prefixes = ["/data", "/tmp", "/proc", "/etc", "/usr/sbin", "/var/run", "/sys"]
            for index, prefix in enumerate(prefixes):
                source = source.replace(prefix, f"@@ROOT{index}@@")
            for index, prefix in enumerate(prefixes):
                source = source.replace(f"@@ROOT{index}@@", str(self.base / prefix.lstrip("/")))
            source = source.replace('"$INIT" enable', 'mock-init enable').replace('"$INIT" restart', 'mock-init restart')
            (self.stage / name).write_text(source)
        self.mock("mock-init", "\n".join([
            'if [ "$1" = enable ]; then exit 0; fi',
            f'printf "restart\\n" >> "{self.base}/restarts"',
            '[ "${MOCK_READY:-yes}" = yes ] || exit 0',
            f'mkdir -p "{self.base}/proc/123/fd"',
            f'printf 123 > "{self.base}/var/run/be6500-rescue-2222.pid"',
            f'ln -sf "{self.base}/data/ssh/bin/dropbear" "{self.base}/proc/123/exe"',
            f'printf "%s\\000" dropbear -s "${{MOCK_IP:-10.6.5.1}}:2222" 127.0.0.1:2222 > "{self.base}/proc/123/cmdline"',
            f"ln -sf 'socket:[71]' '{self.base}/proc/123/fd/3'",
            f"ln -sf 'socket:[72]' '{self.base}/proc/123/fd/4'",
            "hex=$(printf '%s\\n' \"${MOCK_IP:-10.6.5.1}\" | awk -F. '{printf \"%02X%02X%02X%02X\",$4,$3,$2,$1}')",
            f"printf '0: %s:08AE 00000000:0000 0A 0:0 00:0 0 0 0 71\\n1: 0100007F:08AE 00000000:0000 0A 0:0 00:0 0 0 0 72\\n' \"$hex\" > '{self.base}/proc/net/tcp'",
        ]))

    def write(self, name, text):
        (self.base / name).write_text(text)

    def mock(self, name, text):
        p = self.bin / name
        p.write_text("#!/bin/sh\nset -eu\n" + text + "\n")
        p.chmod(0o700)

    def run_script(self, name="router-rescue-setup.sh", args=None):
        if args is None:
            args = ["--public-key", str(self.pub)] if name.startswith("router-") else []
        return subprocess.run(["sh", str(self.stage / name), *args], env=self.env, text=True, capture_output=True, timeout=10)

    def assert_ready(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "RESCUE_READY port=2222\n")

    def test_idempotent_preserves_keys_cron_host(self):
        self.assert_ready(self.run_script())
        self.assert_ready(self.run_script())
        saved = self.base / "data/ssh"
        self.assertEqual((saved / "dropbear_rsa_host_key").read_text(), "synthetic-host-fixture")
        keys = (saved / "authorized_keys").read_text()
        self.assertIn("existing-fixture", keys)
        self.assertEqual(keys.count(self.blob), 1)
        self.assertEqual((saved / "authorized_keys").stat().st_mode & 0o777, 0o600)
        cron = (self.base / "etc/crontabs/root").read_text()
        self.assertEqual(cron.count("# be6500-rescue-bootstrap"), 1)
        self.assertIn("/factory/task", cron)
        self.assertIn("/data/be6500panel/bootstrap.sh", cron)
        self.assertEqual((self.base / "restarts").read_text().count("restart"), 1)

    def test_etc_repair_and_bridge_rebind(self):
        self.assert_ready(self.run_script())
        (self.base / "etc/init.d/be6500-rescue").unlink()
        (self.base / "etc/dropbear/authorized_keys").unlink()
        self.env["MOCK_IP"] = "172.20.2.4"
        self.assert_ready(self.run_script("rescue-bootstrap.sh"))
        self.assertTrue((self.base / "etc/init.d/be6500-rescue").exists())
        self.assertTrue((self.base / "etc/dropbear/authorized_keys").is_symlink())
        self.assertEqual((self.base / "restarts").read_text().count("restart"), 2)

    def test_host_key_generation(self):
        (self.base / "etc/dropbear/dropbear_rsa_host_key").unlink()
        self.assert_ready(self.run_script())
        self.assertEqual((self.base / "data/ssh/dropbear_rsa_host_key").read_text(), "synthetic-generated-host")
        self.assert_ready(self.run_script())

    def test_missing_listener_or_banner_not_ready(self):
        for flag in ["MOCK_READY", "MOCK_BANNER"]:
            with self.subTest(flag=flag):
                self.env[flag] = "no"
                result = self.run_script()
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn("RESCUE_READY", result.stdout)
                self.env.pop(flag)

    def test_private_key_and_unknown_options_rejected(self):
        self.pub.write_text("-----BEGIN OPENSSH PRIVATE KEY-----\nnot-a-key\n")
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("not-a-key", result.stderr + result.stdout)
        self.assertFalse((self.base / "data/ssh").exists())
        self.assertNotEqual(self.run_script(args=["--command", "echo injected"]).returncode, 0)

    def test_corrupt_binary_rejected(self):
        self.assert_ready(self.run_script())
        (self.base / "data/ssh/bin/dropbear").write_text("corruption")
        result = self.run_script("rescue-bootstrap.sh")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("binary-integrity", result.stderr)

    def test_invalid_bridge_has_no_fallback(self):
        self.env["MOCK_IP"] = "0.0.0.0"
        self.assertNotEqual(self.run_script().returncode, 0)
        self.assertFalse((self.base / "restarts").exists())

    def test_procd_22_only_if_free(self):
        self.assert_ready(self.run_script())
        procd = "procd_open_instance() { printf 'instance=%s\\n' \"$1\"; }; procd_set_param() { printf '%s ' \"$@\"; printf '\\n'; }; procd_close_instance() { :; }; "
        command = procd + '. "' + str(self.stage / "rescue-ssh.init") + '"; start_service'
        free = subprocess.run(["sh", "-c", command], env=self.env, text=True, capture_output=True, timeout=10)
        self.assertEqual(free.returncode, 0, free.stderr)
        self.assertIn("instance=lan-2222", free.stdout)
        self.assertIn("instance=lan-22", free.stdout)
        self.assertIn("-s -j -k", free.stdout)
        with (self.base / "proc/net/tcp6").open("a") as stream:
            stream.write("2: 00000000000000000000000000000000:0016 0:0 0A 0:0 00:0 0 0 0 91\n")
        busy = subprocess.run(["sh", "-c", command], env=self.env, text=True, capture_output=True, timeout=10)
        self.assertEqual(busy.returncode, 0, busy.stderr)
        self.assertIn("instance=lan-2222", busy.stdout)
        self.assertNotIn("instance=lan-22\n", busy.stdout)


if __name__ == "__main__":
    unittest.main()
