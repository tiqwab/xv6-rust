fn main() {
    // TODO: Is it possible to specify filename with wildcard such as '*.c'?
    cc::Build::new()
        .file("src/entry.S")
        .file("src/entrypgdir.c")
        .file("src/vectors.S")
        .file("src/alltraps.S")
        .file("src/mpentry.S")
        .file("src/kbdmap.c")
        .include("inc")
        .compile("xv6rustkernel");
}

