{
  description = "Development environment for the Zed CEA extension";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      supportedSystems = [
        "aarch64-linux"
        "x86_64-linux"
      ];
      forEachSystem = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      devShells = forEachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          wasiCc = pkgs.pkgsCross.wasi32.stdenv.cc;
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.nodejs
              pkgs.tree-sitter
              pkgs.stdenv.cc
            ];

            TREE_SITTER = "${pkgs.tree-sitter}/bin/tree-sitter";
            WASI_CLANG = "${wasiCc}/bin/${wasiCc.targetPrefix}cc";
            WASI_TOOLCHAIN_PATH = pkgs.lib.makeBinPath [
              wasiCc
              wasiCc.bintools
              pkgs.llvmPackages.lld
            ];
          };
        }
      );
    };
}
