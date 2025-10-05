#!/usr/bin/env python3
"""
Script to generate network-related syscalls.
This will trigger: socket, connect, bind, listen, accept, send, recv, etc.
"""

import socket
import time
import threading

def start_server(port):
    """Start a simple TCP server"""
    # socket() syscall
    server_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server_socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

    # bind() syscall
    server_socket.bind(('127.0.0.1', port))

    # listen() syscall
    server_socket.listen(1)
    print(f"Server listening on 127.0.0.1:{port}")

    # Set timeout so we don't block forever
    server_socket.settimeout(2.0)

    try:
        # accept() syscall
        client_socket, address = server_socket.accept()
        print(f"Server accepted connection from {address}")

        # recv() syscall
        data = client_socket.recv(1024)
        print(f"Server received: {data.decode()}")

        # send() syscall
        client_socket.send(b"Hello from server!")

        client_socket.close()
    except socket.timeout:
        print("Server timeout, no connection received")
    finally:
        server_socket.close()

def main():
    print("Network syscalls demo starting...")

    port = 9999

    # Start server in background thread
    print("\n1. Creating TCP server socket...")
    server_thread = threading.Thread(target=start_server, args=(port,))
    server_thread.start()

    # Give server time to start
    time.sleep(0.5)

    # 2. Create client and connect
    print("\n2. Creating client socket and connecting...")
    client_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)

    try:
        # connect() syscall
        client_socket.connect(('127.0.0.1', port))
        print(f"Client connected to 127.0.0.1:{port}")

        # send() syscall
        print("\n3. Sending data...")
        client_socket.send(b"Hello from client!")

        # recv() syscall
        print("\n4. Receiving data...")
        response = client_socket.recv(1024)
        print(f"Client received: {response.decode()}")

    except Exception as e:
        print(f"Client error: {e}")
    finally:
        client_socket.close()

    # Wait for server thread
    server_thread.join()

    # 5. UDP socket example
    print("\n5. Creating UDP socket...")
    udp_socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp_socket.bind(('127.0.0.1', 0))
    print(f"UDP socket bound to {udp_socket.getsockname()}")
    udp_socket.close()

    print("\nNetwork syscalls demo complete!")

if __name__ == "__main__":
    main()