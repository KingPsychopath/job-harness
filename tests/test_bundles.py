"""Real transfers and invalid archives in disposable synthetic workspaces."""

import hashlib
import json
import os
import stat
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

from job_harness.bundles import FORMAT, export_bundle, import_bundle
from job_harness.host import configure, connection, default_workspace
from job_harness.service import Harness
from job_harness.workspace import initialize


class BundleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.home = Path(self.temp.name)
        self.root = self.home / "private"
        initialize(self.root)
        (self.root / "context/profile/facts.md").write_text(
            "# Facts\n\nSynthetic candidate evidence.\n"
        )
        (self.root / "output/cv.pdf").write_bytes(b"%PDF-1.7\nSynthetic fixture\n")
        config = json.loads((self.root / "config/career-ledger.json").read_text())
        config["primary_cv"] = "output/cv.pdf"
        (self.root / "config/career-ledger.json").write_text(json.dumps(config))
        (self.root / "applications/example-engineer.md").write_text(
            "# Example — Engineer\n\n- Stage: applied\n- Source URL: https://jobs.example.com/123\n"
        )
        self.h = Harness(self.root)

    def tearDown(self):
        self.temp.cleanup()

    def test_workspace_round_trip_preserves_data_and_unknown_attempts(self):
        start = self.h.call(
            "start_run",
            {
                "operation": "apply",
                "scope": {"sources": ["employer"], "max_submissions": 1},
                "user_request": "Synthetic authorized application",
                "idempotency_key": "run",
            },
        )
        # Exact attempt preservation must not depend on reconstructing CV/answers from chat memory.
        state = self.h.ledger.load_state()
        state["attempts"]["fixture"] = {
            "id": "fixture",
            "run_id": start["data"]["run_id"],
            "packet_id": "fixture",
            "record": "applications/example-engineer.md",
            "company": "Example",
            "vacancy_url": "https://jobs.example.com/123",
            "status": "outcome_unknown",
        }
        self.h.ledger.state_path.write_text(json.dumps(state))
        before = (self.root / "applications/operations/state.json").read_bytes()
        archive = self.home / "workspace.jhb"
        result = export_bundle(self.root, archive)
        self.assertFalse(result["encrypted"])
        self.assertEqual(stat.S_IMODE(archive.stat().st_mode), 0o600)
        self.assertEqual((self.root / "applications/operations/state.json").read_bytes(), before)
        restored = self.home / "restored"
        import_bundle(restored, archive)
        self.assertEqual(
            (restored / "output/cv.pdf").read_bytes(), (self.root / "output/cv.pdf").read_bytes()
        )
        h = Harness(restored)
        duplicate = h.call(
            "check_duplicate", {"company": "Example", "vacancy_url": "https://jobs.example.com/123"}
        )
        self.assertTrue(duplicate["data"]["blocked"])
        self.assertEqual(h.ledger.load_state()["attempts"]["fixture"]["status"], "outcome_unknown")

    def test_profile_excludes_history_runtime_and_credentials(self):
        for name in [
            ".git/private",
            ".scratch/private",
            "config/.env",
            "context/profile/credentials.json",
            "sources/browser.har",
            "context/profile/.netrc",
            "context/profile/id_ed25519",
            "context/profile/Cookies",
            "context/profile/cookies.sqlite",
            "context/profile/storage-state.json",
            "context/.Git/private",
        ]:
            p = self.root / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("PRIVATE-RUNTIME-SENTINEL")
        archive = self.home / "profile.jhb"
        export_bundle(self.root, archive, "profile")
        with zipfile.ZipFile(archive) as z:
            self.assertIn("context/profile/facts.md", z.namelist())
            self.assertIn("output/cv.pdf", z.namelist())
            self.assertNotIn("applications/example-engineer.md", z.namelist())
            self.assertNotIn("applications/operations/state.json", z.namelist())
            self.assertFalse(any(b"PRIVATE-RUNTIME-SENTINEL" in z.read(n) for n in z.namelist()))
        restored = self.home / "fresh"
        import_bundle(restored, archive)
        self.assertEqual(Harness(restored).call("bootstrap")["data"]["health"]["records"], 0)

    def test_import_refuses_overwrite_and_corruption_without_partial_files(self):
        archive = self.home / "source.jhb"
        export_bundle(self.root, archive)
        with self.assertRaises(ValueError):
            import_bundle(self.root, archive)
        corrupt = self.home / "corrupt.jhb"
        with zipfile.ZipFile(archive) as src, zipfile.ZipFile(corrupt, "w") as dst:
            for name in src.namelist():
                dst.writestr(name, b"changed" if name == "output/cv.pdf" else src.read(name))
        target = self.home / "failed"
        with self.assertRaises(ValueError):
            import_bundle(target, corrupt)
        self.assertFalse(target.exists())
        self.assertFalse(list(self.home.glob(".job-harness-import-*")))
        bad = self.home / "not-zip.jhb"
        bad.write_text("not zip")
        with self.assertRaises(ValueError):
            import_bundle(target, bad)

    def test_malicious_archive_names_types_and_undeclared_payloads(self):
        for path, symlink, extra in [
            ("../outside", False, False),
            ("context\\outside", False, False),
            ("context/x.md", True, False),
            ("context/x.md", False, True),
        ]:
            archive = self.home / "malicious.jhb"
            data = b"x"
            manifest = {
                "format": FORMAT,
                "schema_version": 1,
                "kind": "workspace",
                "created_at": "fixture",
                "files": [{"path": path, "size": 1, "sha256": hashlib.sha256(data).hexdigest()}],
            }
            with zipfile.ZipFile(archive, "w") as z:
                info = zipfile.ZipInfo(path)
                if symlink:
                    info.external_attr = (stat.S_IFLNK | 0o777) << 16
                z.writestr(info, data)
                z.writestr("manifest.json", json.dumps(manifest))
                if extra:
                    z.writestr("context/undeclared.md", "x")
            with self.assertRaises(ValueError):
                import_bundle(self.home / "unsafe", archive)
            self.assertFalse((self.home / "unsafe").exists())
            self.assertFalse((self.home / "outside").exists())

    def test_export_refuses_symlinks_and_pending_journal(self):
        (self.root / "context/profile/link.md").symlink_to(self.root / "context/profile/facts.md")
        archive = self.home / "refused.jhb"
        with self.assertRaisesRegex(ValueError, "symlinked data"):
            export_bundle(self.root, archive)
        self.assertFalse(archive.exists())
        (self.root / "context/profile/link.md").unlink()
        state = self.h.ledger.load_state()
        state["pending"] = [
            {"path": "applications/example-engineer.md", "text": "pending", "before_sha256": "fake"}
        ]
        self.h.ledger.state_path.write_text(json.dumps(state))
        with self.assertRaisesRegex(ValueError, "Pending transaction"):
            export_bundle(self.root, archive)
        self.assertFalse(archive.exists())

    def test_host_settings_precedence_and_no_credentials(self):
        with patch.dict(os.environ, {"XDG_CONFIG_HOME": str(self.home / "host")}, clear=True):
            self.assertIsNone(default_workspace())
            configure(self.root)
            self.assertEqual(default_workspace(), self.root.resolve())
            self.assertEqual(connection(self.root)["args"][-1], "focused")
            self.assertEqual(connection(self.root)["args"][:2], ["-P", "-m"])
            with patch.dict(os.environ, {"JOB_HARNESS_WORKSPACE": str(self.home / "override")}):
                self.assertEqual(default_workspace(), self.home / "override")
            with self.assertRaises(ValueError):
                configure(self.home / "missing")
            settings = json.loads((self.home / "host/job-harness/settings.json").read_text())
            self.assertEqual(set(settings), {"schema_version", "workspace"})

    def test_framework_launcher_preserves_selected_virtual_environment(self):
        runtime = self.home / "runtime"
        (runtime / "bin").mkdir(parents=True)
        (runtime / "bin/python").write_text("synthetic interpreter path")
        with (
            patch("job_harness.host.sys.prefix", str(runtime)),
            patch("job_harness.host.sys.base_prefix", "/framework"),
            patch("job_harness.host.sys.executable", "/framework/python"),
        ):
            self.assertEqual(connection(self.root)["command"], str(runtime / "bin/python"))

    def test_export_refuses_destination_inside_workspace(self):
        for archive in [self.root / "snapshot.jhb", self.root / "sources/snapshot.jhb"]:
            with self.assertRaisesRegex(ValueError, "outside the private workspace"):
                export_bundle(self.root, archive)
            self.assertFalse(archive.exists())
        self.assertFalse(list(self.root.rglob(".job-harness-bundle-*")))
        alias = self.root.parent / self.root.name.upper()
        if alias.exists() and alias.samefile(self.root):
            with self.assertRaisesRegex(ValueError, "outside the private workspace"):
                export_bundle(self.root, alias / "snapshot.zip")

    def test_export_refuses_missing_configured_files(self):
        for path in ["output/cv.pdf", "applications/approved-answers.md"]:
            file = self.root / path
            data = file.read_bytes()
            file.unlink()
            with self.assertRaisesRegex(ValueError, "Configured files"):
                export_bundle(self.root, self.home / "missing.jhb")
            self.assertFalse((self.home / "missing.jhb").exists())
            file.write_bytes(data)

    def rewrite_archive(self, source, destination, change):
        with zipfile.ZipFile(source) as src, zipfile.ZipFile(destination, "w") as dst:
            files = {name: src.read(name) for name in src.namelist() if name != "manifest.json"}
            manifest = json.loads(src.read("manifest.json"))
            change(files, manifest)
            manifest["files"] = [
                {"path": name, "size": len(data), "sha256": hashlib.sha256(data).hexdigest()}
                for name, data in files.items()
            ]
            for name, data in files.items():
                dst.writestr(name, data)
            dst.writestr("manifest.json", json.dumps(manifest))

    def test_import_validates_profile_shape_configuration_and_required_files(self):
        source = self.home / "profile.jhb"
        export_bundle(self.root, source, "profile")

        def bad_config(files, _):
            config = json.loads(files["config/career-ledger.json"])
            config["answer_sources"] = [None]
            files["config/career-ledger.json"] = json.dumps(config).encode()

        changes = [
            lambda files, _: files.update({"applications/operations/state.json": b"{}"}),
            lambda files, _: files.pop("output/cv.pdf"),
            bad_config,
        ]
        for change in changes:
            destination = self.home / "invalid.jhb"
            self.rewrite_archive(source, destination, change)
            with self.assertRaises(ValueError):
                import_bundle(self.home / "invalid-workspace", destination)
            self.assertFalse((self.home / "invalid-workspace").exists())

    def test_full_import_does_not_add_onboarding_records(self):
        (self.root / "README.md").unlink()
        source = self.home / "source.jhb"
        export_bundle(self.root, source)
        restored = self.home / "restored"
        import_bundle(restored, source)
        self.assertFalse((restored / "README.md").exists())
        state = Harness(restored).ledger.load_state()
        original = self.h.ledger.load_state()
        self.assertEqual(state["events"][:-1], original["events"])
        self.assertTrue(state["events"][-1]["idempotency_key"].startswith("bundle:rebuild:"))
        self.assertEqual(state["runs"], original["runs"])

    def test_unicode_duplicate_names_and_expansion_limits_are_rejected(self):
        source = self.home / "source.jhb"
        export_bundle(self.root, source)
        duplicate = self.home / "duplicate.jhb"
        self.rewrite_archive(
            source,
            duplicate,
            lambda files, _: files.update(
                {
                    "context/caf\u00e9.md": b"a",
                    "context/cafe\u0301.md": b"b",
                }
            ),
        )
        with self.assertRaisesRegex(ValueError, "duplicate"):
            import_bundle(self.home / "duplicate", duplicate)
        with patch("job_harness.bundles.MAX_TOTAL_BYTES", 1):
            with self.assertRaises(ValueError):
                import_bundle(self.home / "large", source)
        self.assertFalse((self.home / "large").exists())

    def test_generic_aliases_are_exact_and_other_external_paths_are_denied(self):
        from job_harness.workspace import source_file

        p = self.root / "docs/career-operations.md"
        p.symlink_to(source_file("docs/career-operations.md"))
        result = self.h.call("read_record", {"path": "docs/career-operations.md"})
        self.assertTrue(result["ok"])
        self.assertEqual(result["data"]["origin"], "toolkit")
        p.unlink()
        outside = self.home / "outside.md"
        outside.write_text("Not a generic resource")
        p.symlink_to(outside)
        self.assertEqual(
            self.h.call("read_record", {"path": "docs/career-operations.md"})["error"]["code"],
            "path_denied",
        )


if __name__ == "__main__":
    unittest.main()
