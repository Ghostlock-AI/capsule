# Syscall Reference for Trace Parsing

This document catalogs the syscalls we need to parse, their signatures from man pages, and real examples from our traces.

## How to Use This Document
1. When implementing a parser for a syscall, reference this document for the parameter types
2. Cross-reference with actual trace examples to understand strace output format
3. Update this document as we discover edge cases

---

## PROCESS SYSCALLS

### execve
**Signature**: `int execve(const char *pathname, char *const argv[], char *const envp[])`

**Parameters**:
- `pathname`: Path to executable file
- `argv[]`: Array of argument strings (NULL-terminated)
- `envp[]`: Array of environment strings (NULL-terminated)

**Return**:
- On success: does not return (process is replaced)
- On error: -1, errno is set

**Trace Examples**:
```
21:00:22.340048 [ 221] execve("/usr/bin/python3", ["python3", "scripts/process_syscalls.py"], ["HOSTNAME=5138b5b9b5fe", ...]) = 0

[pid  4120] 21:00:22.416065 [ 221] execve("/usr/local/cargo/bin/echo", ["echo", "Hello from child process"], [...]) = -1 ENOENT (No such file or directory)
```

**Strace Format**:
- Args: pathname (string), argv (array of strings), envp (array of strings)
- Success: `= 0`
- Error: `= -1 ERRNO (message)`

---

## FILE SYSCALLS

### openat
**Signature**: `int openat(int dirfd, const char *pathname, int flags, mode_t mode)`

**Parameters**:
- `dirfd`: Directory file descriptor (or AT_FDCWD for current directory)
- `pathname`: Path to file to open
- `flags`: File access mode and flags (O_RDONLY, O_WRONLY, O_RDWR, O_CREAT, etc.)
- `mode`: Permission mode (optional, only when O_CREAT is specified)

**Return**:
- On success: file descriptor (non-negative integer)
- On error: -1, errno is set

**Trace Examples**:
```
20:59:31.400594 [  56] openat(AT_FDCWD, "/etc/ld.so.cache", O_RDONLY|O_CLOEXEC) = 3</etc/ld.so.cache>

20:59:31.401018 [  56] openat(AT_FDCWD, "/lib/aarch64-linux-gnu/libpthread.so.0", O_RDONLY|O_CLOEXEC) = 3</lib/aarch64-linux-gnu/libpthread-2.31.so>

20:59:31.450720 [  56] openat(AT_FDCWD, "/working/mini-capsule/scripts", O_RDONLY|O_NONBLOCK|O_CLOEXEC|O_DIRECTORY) = 3</working/mini-capsule/scripts>
```

**Strace Format**:
- Args: dirfd (constant or fd), pathname (string), flags (OR'ed constants), [mode (octal)]
- Success: `= FD<resolved_path>`
- Error: `= -1 ERRNO (message)`
- Note: flags are OR'ed with `|` operator

---

### newfstatat (fstatat)
**Signature**: `int fstatat(int dirfd, const char *pathname, struct stat *statbuf, int flags)`

**Parameters**:
- `dirfd`: Directory file descriptor
- `pathname`: Path to file
- `statbuf`: Pointer to stat structure (memory address in trace)
- `flags`: Flags (AT_SYMLINK_NOFOLLOW, etc.)

**Return**:
- On success: 0
- On error: -1, errno is set

**Trace Examples**:
```
20:59:31.408307 [  79] newfstatat(AT_FDCWD, "/usr/local/cargo/bin/python3", 0xfffff9edb740, 0) = -1 ENOENT (No such file or directory)

20:59:31.408926 [  79] newfstatat(AT_FDCWD, "/usr/bin/python3", {st_dev=makedev(0, 0x4c), st_ino=8815141, st_mode=S_IFREG|0755, st_nlink=1, st_uid=0, st_gid=0, st_blksize=4096, st_blocks=10296, st_size=5268696, st_atime=1742436459, ...}, 0) = 0
```

**Strace Format**:
- Args: dirfd, pathname (string), statbuf (hex address or struct), flags (integer)
- Success: `= 0`, statbuf shows as struct with fields
- Error: `= -1 ERRNO (message)`, statbuf shows as hex address

---

### readlinkat
**Signature**: `ssize_t readlinkat(int dirfd, const char *pathname, char *buf, size_t bufsiz)`

**Parameters**:
- `dirfd`: Directory file descriptor
- `pathname`: Path to symbolic link
- `buf`: Buffer for link contents (memory address in trace)
- `bufsiz`: Buffer size

**Return**:
- On success: number of bytes placed in buffer
- On error: -1, errno is set

**Trace Examples**:
```
21:00:22.348689 [  78] readlinkat(AT_FDCWD, "/usr/bin/python3", "python3.9", 4096) = 9

21:00:22.348827 [  78] readlinkat(AT_FDCWD, "/usr/bin/python3.9", 0xffffdf523a40, 4096) = -1 EINVAL (Invalid argument)
```

**Strace Format**:
- Args: dirfd, pathname (string), buf (string on success, hex on error), bufsiz (integer)
- Success: `= bytes_read`, buf shows the link target as string
- Error: `= -1 ERRNO (message)`, buf shows hex address

---

### getcwd
**Signature**: `char *getcwd(char *buf, size_t size)`

**Parameters**:
- `buf`: Buffer for current directory (memory address in trace)
- `size`: Buffer size

**Return**:
- On success: pointer to buffer (shows path in strace)
- On error: NULL, errno is set

**Trace Examples**:
```
20:59:31.408159 [  17] getcwd("/working/mini-capsule", 4096) = 22
```

**Strace Format**:
- Args: buf (string on success, hex on error), size (integer)
- Success: `= bytes_returned`, buf shows the current directory path
- Error: `= -1 ERRNO (message)`

---

## NETWORK SYSCALLS

### socket
**Signature**: `int socket(int domain, int type, int protocol)`

**Parameters**:
- `domain`: Communication domain (AF_INET, AF_INET6, AF_UNIX, etc.)
- `type`: Socket type (SOCK_STREAM, SOCK_DGRAM, SOCK_RAW, can be OR'ed with SOCK_NONBLOCK, SOCK_CLOEXEC)
- `protocol`: Protocol (IPPROTO_TCP, IPPROTO_UDP, IPPROTO_IP, etc.)

**Return**:
- On success: socket file descriptor
- On error: -1, errno is set

**Trace Examples**:
```
[pid  3760] 13:01:09.180154 [ 198] socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC, IPPROTO_IP) = 3<TCP:[18898905]>

[pid  3758] 13:01:09.701456 [ 198] socket(AF_INET, SOCK_DGRAM|SOCK_CLOEXEC, IPPROTO_IP) = 4<UDP:[18907121]>
```

**Strace Format**:
- Args: domain (constant), type (OR'ed constants), protocol (constant)
- Success: `= FD<PROTO:[id]>` (shows socket type and internal ID)
- Error: `= -1 ERRNO (message)`

---

### bind
**Signature**: `int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen)`

**Parameters**:
- `sockfd`: Socket file descriptor
- `addr`: Socket address structure
- `addrlen`: Size of address structure

**Return**:
- On success: 0
- On error: -1, errno is set

**Trace Examples**:
```
[pid  3760] 13:01:09.183275 [ 200] bind(3<TCP:[18898905]>, {sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr("127.0.0.1")}, 16) = 0

[pid  3758] 13:01:09.701947 [ 200] bind(4<UDP:[18907121]>, {sa_family=AF_INET, sin_port=htons(0), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
```

**Strace Format**:
- Args: sockfd (with socket info), addr (struct with sa_family, sin_port, sin_addr), addrlen (integer)
- Success: `= 0`
- Error: `= -1 ERRNO (message)`
- Note: sockfd shows current socket state in angle brackets

---

### connect
**Signature**: `int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen)`

**Parameters**:
- `sockfd`: Socket file descriptor
- `addr`: Socket address structure
- `addrlen`: Size of address structure

**Return**:
- On success: 0
- On error: -1, errno is set

**Trace Examples**:
```
[pid  3758] 13:01:09.690808 [ 203] connect(4<TCP:[18907120]>, {sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
```

**Strace Format**:
- Same as bind

---

### listen
**Signature**: `int listen(int sockfd, int backlog)`

**Parameters**:
- `sockfd`: Socket file descriptor
- `backlog`: Maximum queue length for pending connections

**Return**:
- On success: 0
- On error: -1, errno is set

**Trace Examples**:
```
[pid  3760] 13:01:09.184730 [ 201] listen(3<TCP:[127.0.0.1:9999]>, 1) = 0
```

**Strace Format**:
- Args: sockfd (with bound address info), backlog (integer)
- Success: `= 0`
- Error: `= -1 ERRNO (message)`
- Note: After listen, sockfd shows the bound address

---

### accept / accept4
**Signature**:
- `int accept(int sockfd, struct sockaddr *addr, socklen_t *addrlen)`
- `int accept4(int sockfd, struct sockaddr *addr, socklen_t *addrlen, int flags)`

**Parameters**:
- `sockfd`: Listening socket file descriptor
- `addr`: Pointer to sockaddr structure (filled with peer address)
- `addrlen`: Pointer to address length (in/out parameter, shown as [len] in trace)
- `flags`: Flags for accept4 (SOCK_NONBLOCK, SOCK_CLOEXEC)

**Return**:
- On success: new socket file descriptor for accepted connection
- On error: -1, errno is set

**Trace Examples**:
```
[pid  3760] 13:01:09.693847 [ 242] accept4(3<TCP:[127.0.0.1:9999]>, {sa_family=AF_INET, sin_port=htons(32820), sin_addr=inet_addr("127.0.0.1")}, [16], SOCK_CLOEXEC) = 5<TCP:[127.0.0.1:9999->127.0.0.1:32820]>
```

**Strace Format**:
- Args: sockfd (listening socket info), addr (struct with peer info), addrlen (in brackets `[16]`), flags (for accept4)
- Success: `= FD<connection_info>` shows both local and remote addresses
- Error: `= -1 ERRNO (message)`

---

### sendto
**Signature**: `ssize_t sendto(int sockfd, const void *buf, size_t len, int flags, const struct sockaddr *dest_addr, socklen_t addrlen)`

**Parameters**:
- `sockfd`: Socket file descriptor
- `buf`: Buffer containing data to send
- `len`: Length of data
- `flags`: Send flags (MSG_CONFIRM, MSG_DONTROUTE, etc.)
- `dest_addr`: Destination address (NULL for connected sockets)
- `addrlen`: Size of destination address (0 for connected sockets)

**Return**:
- On success: number of bytes sent
- On error: -1, errno is set

**Trace Examples**:
```
[pid  3758] 13:01:09.695585 [ 206] sendto(4<TCP:[127.0.0.1:32820->127.0.0.1:9999]>, "Hello from client!", 18, 0, NULL, 0) = 18

[pid  3760] 13:01:09.699412 [ 206] sendto(5<TCP:[127.0.0.1:9999->127.0.0.1:32820]>, "Hello from server!", 18, 0, NULL, 0) = 18
```

**Strace Format**:
- Args: sockfd (with connection info), buf (string), len (integer), flags (integer), dest_addr (struct or NULL), addrlen (integer)
- Success: `= bytes_sent`
- Error: `= -1 ERRNO (message)`

---

### recvfrom
**Signature**: `ssize_t recvfrom(int sockfd, void *buf, size_t len, int flags, struct sockaddr *src_addr, socklen_t *addrlen)`

**Parameters**:
- `sockfd`: Socket file descriptor
- `buf`: Buffer to receive data
- `len`: Length of buffer
- `flags`: Receive flags (MSG_PEEK, MSG_WAITALL, etc.)
- `src_addr`: Source address (NULL if not needed)
- `addrlen`: Size of source address (NULL if not needed)

**Return**:
- On success: number of bytes received
- On error: -1, errno is set

**Trace Examples**:
```
[pid  3760] 13:01:09.699073 [ 207] recvfrom(5<TCP:[127.0.0.1:9999->127.0.0.1:32820]>,  <unfinished ...>
[pid  3760] 13:01:09.699190 [ 207] <... recvfrom resumed>"Hello from client!", 1024, 0, NULL, NULL) = 18

[pid  3758] 13:01:09.699168 [ 207] recvfrom(4<TCP:[127.0.0.1:32820->127.0.0.1:9999]>,  <unfinished ...>
[pid  3758] 13:01:09.699586 [ 207] <... recvfrom resumed>"Hello from server!", 1024, 0, NULL, NULL) = 18
```

**Strace Format**:
- Args: sockfd (with connection info), buf (string on success), len (integer), flags (integer), src_addr (struct or NULL), addrlen (struct or NULL)
- Success: `= bytes_received`, buf shows received data as string
- Error: `= -1 ERRNO (message)`
- Note: May show `<unfinished ...>` and `<... recvfrom resumed>` for blocking calls

---

## SPECIAL CASES & PATTERNS

### PID Prefixes
Some lines have `[pid XXXX]` prefix when tracing multi-process applications:
```
[pid  3760] 13:01:09.180154 [ 198] socket(...) = ...
```

### Unfinished Syscalls
Blocking syscalls may be interrupted and show across multiple lines:
```
[pid  3760] 13:01:09.699073 [ 207] recvfrom(5<...>,  <unfinished ...>
[pid  3760] 13:01:09.699190 [ 207] <... recvfrom resumed>"data", ...) = 18
```

### File Descriptor Annotations
strace with `-yy` flag shows file descriptor details in angle brackets:
- `3</path/to/file>` - file path
- `3<TCP:[id]>` - TCP socket with internal ID
- `3<TCP:[127.0.0.1:9999]>` - bound/listening TCP socket
- `3<TCP:[127.0.0.1:9999->127.0.0.1:32820]>` - connected TCP socket
- `4<UDP:[id]>` - UDP socket

### Flag Combinations
Flags are combined with bitwise OR (`|`):
```
O_RDONLY|O_NONBLOCK|O_CLOEXEC|O_DIRECTORY
SOCK_STREAM|SOCK_CLOEXEC
```

### Struct Fields
Structures show field values with `=`:
```
{sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr("127.0.0.1")}
{st_dev=makedev(0, 0x4c), st_ino=8815141, st_mode=S_IFREG|0755, ...}
```

### Error Format
Errors always follow this pattern:
```
= -1 ERRNO (Human readable message)
```
Common errno values: ENOENT, EINVAL, EACCES, ECONNREFUSED, etc.

---

## SYSCALL NUMBERS

**IMPORTANT**: Syscall numbers are shown in brackets `[ NUM ]` and vary by OS, kernel version, and CPU architecture.

On our example aarch64 Debian system:
- execve: 221
- openat: 56
- newfstatat: 79
- readlinkat: 78
- getcwd: 17
- socket: 198
- bind: 200
- connect: 203
- listen: 201
- accept4: 242
- sendto: 206
- recvfrom: 207

**DO NOT rely on syscall numbers for categorization or identification** - always use the syscall name instead. The same syscall will have different numbers on x86_64 vs aarch64 vs other architectures.
