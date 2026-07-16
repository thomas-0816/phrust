#!/usr/bin/env python3
"""Focused tests for real WordPress response contracts."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from smoke import parse_args, web_response_contract_error


class WordPressResponseContractTests(unittest.TestCase):
    def test_rejects_http_200_wordpress_error_page(self) -> None:
        body = (
            "<html><head><title>WordPress &rsaquo; Error</title></head>"
            '<body id="error-page"><p class="wp-die-message">broken</p></body></html>'
        )
        error = web_response_contract_error("db-install", 200, body)
        self.assertIn("error page", error or "")

    def test_rejects_empty_frontpage(self) -> None:
        error = web_response_contract_error("web-frontpage", 200, "")
        self.assertIn("empty response body", error or "")

    def test_accepts_successful_install(self) -> None:
        body = "<html><h1>Success!</h1><p>WordPress has been installed.</p></html>"
        self.assertIsNone(web_response_contract_error("db-install", 200, body))

    def test_accepts_pre_config_wordpress_setup_page(self) -> None:
        body = (
            "<html><title>WordPress &rsaquo; Setup Configuration File</title>"
            "<h1>Before getting started</h1></html>"
        )
        self.assertIsNone(web_response_contract_error("web-install-page", 200, body))

    def test_accepts_login_form(self) -> None:
        body = (
            '<html><form id="loginform"><input name="log">'
            '<input name="pwd"></form></html>'
        )
        self.assertIsNone(web_response_contract_error("admin-login-page", 200, body))

    def test_accepts_post_install_frontpage(self) -> None:
        body = '<html><link href="/wp-content/themes/theme/style.css"></html>'
        self.assertIsNone(web_response_contract_error("post-install-frontpage", 200, body))

    def test_native_cache_cli_defaults_are_parseable(self) -> None:
        original = sys.argv
        try:
            sys.argv = ["smoke.py", "--native-cache", "read-write"]
            args = parse_args()
        finally:
            sys.argv = original
        self.assertEqual(args.native_cache, "read-write")
        self.assertTrue(args.native_cache_dir.endswith("native-cache"))


if __name__ == "__main__":
    unittest.main()
