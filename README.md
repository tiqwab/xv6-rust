```
### run OS
$ make qemu

### run OS with debug log
$ make DEBUG=1 qemu

### debug OS with GDB
$ make qemu-gdb

# in another console
$ make gdb
# reset symbol-file to debug OS not bootloader
(gdb) symbol-file target/i686-xv6rust/debug/xv6-rust
...

# QEMU monitor (at the same console executing qemu)
# press Ctrl+A and then C
(qemu)
...
# press Ctrl+A and then C again to go back
```
