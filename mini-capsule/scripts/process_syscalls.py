#!/usr/bin/env python3
"""
Script to generate process-related syscalls.
This will trigger: fork, clone, execve, wait, exit, etc.
"""

import os
import subprocess
import sys
import time

def main():
    print("Process syscalls demo starting...")

    # 1. fork() - via subprocess
    print("\n1. Creating child process (fork/clone)...")
    proc = subprocess.Popen(
        ["echo", "Hello from child process"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )

    # 2. wait() - wait for child
    print("2. Waiting for child process...")
    stdout, stderr = proc.communicate()
    print(f"   Child output: {stdout.decode().strip()}")

    # 3. execve() - execute another program
    print("\n3. Executing another program (execve)...")
    result = subprocess.run(
        ["python3", "-c", "print('Executed via execve')"],
        capture_output=True,
        text=True
    )
    print(f"   Result: {result.stdout.strip()}")

    # 4. Multiple child processes
    print("\n4. Creating multiple child processes...")
    processes = []
    for i in range(3):
        p = subprocess.Popen(
            ["sleep", "0.1"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE
        )
        processes.append(p)
        print(f"   Spawned process {i+1} with PID {p.pid}")

    # Wait for all
    for i, p in enumerate(processes):
        p.wait()
        print(f"   Process {i+1} completed")

    # 5. Get process info
    print("\n5. Process information...")
    print(f"   Current PID: {os.getpid()}")
    print(f"   Parent PID: {os.getppid()}")

    print("\nProcess syscalls demo complete!")

if __name__ == "__main__":
    main()