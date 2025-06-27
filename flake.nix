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
        {
          name = "x86_64-linux";
          cargoHash = "sha256-DrBt2kJQvMyz9kEYCMzI/E/b/JzN1OrOXleFmQKt6vo=";
        }
        {
          name = "aarch64-linux";
          cargoHash = "sha256-DrBt2kJQvMyz9kEYCMzI/E/b/JzN1OrOXleFmQKt6vo=";
        }
        {
          name = "aarch64-darwin";
          cargoHash = "sha256-DrBt2kJQvMyz9kEYCMzI/E/b/JzN1OrOXleFmQKt6vo=";
        }
      ];
      genericGenAttrs =
        values: f:
        builtins.listToAttrs (
          map (v: {
            name = v.name;
            value = f v;
          }) values
        );
      forAllSystems =
        f:
        genericGenAttrs systems (
          { name, cargoHash }:
          f {
            pkgs = nixpkgs.legacyPackages.${name};
            inherit cargoHash;
          }
        );
    in
    {
      packages = forAllSystems (
        { pkgs, cargoHash }:
        let
          bin = pkgs.rustPlatform.buildRustPackage {
            pname = "bitlens";
            name = "bitlens";
            src = pkgs.lib.cleanSource ./.;
            inherit cargoHash;
            preBuild = ''
              # This fixes building the flake on an aarch64 github runner that has a /homeless-shelter present
              export HOME=$(pwd)
              export LIBCLANG_PATH=${pkgs.libclang.lib}/lib
            '';
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
            ];
            shellHook = ''
              export LIBCLANG_PATH=${pkgs.libclang.lib}/lib
            '';
          };
        }
      );
    };
}
