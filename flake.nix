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

      get-pkgs = system: import nixpkgs {
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

          inherit (self.packages.${system}) rust-toolchain rustfmt-nightly;

          setupHook = pkgs.runCommand "setup-hook" { } ''
            mkdir -p $out
            cp -r ${./.githooks} $out/.githooks
            chmod -R u+w $out/.githooks

            substituteInPlace $out/.githooks/pre-commit \
              --replace-fail "+nightly" ""
          '';
        in
        {
          default = pkgs.mkShell {
            env.RUSTFMT = lib.getExe' rustfmt-nightly "rustfmt";

            packages = [
              # Rust
              rust-toolchain
              rustfmt-nightly

              # Nix
              self.formatter.${system}
            ];

            shellHook = ''
              git config core.hooksPath "${setupHook}/.githooks"
            '';
          };
        }
      );

      packages = eachSystem (pkgs: {
        rust-toolchain = pkgs.rust-bin.stable.latest.default.override {
          # `rust-analyzer` puts the proc-macro server matching this compiler in
          # the sysroot, which the editor's rust-analyzer needs to expand derives.
          extensions = [
            "rust-src"
            "clippy"
            "rust-analyzer"
          ];
          targets = [ "wasm32-wasip2" ];
        };

        rustfmt-nightly = pkgs.rust-bin.selectLatestNightlyWith (
          toolchain: toolchain.minimal.override { extensions = [ "rustfmt" ]; }
        );
      });
    };
}
