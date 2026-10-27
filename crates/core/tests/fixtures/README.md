# WIM interoperability fixture

`wimlib-xpress.wim` was generated with wimlib-imagex 1.14.4, independently of the GeeZipX writer:

```sh
wimlib-imagex capture SOURCE wimlib-xpress.wim --compress=XPRESS --no-acls
```

SOURCE contains `empty` (zero bytes), `empty-dir/`, and `目录/示例.txt` with `b"WIM external fixture\n"` repeated 4000 times. This exercises the standard nameless root dentry, 102-byte dentry fixed header, XPRESS chunks, Unicode names, and empty file/directory handling. All fixture content is original regression test data.
