fn main() {
    cc::Build::new()
        .file("src/add.c")
        .file("src/nop.S")
        .include("src")
        .compile("foo");
}

