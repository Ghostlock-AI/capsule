building the cli tool

```bash
# build
cargo install --path . --force
# run
minic
```

running strace on its own

```bash
strace --follow-forks -n -tt -v -yy -s 65535 -e trace=process,file,network -- ls
```
