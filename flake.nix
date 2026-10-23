{
  description = "Reusable MorphieCore development environments";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      shellsFor =
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          rust = pkgs.mkShell {
            name = "morphiecore-rust";
            packages = with pkgs; [
              toolchain
              # aws-lc-sys and ring compile native code.
              cmake
              pkg-config
            ];
            RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
          };
        in
        {
          inherit rust;
          default = pkgs.mkShell {
            name = "morphiecore-dev";
            inputsFrom = [ rust ];
            inherit (rust) RUST_SRC_PATH;
            packages = with pkgs; [
              python313
              uv
              nodejs_24
              git
              nixfmt
            ];
          };
        };
    in
    {
      devShells = forAllSystems shellsFor;
      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);
      checks = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          shell = shellsFor system;
        in
        {
          toolchain =
            pkgs.runCommand "morphiecore-toolchain-check"
              {
                nativeBuildInputs = shell.default.nativeBuildInputs;
              }
              ''
                rustc --version
                cargo --version
                cargo-clippy --version
                rustfmt --version
                python3 --version
                uv --version
                cmake --version
                pkg-config --version
                touch "$out"
              '';
        }
      );
    };
}
