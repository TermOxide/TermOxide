{ pkgs ? import <nixpkgs> {
    overlays = [ (import (builtins.fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz")) ];
  }
}:
let
  nightlyRustfmt = pkgs.rust-bin.selectLatestNightlyWith (toolchain:
    toolchain.minimal.override { extensions = [ "rustfmt" ]; });
in
pkgs.mkShell {
  buildInputs = [ nightlyRustfmt ];
  shellHook = ''
    export RUSTFMT="${nightlyRustfmt}/bin/rustfmt"
  '';
}
