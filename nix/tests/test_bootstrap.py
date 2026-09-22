"""Exercise install.sh through the real public CLI and temporary Git host."""
import os
from pathlib import Path
import shlex
import subprocess
import unittest

import test_plan
from test_plan import REPOSITORY, TARGET


class BootstrapContractTests(unittest.TestCase):
    def setUp(self):
        self.host = test_plan.PlanTests()
        self.host.setUp()
        self.addCleanup(self.host.doCleanups)
        host = self.host
        (host.repo / 'lib').mkdir()
        (host.repo / 'lib/install-system.sh').write_text('''install_system_ensure_lix() {
  printf '%s\\n' "$CONTRACT_STATE/nix/bin/nix"
}
''')
        host.executable(host.repo / 'bin/install-mise.sh', '#!/bin/sh\n: > "$CONTRACT_STATE/installer"\n')
        mise = host.home / '.local/bin/mise'
        mise.parent.mkdir(parents=True)
        host.executable(mise, '''#!/bin/sh
set -eu
[ -e "$CONTRACT_STATE/activated" ]
[ "$(cat "$HOME/managed/settings")" = desired ]
: > "$CONTRACT_STATE/mise-$1"
''')
        cargo = host.state / 'nix/bin/cargo'
        host.executable(cargo, '''#!/bin/sh
set -eu
[ "$1" = run ]; shift
while [ "$#" -gt 0 ] && [ "$1" != -- ]; do shift; done
[ "$#" -gt 0 ]; shift
exec ''' + shlex.quote(str(TARGET / 'debug/dotfiles')) + ''' "$@"
''')
        nix = host.state / 'nix/bin/nix'
        text = nix.read_text()
        text = text.replace("if a[:2] == ['flake', 'metadata']:",
                            "if a[:1] == ['shell']:\n    sys.exit(subprocess.call(a[a.index('--command') + 1:]))\nelif a[:2] == ['flake', 'metadata']:")
        nix.write_text(text)
        host.env['PATH'] = str(cargo.parent) + os.pathsep + host.env.get('PATH', '')
        host.git('add', '.')
        host.git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                 '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'bootstrap fixture')
        host.git('branch', '-M', 'master')
        remote = host.root / 'remote.git'
        subprocess.run(['git', 'clone', '--bare', str(host.repo), str(remote)], env=host.env,
                       capture_output=True, check=True)
        host.git('remote', 'add', 'origin', str(remote))

    def install(self, answer):
        return subprocess.run(['/bin/sh', str(REPOSITORY / 'install.sh')], env=self.host.env,
                              input=answer, capture_output=True, text=True, timeout=30)

    def test_bootstrap_applies_before_installing_user_tools_and_connects_master(self):
        result = self.install('yes\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        host = self.host
        self.assertEqual((host.home / 'managed/settings').read_text(), 'desired')
        self.assertEqual((host.state / 'record').readlink(), host.repo / 'flake.nix')
        for stage in ['trust', 'install', 'bootstrap']:
            self.assertTrue((host.state / ('mise-' + stage)).exists())
        self.assertEqual(host.git('branch', '--show-current').stdout.strip(), 'master')
        self.assertEqual(host.git('rev-parse', '--abbrev-ref', '@{upstream}').stdout.strip(), 'origin/master')
        self.assertEqual(host.git('rev-parse', 'HEAD').stdout, host.git('rev-parse', 'origin/master').stdout)

    def test_declining_bootstrap_stops_before_user_tool_installation(self):
        result = self.install('no\n')
        self.assertNotEqual(result.returncode, 0)
        self.host.assert_untouched()
        self.assertFalse((self.host.state / 'mise-install').exists())
        self.assertFalse((self.host.state / 'mise-bootstrap').exists())

    def test_dirty_checkout_is_rejected_before_installation(self):
        (self.host.repo / 'home/managed/settings').write_text('uncommitted')
        result = self.install('yes\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.host.state / 'installer').exists())
        self.host.assert_untouched()


if __name__ == '__main__':
    unittest.main()
