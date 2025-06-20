{
  description = "rust dev env flake for bitlens";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs =
    {
      self,
      nixpkgs,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f {
            pkgs = nixpkgs.legacyPackages.${system};
          }
        );
    in
    {
      devShells = forAllSystems (
        { pkgs, ... }:
        {
          default = pkgs.mkShell.override { stdenv = pkgs.clangStdenv; } {
            packages = with pkgs; [
              cargo
              rustfmt
              clippy
              rustc
              cargo-flamegraph
              sqlite
              pprof
              graphviz
              libllvm
              llvmPackages_20.clang-unwrapped.lib
            ];
            shellHook = ''
              export LIBCLANG_PATH=${pkgs.libclang.lib}/lib
            '';
          };
        }
      );
    };
}
