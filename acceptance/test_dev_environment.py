import json
import os
from pathlib import Path
import tempfile
import unittest

from scripts.dev_environment import configuration


class DevEnvironmentTests(unittest.TestCase):
    def test_private_directories_reject_symlinks_and_shared_permissions(self):
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary)
            shared = parent / "shared"
            shared.mkdir(mode=0o755)
            linked = parent / "linked"
            linked.symlink_to(shared, target_is_directory=True)
            for directory in (shared, linked):
                with self.assertRaises(ValueError):
                    configuration.private_directory(directory)

    def test_child_environment_does_not_inherit_desktop_or_credentials(self):
        with tempfile.TemporaryDirectory() as temporary:
            before = dict(os.environ)
            environment = configuration.child_environment(Path(temporary))
            self.assertEqual(dict(os.environ), before)
            self.assertNotIn("SSH_AUTH_SOCK", environment)
            self.assertTrue(environment["CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET"].startswith(temporary))
            for name in ("HOME", "CODEX_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME"):
                self.assertTrue(environment[name].startswith(temporary))

    def test_production_data_path_is_rejected_even_with_a_matching_checksum(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / "config.json"
            config.write_text(json.dumps({"data_dir": "/production"}))
            manifest = {"source": str(root), "fixture": "/fixture",
                        "files": {"config.json": configuration.digest(config)}}
            with self.assertRaisesRegex(ValueError, "边界"):
                configuration.configuration(root, root, manifest)
