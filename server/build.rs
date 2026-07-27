fn main() {
    let grammar_dir = "../grammar/src";

    cc::Build::new()
        .include(grammar_dir)
        .file(format!("{grammar_dir}/parser.c"))
        .compile("tree-sitter-cea");

    println!("cargo:rerun-if-changed={grammar_dir}/parser.c");
    println!("cargo:rerun-if-changed={grammar_dir}/tree_sitter/parser.h");
}
