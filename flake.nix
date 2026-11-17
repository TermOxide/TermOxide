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

      eachSystem =
        f:
        genSystems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      formatter = eachSystem (pkgs: pkgs.nixfmt-tree);

      devShells = eachSystem (
        pkgs:
        let
          stableToolchain = pkgs.rust-bin.stable.latest.default.override {
            # `rust-analyzer` puts the proc-macro server matching this compiler in
            # the sysroot, which the editor's rust-analyzer needs to expand derives.
            extensions = [
              "rust-src"
              "clippy"
              "rust-analyzer"
            ];
            targets = [ "wasm32-wasip2" ];
          };

          nightlyRustfmt = pkgs.rust-bin.selectLatestNightlyWith (
            toolchain: toolchain.minimal.override { extensions = [ "rustfmt" ]; }
          );
        in
        {
          default = pkgs.mkShell {
            packages = [
              # Rust
              stableToolchain
              nightlyRustfmt

              # Nix
              self.formatter.${pkgs.stdenv.hostPlatform.system}
            ];

            shellHook = ''
              export RUSTFMT="${nightlyRustfmt}/bin/rustfmt"
            '';
          };
        }
      );
    };
}
