{
  # Vix as a Nix flake (T742, T743):
  #
  #   nix run github:vixide/vix            # try Vix without installing it
  #   nix run github:vixide/vix -- --version
  #   nix profile install github:vixide/vix
  #   nix build                            # from a checkout: ./result/bin/vix
  #
  # It builds the `vix` command (package `vix-tui`) from source with the
  # default features -- the same feature set as the release archives.
  # Not yet verified with a real `nix build` (see packaging/README.md), and
  # there is no committed flake.lock yet: the first `nix build` writes one.
  description = "Vix, a keyboard-friendly terminal text editor and IDE";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      # The release targets (dist-workspace.toml) that Nix also runs on.
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
    in
    {
      packages = forAllSystems (pkgs: rec {
        vix = pkgs.rustPlatform.buildRustPackage {
          pname = "vix-tui";
          inherit (cargoToml.workspace.package) version;
          src = pkgs.lib.cleanSource self;

          # Every dependency comes from crates.io, so the lockfile alone pins
          # them; no `outputHashes` are needed.
          cargoLock.lockFile = ./Cargo.lock;

          # Build only the terminal app, as `dist` does (`precise-builds`).
          cargoBuildFlags = [
            "--package"
            "vix-tui"
          ];

          # Tree-sitter grammars and other vendored C/C++ code.
          nativeBuildInputs = [ pkgs.pkg-config ];

          # The test suite is `scripts/check`'s job, on every forge's CI.
          doCheck = false;

          meta = {
            description = "Keyboard-friendly terminal text editor and IDE";
            homepage = "https://github.com/vixide/vix";
            # The `vix` binary's license (tui/Cargo.toml, spec/license).
            license = with pkgs.lib.licenses; [
              asl20
              bsd3
              mit
              gpl3Only
            ];
            mainProgram = "vix";
            platforms = systems;
          };
        };
        default = vix;
      });

      apps = forAllSystems (pkgs: rec {
        vix = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.vix}/bin/vix";
        };
        default = vix;
      });
    };
}
