fn main() {
    // TODO: Is it possible to specify filename with wildcard such as '*.c'?
    cc::Build::new()
        .file("src/entry.S")
        .file("src/entrypgdir.c")
        .include("src")
        .compile("foo");
}

