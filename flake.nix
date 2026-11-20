{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      inherit (nixpkgs) lib;

      genSystems = lib.genAttrs [
        "x86_64-linux"
        "aarch64-darwin"
      ];

      pkgs.overlays = [ rust-overlay.overlays.default ];

      get-pkgs =
        system:
        import nixpkgs {
          inherit system;
          inherit (pkgs) overlays;
        };

      eachSystem = f: genSystems (system: f (get-pkgs system));
    in
    {
      formatter = eachSystem (pkgs: pkgs.nixfmt-tree);

      devShells = eachSystem (
        pkgs:
        let
          inherit (pkgs.stdenv.hostPlatform) system;

          inherit (self.packages.${system})
            rust-toolchain
            rust-toolchain-with-coverage
            rustfmt-nightly
            ;

          setupHook = pkgs.runCommand "setup-hook" { } ''
            mkdir -p $out
            cp -r ${./.githooks} $out/.githooks
            chmod -R u+w $out/.githooks

            substituteInPlace $out/.githooks/pre-commit \
              --replace-fail "+nightly" ""
          '';

          mkToolchainShell =
            toolchain: extraPackages:
            pkgs.mkShell {
              env = {
                RUSTFMT = lib.getExe' rustfmt-nightly "rustfmt";
              }
              # Avoid build issue with non-ascii filepath
              # https://github.com/TermOxide/TermOxide/issues/253
              // (lib.optionalAttrs pkgs.stdenv.hostPlatform.isDarwin {
                NIX_CC_USE_RESPONSE_FILE = "0";
                NIX_LD_USE_RESPONSE_FILE = "0";
              });

              packages = [
                toolchain
                rustfmt-nightly
                self.formatter.${system}
              ]
              ++ extraPackages;

              shellHook = ''
                git config core.hooksPath "${setupHook}/.githooks"
              '';
            };
        in
        {
          default = self.devShells.${system}.basic-toolchain;

          basic-toolchain = mkToolchainShell rust-toolchain [ ];

          with-coverage = mkToolchainShell rust-toolchain-with-coverage (
            with pkgs;
            [
              cargo-tarpaulin
              cargo-llvm-cov
            ]
          );
        }
      );

      packages = eachSystem (
        pkgs:
        let
          toolchain = pkgs.rust-bin.stable.latest.default;

          toolchainArgs = {
            extensions = [
              "clippy"
              "rust-analyzer"
              "rust-src"
            ];
            targets = [ "wasm32-wasip2" ];
          };

        in
        {
          rust-toolchain = toolchain.override toolchainArgs;

          rust-toolchain-with-coverage = toolchain.override (
            toolchainArgs
            // {
              extensions = toolchainArgs.extensions ++ [ "llvm-tools" ];
            }
          );

          rustfmt-nightly = pkgs.rust-bin.selectLatestNightlyWith (
            toolchain: toolchain.minimal.override { extensions = [ "rustfmt" ]; }
          );
        }
      );
    };
}
