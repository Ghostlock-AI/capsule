#!/usr/bin/env python3
"""
Test script to verify AppArmor profile restrictions.
This script tests all the permissions and restrictions defined in the profile.
"""

import os
import sys
import subprocess
from pathlib import Path
from typing import Tuple, List


class Color:
    """ANSI color codes for terminal output."""
    GREEN = '\033[92m'
    RED = '\033[91m'
    YELLOW = '\033[93m'
    BLUE = '\033[94m'
    BOLD = '\033[1m'
    END = '\033[0m'


class AppArmorTester:
    """Test AppArmor profile restrictions."""

    def __init__(self):
        self.tests_passed = 0
        self.tests_failed = 0
        self.tests_total = 0
        self.is_root = os.geteuid() == 0

    def print_header(self, message: str):
        """Print a formatted header."""
        print(f"\n{Color.BLUE}{Color.BOLD}{'=' * 70}{Color.END}")
        print(f"{Color.BLUE}{Color.BOLD}{message}{Color.END}")
        print(f"{Color.BLUE}{Color.BOLD}{'=' * 70}{Color.END}\n")

    def print_test(self, name: str, passed: bool, message: str = ""):
        """Print test result."""
        self.tests_total += 1
        if passed:
            self.tests_passed += 1
            status = f"{Color.GREEN}✓ PASS{Color.END}"
        else:
            self.tests_failed += 1
            status = f"{Color.RED}✗ FAIL{Color.END}"

        print(f"{status} - {name}")
        if message:
            print(f"      {message}")

    def test_read_access(self, path: str, should_succeed: bool = True) -> bool:
        """Test read access to a file or directory."""
        try:
            # Try to read
            if os.path.isdir(path):
                os.listdir(path)
            else:
                # For binary files, just check if we can open them
                with open(path, 'rb') as f:
                    f.read(1)
            success = True
            error = None
        except PermissionError as e:
            success = False
            error = str(e)
        except FileNotFoundError:
            # File doesn't exist - skip this test
            return True
        except Exception as e:
            success = False
            error = str(e)

        passed = (success == should_succeed)
        expected = "succeed" if should_succeed else "be denied"
        actual = "succeeded" if success else "was denied"

        msg = f"Expected to {expected}, {actual}"
        if error and not should_succeed and not success:
            msg += " ✓"
        elif error:
            msg += f" ({error})"

        return passed

    def test_write_access(self, path: str, should_succeed: bool = True) -> bool:
        """Test write access to a file."""
        try:
            # Try to write
            with open(path, 'w') as f:
                f.write("test")
            success = True
            error = None
            # Clean up if successful
            try:
                os.remove(path)
            except:
                pass
        except PermissionError as e:
            success = False
            error = str(e)
        except Exception as e:
            success = False
            error = str(e)

        passed = (success == should_succeed)
        return passed

    def test_execute_access(self, command: List[str], should_succeed: bool = True) -> bool:
        """Test execution of a command."""
        try:
            result = subprocess.run(
                command,
                capture_output=True,
                timeout=5,
                check=True
            )
            success = True
            error = None
        except subprocess.CalledProcessError as e:
            success = False
            error = f"Exit code {e.returncode}"
        except subprocess.TimeoutExpired:
            success = False
            error = "Timeout"
        except PermissionError as e:
            success = False
            error = str(e)
        except Exception as e:
            success = False
            error = str(e)

        passed = (success == should_succeed)
        return passed

    def run_all_tests(self):
        """Run all AppArmor restriction tests."""

        self.print_header("AppArmor Profile Test Suite")

        print(f"Running as: {Color.BOLD}{'root' if self.is_root else 'non-root user'}{Color.END}")
        print(f"UID: {os.geteuid()}")
        print(f"GID: {os.getegid()}")
        print(f"Working Directory: {os.getcwd()}")

        if self.is_root:
            print(f"\n{Color.YELLOW}Warning: Running as root - AppArmor restrictions may not apply{Color.END}")

        # Test 1: Read access to system binaries (SHOULD SUCCEED)
        self.print_header("Test Group 1: Read Access to System Binaries")

        test_paths = [
            ("/bin/bash", True, "Read /bin/bash"),
            ("/usr/bin/python3", True, "Read /usr/bin/python3"),
            ("/lib/x86_64-linux-gnu", True, "List /lib/ directory"),
        ]

        for path, should_succeed, description in test_paths:
            if os.path.exists(path):
                passed = self.test_read_access(path, should_succeed)
                self.print_test(description, passed)

        # Test 2: Write access to workspace (SHOULD SUCCEED)
        self.print_header("Test Group 2: Read/Write Access to Workspace")

        workspace_file = "/workspace/test_file.txt"
        passed = self.test_write_access(workspace_file, should_succeed=True)
        self.print_test("Write to /workspace/test_file.txt", passed)

        if os.path.exists(workspace_file):
            passed = self.test_read_access(workspace_file, should_succeed=True)
            self.print_test("Read from /workspace/test_file.txt", passed)
            try:
                os.remove(workspace_file)
            except:
                pass

        # Test 3: Write access to /tmp (SHOULD SUCCEED)
        tmp_file = "/tmp/test_file.txt"
        passed = self.test_write_access(tmp_file, should_succeed=True)
        self.print_test("Write to /tmp/test_file.txt", passed)

        # Test 4: Access to /etc (SHOULD BE DENIED for non-root)
        self.print_header("Test Group 3: Denied Access to /etc")

        if not self.is_root:
            etc_paths = [
                "/etc/passwd",
                "/etc/shadow",
                "/etc/hosts",
            ]

            for path in etc_paths:
                if os.path.exists(path):
                    passed = self.test_read_access(path, should_succeed=False)
                    self.print_test(f"Read {path} (should be DENIED)", passed)

        # Test 5: Access to /root (SHOULD BE DENIED for non-root)
        self.print_header("Test Group 4: Denied Access to /root")

        if not self.is_root and os.path.exists("/root"):
            passed = self.test_read_access("/root", should_succeed=False)
            self.print_test("List /root directory (should be DENIED)", passed)

        # Test 6: Access to .ssh directory (SHOULD BE DENIED)
        self.print_header("Test Group 5: Denied Access to .ssh")

        home = os.path.expanduser("~")
        ssh_dir = os.path.join(home, ".ssh")

        if os.path.exists(ssh_dir) and not self.is_root:
            passed = self.test_read_access(ssh_dir, should_succeed=False)
            self.print_test(f"Access {ssh_dir} (should be DENIED)", passed)

        # Test 7: Python execution (SHOULD SUCCEED)
        self.print_header("Test Group 6: Execute Python")

        passed = self.test_execute_access(
            ["python3", "-c", "print('Hello from Python')"],
            should_succeed=True
        )
        self.print_test("Execute Python script", passed)

        # Test 8: Access to /var/log (SHOULD BE DENIED except /var/log/capsule)
        self.print_header("Test Group 7: Access to /var/log")

        if not self.is_root:
            # Should be denied
            if os.path.exists("/var/log/syslog"):
                passed = self.test_read_access("/var/log/syslog", should_succeed=False)
                self.print_test("Read /var/log/syslog (should be DENIED)", passed)

        # Should be allowed
        capsule_log = "/var/log/capsule/test.log"
        os.makedirs("/var/log/capsule", exist_ok=True)
        passed = self.test_write_access(capsule_log, should_succeed=True)
        self.print_test("Write to /var/log/capsule/test.log (should be ALLOWED)", passed)

        # Test 9: Verify working directory
        self.print_header("Test Group 8: Working Directory")

        is_workspace = os.getcwd() == "/workspace"
        self.print_test("Default directory is /workspace", is_workspace)

        # Print summary
        self.print_header("Test Summary")

        total = self.tests_total
        passed = self.tests_passed
        failed = self.tests_failed
        pass_rate = (passed / total * 100) if total > 0 else 0

        print(f"Total Tests: {Color.BOLD}{total}{Color.END}")
        print(f"Passed:      {Color.GREEN}{passed}{Color.END}")
        print(f"Failed:      {Color.RED}{failed}{Color.END}")
        print(f"Pass Rate:   {Color.BOLD}{pass_rate:.1f}%{Color.END}")

        if self.is_root:
            print(f"\n{Color.YELLOW}Note: Some tests may not reflect AppArmor restrictions when running as root{Color.END}")

        return failed == 0


def main():
    """Main entry point."""
    tester = AppArmorTester()
    success = tester.run_all_tests()

    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
