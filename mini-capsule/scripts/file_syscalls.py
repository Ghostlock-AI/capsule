#!/usr/bin/env python3
"""
Script to generate file I/O related syscalls.
This will trigger: open, read, write, close, stat, unlink, mkdir, etc.
"""

import os
import tempfile
import shutil

def main():
    print("File I/O syscalls demo starting...")

    # Create temporary directory for our tests
    temp_dir = tempfile.mkdtemp(prefix="capsule_test_")
    print(f"\nWorking in: {temp_dir}")

    try:
        # 1. open() and write() syscalls
        print("\n1. Creating and writing to a file...")
        test_file = os.path.join(temp_dir, "test.txt")
        with open(test_file, 'w') as f:
            f.write("Hello, this is test data!\n")
            f.write("Line 2\n")
            f.write("Line 3\n")
        print(f"   Wrote to {test_file}")

        # 2. open() and read() syscalls
        print("\n2. Reading from file...")
        with open(test_file, 'r') as f:
            content = f.read()
        print(f"   Read {len(content)} bytes")

        # 3. stat() syscall
        print("\n3. Getting file stats...")
        stats = os.stat(test_file)
        print(f"   Size: {stats.st_size} bytes")
        print(f"   Permissions: {oct(stats.st_mode)}")

        # 4. mkdir() syscall
        print("\n4. Creating directory...")
        subdir = os.path.join(temp_dir, "subdir")
        os.mkdir(subdir)
        print(f"   Created {subdir}")

        # 5. Multiple file operations
        print("\n5. Creating multiple files...")
        for i in range(5):
            filepath = os.path.join(subdir, f"file_{i}.txt")
            with open(filepath, 'w') as f:
                f.write(f"File number {i}\n")

        # 6. readdir() via listdir
        print("\n6. Listing directory contents...")
        files = os.listdir(subdir)
        print(f"   Found {len(files)} files: {files}")

        # 7. rename() syscall
        print("\n7. Renaming file...")
        old_name = os.path.join(temp_dir, "test.txt")
        new_name = os.path.join(temp_dir, "renamed.txt")
        os.rename(old_name, new_name)
        print(f"   Renamed to renamed.txt")

        # 8. Appending to file
        print("\n8. Appending to file...")
        with open(new_name, 'a') as f:
            f.write("Appended line\n")

        # 9. Binary file operations
        print("\n9. Binary file operations...")
        binary_file = os.path.join(temp_dir, "binary.dat")
        with open(binary_file, 'wb') as f:
            f.write(b'\x00\x01\x02\x03\x04\x05')

        with open(binary_file, 'rb') as f:
            binary_data = f.read()
        print(f"   Read {len(binary_data)} binary bytes")

        # 10. chmod() syscall
        print("\n10. Changing file permissions...")
        os.chmod(new_name, 0o644)
        print(f"   Changed permissions to 644")

        # 11. unlink() syscall (delete file)
        print("\n11. Deleting files...")
        os.unlink(binary_file)
        print(f"   Deleted {binary_file}")

    finally:
        # Cleanup - this will trigger more syscalls
        print("\n12. Cleaning up...")
        shutil.rmtree(temp_dir)
        print(f"   Removed {temp_dir}")

    print("\nFile I/O syscalls demo complete!")

if __name__ == "__main__":
    main()