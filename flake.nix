{
  description = "Reusable MorphieCore development environments";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      nodeVersion = (builtins.fromJSON (builtins.readFile ./package.json)).engines.node;
      shellsFor =
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          rust = pkgs.mkShell {
            name = "morphiecore-rust";
            packages = with pkgs; [
              cargo
              rustc
              clippy
              rustfmt
              rust-analyzer
              # aws-lc-sys and ring compile native code.
              cmake
              pkg-config
            ];
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };
        in
        {
          inherit rust;
          default =
            assert pkgs.lib.assertMsg (
              pkgs.nodejs_26.version == nodeVersion
            ) "The locked nixpkgs Node version must match package.json engines.node.";
            pkgs.mkShell {
              name = "morphiecore-dev";
              inputsFrom = [ rust ];
              inherit (rust) RUST_SRC_PATH;
              packages = with pkgs; [
                nodejs_26
                python313
                uv
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
                test "$(node -p process.versions.node)" = ${pkgs.lib.escapeShellArg nodeVersion}
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
