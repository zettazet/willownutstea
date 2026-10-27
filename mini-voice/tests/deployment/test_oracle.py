"""Local checks only: no cloud API, apt, firewall changes, or certificate issuance."""
import getpass
import hashlib
import json
import shutil
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEPLOY = ROOT / 'deploy/oracle'


class OracleDeployment(unittest.TestCase):
    def call(self, *args, script=None):
        return subprocess.run(['bash', str(script or DEPLOY / 'install.sh'), *args],
                              text=True, capture_output=True, timeout=20)

    def test_shell_syntax_and_help(self):
        for script in DEPLOY.glob('*.sh'):
            subprocess.run(['bash', '-n', str(script)], check=True)
        result = self.call('--help')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Ubuntu 24.04', result.stdout)

    def test_both_architecture_plans_and_binary_integrity(self):
        for arch, machine in [('x64', 62), ('arm64', 183)]:
            folder = ROOT / 'releases' / ('linux-' + arch)
            binary = folder / 'mini-server'
            with binary.open('rb') as stream:
                header = stream.read(20)
            self.assertEqual(header[:4], b'\x7fELF')
            self.assertEqual(header[4:6], b'\x02\x01')  # ELF64, little endian
            self.assertEqual(struct.unpack('<H', header[18:20])[0], machine)
            expected = (folder / 'BINARY-SHA256SUMS.txt').read_text().split()[0]
            self.assertEqual(hashlib.sha256(binary.read_bytes()).hexdigest(), expected)
            result = self.call('--domain', 'voice.example.org', '--email', 'test@example.org',
                               '--arch', arch, '--dry-run')
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('Plan only; no changes made', result.stdout)
            self.assertIn('Architecture: ' + arch, result.stdout)

    def test_invalid_inputs_rejected_before_any_installation(self):
        cases = [[], ['--domain'], ['--domain', '--dry-run'],
                 ['--domain', '127.0.0.1', '--email', 'a@example.org', '--dry-run'],
                 ['--domain', 'voice..example.org', '--email', 'a@example.org', '--dry-run'],
                 ['--domain', 'voice.example.org;id', '--email', 'a@example.org', '--dry-run'],
                 ['--domain', 'voice.example.org', '--email', 'bad', '--dry-run'],
                 ['--domain', 'voice.example.org', '--email', 'a@example.org', '--arch', 'bad', '--dry-run']]
        for args in cases:
            result = self.call(*args)
            self.assertNotEqual(result.returncode, 0, args)
            self.assertNotIn('Installed.', result.stdout)

    def test_tampered_bundle_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            script = root / 'deploy/oracle/install.sh'
            script.parent.mkdir(parents=True)
            shutil.copy2(DEPLOY / 'install.sh', script)
            release = root / 'releases/linux-x64'
            release.mkdir(parents=True)
            (release / 'mini-server').write_bytes(b'corrupt')
            (release / 'BINARY-SHA256SUMS.txt').write_text('0' * 64 + '  mini-server\n')
            result = self.call('--domain', 'voice.example.org', '--email', 'a@example.org',
                               '--arch', 'x64', '--dry-run', script=script)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('checksum mismatch', result.stderr)

    def test_firewall_plan_is_non_mutating_and_explicit(self):
        result = self.call('--dry-run', script=DEPLOY / 'open-firewall.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('No changes made', result.stdout)
        result = self.call(script=DEPLOY / 'open-firewall.sh')
        self.assertNotEqual(result.returncode, 0)

    def test_configuration_and_systemd_unit(self):
        config = json.loads((DEPLOY / 'server.example.json').read_text())
        self.assertEqual(config['database'], '/var/lib/mini-voice/mini-voice.sqlite')
        self.assertEqual(config['listen'], '0.0.0.0:4433')
        self.assertEqual(config['udp_listen'], '0.0.0.0:4434')
        analyze = shutil.which('systemd-analyze')
        if not analyze:
            self.skipTest('systemd-analyze is unavailable; unit was not verified')
        with tempfile.TemporaryDirectory() as temp:
            # Verify the real directives with paths/users that exist on the build host.
            # systemd-analyze verifies syntax/dependencies; it does not start the service.
            unit = (DEPLOY / 'mini-voice.service').read_text()
            unit = unit.replace('/usr/local/bin/mini-server', str(ROOT / 'releases/linux-x64/mini-server'))
            unit = unit.replace('/var/lib/mini-voice', temp).replace('/etc/mini-voice', temp)
            unit = unit.replace('User=mini-voice', 'User=' + getpass.getuser())
            unit = unit.replace('Group=mini-voice\n', '')
            path = Path(temp) / 'mini-voice.service'
            path.write_text(unit)
            result = subprocess.run([analyze, 'verify', '--man=no', str(path)],
                                    text=True, capture_output=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
