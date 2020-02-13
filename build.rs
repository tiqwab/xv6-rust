fn main() {
    // TODO: Is it possible to specify filename with wildcard such as '*.c'?
    cc::Build::new()
        .file("src/entry.S")
        .file("src/entrypgdir.c")
        .file("src/vectors.S")
        .include("src")
        .compile("foo");
}

