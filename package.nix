{
  lib,
  rustPlatform,
  zstd,
  pkg-config,
  source ? lib.cleanSource ./.,
}:
let
  src = source; # conflict with simple-revision-control
  cargoTOML = builtins.fromTOML
    (builtins.readFile (src + "/crates/termoxide_cli/Cargo.toml"));
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "termoxide_cli";

  inherit (cargoTOML.package) version;
  inherit src;

  __structuredAttrs = true;

  cargoLock.lockFile = ./Cargo.lock;

  cargoBuildFlags = [ "--package=termoxide_cli" ];
  cargoTestFlags = finalAttrs.cargoBuildFlags;

  nativeBuildInputs = [ pkg-config ];

  buildInputs = [ zstd ];

  meta = {
    description = "Runs a TermOxide app from its crate, with hot reload";
    license = lib.licenses.bsd2;
    mainProgram = "termoxide";
  };
})
