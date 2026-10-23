{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
  }: let
    inherit (nixpkgs) lib;

    genSystems = lib.genAttrs ["x86_64-linux" "aarch64-darwin"];

    eachSystem = f:
      genSystems (system:
        f (import nixpkgs {
          inherit system;
          overlays = [rust-overlay.overlays.default];
        }));
  in {
    formatter = eachSystem (pkgs: pkgs.alejandra);

    devShells = eachSystem (pkgs: let
      stableToolchain = pkgs.rust-bin.stable.latest.default.override {
        # `rust-analyzer` puts the proc-macro server matching this compiler in
        # the sysroot, which the editor's rust-analyzer needs to expand derives.
        extensions = ["rust-src" "clippy" "rust-analyzer"];
        targets = ["wasm32-wasip2"];
      };

      nightlyRustfmt = pkgs.rust-bin.selectLatestNightlyWith (
        toolchain: toolchain.minimal.override {extensions = ["rustfmt"];}
      );
    in {
      default = pkgs.mkShell {
        packages = with pkgs; [
          # Rust
          stableToolchain
          nightlyRustfmt

          # Nix
          alejandra
        ];

        shellHook = ''
          export RUSTFMT="${nightlyRustfmt}/bin/rustfmt"
        '';
      };
    });
  };
}
