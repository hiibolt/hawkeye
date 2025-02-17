{
  # Tremendous thanks to @oati for her help
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };
  outputs = { self, nixpkgs, rust-overlay, flake-utils }: 
    flake-utils.lib.eachDefaultSystem (system:
      let
        name = "hawkeye";
        version = "0.1.0";

        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rustVersion = pkgs.rust-bin.stable.latest.default;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustVersion;
          rustc = rustVersion;
        };

        localRustBuild = rustPlatform.buildRustPackage rec {
          pname = name;
          inherit version;
          src = ./.;
          cargoBuildFlags = "";
          meta = {
            mainProgram = name;
          };

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = [ (rustVersion.override { extensions = ["rust-src"]; }) ] ++ (with pkgs; [ 
            pkg-config
            cargo
            gcc
            rustfmt
            clippy
            openssl.dev
            openssh
            curl
            sqlite
            rust-analyzer
          ]);

          # Certain Rust tools won't work without this
          # This can also be fixed by using oxalica/rust-overlay and specifying the rust-src extension
          # See https://discourse.nixos.org/t/rust-src-not-found-and-other-misadventures-of-developing-rust-on-nixos/11570/3?u=samuela. for more details.
          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
          #LD_LIBRARY_PATH = libPath;
          OPENSSL_LIB_DIR = pkgs.openssl.out + "/lib";
        };
      in
      {
        packages = rec {
          ${name} = localRustBuild;

          docker = let
            bin = "${self.packages.${system}.${name}}/bin/${name}";
          in
            pkgs.dockerTools.buildLayeredImage {
              inherit name;
              tag = "latest";

              contents = with pkgs.dockerTools; [
                usrBinEnv
                binSh
                caCertificates
                fakeNss
              ] ++ (with pkgs; [
                openssl.dev
                openssh
                curl
                sqlite
                gnused
                gnugrep
                coreutils
              ]);

              config = {
                Entrypoint = [ bin ];
                Volumes = {
                  "/data" = { };
                  "/root/.ssh" = { };
                };
                ExposedPorts."5777/tcp" = { };
              };
            };
        };

        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [ cargo rustc ];
        };

    });
}