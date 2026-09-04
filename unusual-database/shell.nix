{ pkgs ? import <nixpkgs> {} }:

let
  # Creates a single derivation with symlinks to all Rust binaries
  rustToolchain = pkgs.symlinkJoin {
    name = "rust-toolchain";
    paths = [
      pkgs.rustc
      pkgs.cargo
      pkgs.rustfmt
      pkgs.clippy
    ];
  };
in
pkgs.mkShell {
  buildInputs = [
    rustToolchain
  ];

  # Expose standard library source for IDE autocompletion and navigation
  RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
}
