"""Offline tests. All SSH/scp/curl commands are mocks; no router is contacted."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
INSTALLER = ROOT / "scripts/install-panel.sh"
PAYLOAD = ROOT / "scripts/router-install.sh"

MOCK = r'''
import json, os, pathlib, shutil, sys
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
log = pathlib.Path(os.environ['MOCK_LOG'])
data = sys.stdin.read() if name == 'ssh' and '-n' not in args else ''
with log.open('a') as f: f.write(json.dumps({'tool':name,'args':args,'stdin':data})+'\n')
mode = os.environ.get('MOCK_MODE','fresh')
if name == 'ssh':
    cmd=args[-1]
    if cmd == 'sh -s':
        print('model='+os.environ.get('MOCK_MODEL','RN02'))
        print('architecture=armv7l\nfirmware=1.0.64\nlan=192.168.31.1\nspace=100000KiB /data\nmode='+mode)
    elif ' prepare ' in cmd: print('INSTALL_PREPARED mode='+mode+' lan=192.168.31.1')
    elif ' apply ' in cmd:
        if os.environ.get('MOCK_APPLY_FAIL'): sys.exit(1)
        rows = [json.loads(line) for line in log.read_text().splitlines()]
        assert any('rescue-verified' in r['args'][-1] for r in rows if r['tool']=='ssh')
        print('INSTALL_VERIFIED currentExeSha256='+('a'*64)+' ownerInstance=100:200')
    elif ' rollback ' in cmd: print('INSTALL_ROLLBACK_VERIFIED')
elif name == 'scp':
    if any(':/data/be6500panel/' in a for a in args):
        src=args[-2].rsplit('/',1)[-1]
        shutil.copyfile(pathlib.Path(os.environ['MOCK_OLD'])/src,args[-1])
elif name == 'curl':
    if '%{url_effective}' in args: print('https://github.com/nkanf-dev/be6500panel/releases/tag/v0.3.0',end='')
    else:
        dst=args[args.index('-o')+1]
        url=next(a for a in args if a.startswith('https://'))
        src='SHA256SUMS' if url.endswith('SHA256SUMS') else 'be6500panel-armv7.tar.gz'
        shutil.copyfile(pathlib.Path(os.environ['MOCK_OLD'])/src,dst)
'''


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)
        self.bin = self.dir / 'bin'
        self.bin.mkdir()
        self.log = self.dir / 'commands.jsonl'
        self.log.touch()
        for name in ('ssh', 'scp', 'curl'):
            p=self.bin / name
            p.write_text('#!' + sys.executable + '\n' + MOCK)
            p.chmod(0o755)
        self.tools = self.dir / 'tools'
        self.tools.mkdir()
        self.installer = self.tools / 'install-panel.sh'
        self.installer.write_bytes(INSTALLER.read_bytes())
        for name in ('router-install.sh', 'router-rescue-setup.sh', 'rescue-bootstrap.sh', 'rescue-ssh.init'):
            (self.tools / name).write_text('#!/bin/sh\nexit 0\n')
        self.package = self.dir / 'be6500panel-armv7.tar.gz'
        self.make_package()
        self.key = self.dir / 'rescue'
        self.key.write_text('mock private key')
        self.public = self.dir / 'rescue.pub'
        self.public.write_text('ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAAgQMock test\n')
        self.env = dict(os.environ, PATH=str(self.bin)+os.pathsep+os.environ['PATH'],
                        MOCK_LOG=str(self.log), MOCK_OLD=str(self.dir), TMPDIR=str(self.dir))

    def make_package(self, extra=None):
        with tarfile.open(self.package, 'w:gz') as t:
            for name, content in [('be6500-panel',b'ELFmockmanager'),('bootstrap.sh',b'#!/bin/sh\n'),
                                  ('router-native.lua',b'-- mock'),('web/index.html',b'<html>panel</html>')]:
                info=tarfile.TarInfo(name); info.size=len(content); info.mode=0o755 if name.endswith('.sh') else 0o644
                t.addfile(info,io.BytesIO(content))
            if extra:
                info=tarfile.TarInfo(extra)
                info.type=tarfile.SYMTYPE; info.linkname='/etc/passwd'; t.addfile(info)
        self.sha = hashlib.sha256(self.package.read_bytes()).hexdigest()
        (self.dir / 'SHA256SUMS').write_text(self.sha+'  be6500panel-armv7.tar.gz\n')
        (self.dir / 'panel.tar.gz').write_bytes(self.package.read_bytes())
        (self.dir / 'panel.sha256').write_text(self.sha+'\n')
        (self.dir / 'bootstrap.sh').write_text('#!/bin/sh\n')

    def run_installer(self, *, network=False, **env):
        e=dict(self.env,**env)
        args=['sh',str(self.installer),'--host','192.168.31.1','--public-key',str(self.public)]
        if not network: args += ['--package',str(self.package),'--release','v0.3.0']
        return subprocess.run(args,input='private panel password\n',text=True,capture_output=True,env=e,timeout=20)

    def commands(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_shell_syntax(self):
        for p in (INSTALLER,PAYLOAD):
            r=subprocess.run(['sh','-n',str(p)],capture_output=True,text=True)
            self.assertEqual(r.returncode,0,r.stderr)

    def test_first_install_rescue_before_apply_password_not_argv(self):
        r=self.run_installer()
        self.assertEqual(r.returncode,0,r.stderr)
        rows=self.commands()
        ssh=[row for row in rows if row['tool']=='ssh']
        prepare=next(i for i,x in enumerate(ssh) if ' prepare ' in x['args'][-1])
        rescue=next(i for i,x in enumerate(ssh) if 'rescue-verified' in x['args'][-1])
        apply=next(i for i,x in enumerate(ssh) if ' apply ' in x['args'][-1])
        self.assertLess(prepare,rescue);self.assertLess(rescue,apply)
        self.assertIn('2222',ssh[rescue]['args'])
        self.assertIn('BatchMode=yes',ssh[rescue]['args'])
        self.assertIn('IdentitiesOnly=yes',ssh[rescue]['args'])
        self.assertTrue(any(x['stdin']=='private panel password\n' for x in ssh))
        self.assertNotIn('private panel password',json.dumps([x['args'] for x in rows]))
        self.assertNotIn('StrictHostKeyChecking=no',json.dumps(rows))
        self.assertIn('Panel installed.',r.stdout)

    def test_bad_checksum_never_contacts_router(self):
        (self.dir/'SHA256SUMS').write_text('0'*64+'  be6500panel-armv7.tar.gz\n')
        r=self.run_installer();self.assertNotEqual(r.returncode,0)
        self.assertEqual(self.commands(),[])
        self.assertNotIn('Panel installed.',r.stdout)

    def test_link_in_release_rejected_before_ssh(self):
        self.make_package('web/escape')
        r=self.run_installer();self.assertNotEqual(r.returncode,0)
        self.assertEqual(self.commands(),[])

    def test_private_manifest_member_rejected(self):
        self.make_package('native-command-base.json')
        r=self.run_installer();self.assertNotEqual(r.returncode,0)
        self.assertEqual(self.commands(),[])

    def test_model_rejected_after_readonly_probe(self):
        r=self.run_installer(MOCK_MODEL='OTHER')
        self.assertNotEqual(r.returncode,0)
        self.assertEqual([x['tool'] for x in self.commands()],['ssh'])

    def test_upgrade_backup_only_package_and_preserve_password(self):
        r=self.run_installer(MOCK_MODE='upgrade')
        self.assertEqual(r.returncode,0,r.stderr)
        rows=self.commands()
        backups=[x['args'][-2].rsplit('/',1)[-1] for x in rows if x['tool']=='scp' and ':/data/be6500panel/' in x['args'][-2]]
        self.assertEqual(backups,['panel.tar.gz','panel.sha256','bootstrap.sh'])
        self.assertFalse(any('panel-password' in x['args'][-1] for x in rows if x['tool']=='ssh'))

    def test_upgrade_failure_rolls_back_without_success(self):
        r=self.run_installer(MOCK_MODE='upgrade',MOCK_APPLY_FAIL='1')
        self.assertNotEqual(r.returncode,0)
        self.assertTrue(any(' rollback ' in x['args'][-1] for x in self.commands() if x['tool']=='ssh'))
        self.assertNotIn('Panel installed.',r.stdout)
        self.assertIn('Private rollback files retained at:',r.stderr)

    def test_latest_resolves_explicit_tag_before_download(self):
        r=self.run_installer(network=True)
        self.assertEqual(r.returncode,0,r.stderr)
        urls=[arg for x in self.commands() if x['tool']=='curl' for arg in x['args'] if arg.startswith('https://')]
        self.assertIn('https://github.com/nkanf-dev/be6500panel/releases/latest',urls)
        self.assertTrue(any('/download/v0.3.0/be6500panel-armv7.tar.gz' in u for u in urls))
        self.assertIn('Panel release: v0.3.0',r.stdout)

    def test_payload_rescue_and_owner_safety_order(self):
        s=PAYLOAD.read_text()
        self.assertLess(s.index('fail data-space'),s.index('fail rescue-setup'))
        apply=s[s.index('apply)'):s.index('verify)\n')]
        self.assertLess(apply.index('rescue-key-login-required'),apply.index('stop_owner'))
        self.assertIn('kill -TERM "$pid"',s)
        self.assertNotIn('kill -9',s)
        self.assertNotIn('rm -rf "$D"',s)
        self.assertIn('currentExeSha256=',s)
        self.assertIn('ownerInstance=',s)
        self.assertIn('authenticated-health',s)
        self.assertIn('cron-concurrent-change',s)
        self.assertIn('services/.manager.lock',s)
        self.assertNotIn('\"$D/.manager.lock\"',s)
        self.assertIn('/etc/init.d/cron restart',s)
        self.assertIn('dnsBootstrap',s)
        self.assertNotIn('native-handover-bindings.json" >',s)

if __name__ == '__main__':
    unittest.main()
