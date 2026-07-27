{
  description = "Development environment for the Zed CEA extension";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs, ... }:
    let
      supportedSystems = [
        "aarch64-linux"
        "x86_64-linux"
      ];
      forEachSystem = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      packages = forEachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          source = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./grammar/src
              ./server
            ];
          };
          ceaLanguageServer = pkgs.rustPlatform.buildRustPackage {
            pname = "cea-language-server";
            version = "0.1.0";
            src = source;
            buildAndTestSubdir = "server";
            cargoRoot = "server";
            cargoLock.lockFile = ./server/Cargo.lock;
          };
          treeSitterCea = pkgs.tree-sitter.buildGrammar {
            language = "cea";
            version = "0.1.0";
            src = ./grammar;
          };
        in
        {
          default = ceaLanguageServer;
          cea-language-server = ceaLanguageServer;
          tree-sitter-cea = treeSitterCea;
        }
      );

      devShells = forEachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          wasiCc = pkgs.pkgsCross.wasi32.stdenv.cc;
        in
        {
          default = pkgs.mkShell {
            packages = [
              self.packages.${system}.cea-language-server
              pkgs.lua-language-server
              pkgs.nodejs
              pkgs.rustup
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
