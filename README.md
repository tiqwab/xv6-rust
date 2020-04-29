[xv6](https://github.com/mit-pdos/xv6-public) implementation in Rust.

```
### run OS
$ make qemu

### run OS with debug log
$ make DEBUG=1 qemu

### debug OS with GDB
$ make qemu-gdb

# in another console
$ make gdb
# (if required) reset symbol-file to debug user library
(gdb) symbol-file user/sh
...

### QEMU monitor (at the same console executing qemu)
# press Ctrl+A and then C
(qemu)
...
# press Ctrl+A and then C again to go back
```

A simple shell supports redirect and pipe.

```sh
$ ls
.            1 1 512
..           1 1 512
hello        2 2 21384
filetest     2 3 26128
sh           2 4 39604
argstest     2 5 21444
malloctest   2 6 26812
ls           2 7 31692
pwd          2 8 21436
mkdir        2 9 21528
echo         2 10 21644
whello       2 11 21744
cat          2 12 25880
pipetest     2 13 26132
wc           2 14 26528
console      3 15 0

$ ls > one.txt

$ cat one.txt
.            1 1 512
..           1 1 512
hello        2 2 21384
filetest     2 3 26128
sh           2 4 39604
argstest     2 5 21444
malloctest   2 6 26812
ls           2 7 31692
pwd          2 8 21436
mkdir        2 9 21528
echo         2 10 21644
whello       2 11 21744
cat          2 12 25880
pipetest     2 13 26132
wc           2 14 26528
console      3 15 0
one.txt      2 16 366

$ ls | wc
17 68 388
```
