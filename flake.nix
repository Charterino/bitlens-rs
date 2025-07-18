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
      packages = forAllSystems (
        { pkgs }:
        let
          bin = pkgs.rustPlatform.buildRustPackage.override { stdenv = pkgs.clangStdenv; } {
            pname = "bitlens";
            name = "bitlens";
            src = pkgs.lib.cleanSource ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            env = {
              LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
              TAR_OPTIONS = "--no-same-owner";
            };
          };

          dockerImage = pkgs.dockerTools.buildImage {
            name = "bitlens";
            tag = "latest";
            copyToRoot = [
              bin
              pkgs.cacert
            ];
            config = {
              WorkingDir = "/data";
              Cmd = [
                "/bin/bitlens-rs"
              ];
            };
          };
        in
        {
          inherit bin dockerImage;
          default = bin;
        }
      );

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
              heaptrack
            ];
            shellHook = ''
              export LIBCLANG_PATH=${pkgs.libclang.lib}/lib
            '';
          };
        }
      );
    };
}
