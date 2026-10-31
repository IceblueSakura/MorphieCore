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
          llvmBin = "${toolchain}/lib/rustlib/${pkgs.stdenv.hostPlatform.rust.rustcTarget}/bin";
          llvmTools = pkgs.linkFarm "morphiecore-llvm-tools" (
            map
              (name: {
                name = "bin/${name}";
                path = "${llvmBin}/${name}";
              })
              [
                "llvm-cov"
                "llvm-profdata"
              ]
          );
          coverageEnv = {
            LLVM_COV = "${llvmTools}/bin/llvm-cov";
            LLVM_PROFDATA = "${llvmTools}/bin/llvm-profdata";
          };
          cargoLlvmCov =
            (pkgs.cargo-llvm-cov.override {
              rustPlatform = pkgs.makeRustPlatform {
                cargo = toolchain;
                rustc = toolchain;
              };
            }).overrideAttrs
              (old: {
                # Keep the package's own instrumented fixtures on the same toolchain.
                env = old.env // coverageEnv;
              });
          rust = pkgs.mkShell {
            name = "morphiecore-rust";
            packages = with pkgs; [
              toolchain
              cargoLlvmCov
              llvmTools
              # aws-lc-sys and ring compile native code.
              cmake
              pkg-config
            ];
            inherit (coverageEnv) LLVM_COV LLVM_PROFDATA;
            RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
          };
        in
        {
          inherit rust;
          default = pkgs.mkShell {
            name = "morphiecore-dev";
            inputsFrom = [ rust ];
            inherit (rust) RUST_SRC_PATH LLVM_COV LLVM_PROFDATA;
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
          coverage =
            pkgs.runCommand "morphiecore-coverage-check"
              {
                nativeBuildInputs = shell.rust.nativeBuildInputs;
                inherit (shell.rust) LLVM_COV LLVM_PROFDATA;
              }
              ''
                export CARGO_HOME="$TMPDIR/cargo-home"
                mkdir -p probe/src
                cd probe
                printf '%s\n' '[package]' 'name = "coverage-probe"' 'version = "0.1.0"' 'edition = "2024"' > Cargo.toml
                printf '%s\n' '#[test] fn coverage_probe() { assert_eq!(2 + 2, 4); }' > src/lib.rs
                cargo llvm-cov --version
                "$LLVM_COV" --version
                "$LLVM_PROFDATA" --version
                cargo llvm-cov --offline --lcov --output-path coverage.lcov
                grep -Eq '^DA:[0-9]+,[1-9][0-9]*' coverage.lcov
                touch "$out"
              '';
        }
      );
    };
}
