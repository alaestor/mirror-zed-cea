fn main() {
    let grammar_dir = "../grammar/src";

    cc::Build::new()
        .include(grammar_dir)
        .file(format!("{grammar_dir}/parser.c"))
        .flag_if_supported("-Wno-unused-label")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .compile("tree-sitter-cea");

    println!("cargo:rerun-if-changed={grammar_dir}/parser.c");
    println!("cargo:rerun-if-changed={grammar_dir}/tree_sitter/parser.h");
}
