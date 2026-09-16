{
  description = "Build and serve an Antora documentation site, in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # The back end, as a source tree rather than a flake: this repository takes
    # a *path* dependency on it while the two are developed together, so a build
    # has to put it back where the manifest expects to find it. Point this
    # elsewhere to build against a working copy:
    #
    #   nix build --override-input adocers path:../adocers
    adocers = {
      url = "github:AlexanderThaller/adocers";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      adocers,
    }:
    let
      inherit (nixpkgs) lib;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # The toolchain comes from `rust-overlay`, pinned by this flake's lock,
      # rather than from whatever nixpkgs the caller brought: the workspace asks
      # for a rustc newer than a release branch is likely to carry, which is no
      # reason to make the caller find a newer nixpkgs.
      rustBinFor = pkgs: rust-overlay.lib.mkRustBin { } pkgs;

      rustPlatformFor =
        pkgs:
        let
          toolchain = (rustBinFor pkgs).stable.latest.minimal;
        in
        pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

      # The same toolchain, told to emit for musl, over a musl *cross* stdenv so
      # that the tree-sitter grammars — which are C — are compiled for it too.
      # The compiler still runs natively; only what it produces changes.
      #
      # `pkgsCross.musl64` and not `pkgsStatic`: the latter puts `-static` in
      # the linker flags for everything, build scripts and proc macros
      # included, and a proc macro has to be a shared object the compiler can
      # load. Under `pkgsStatic` every build script in the tree segfaults on
      # its first instruction.
      staticRustPlatformFor =
        pkgs:
        let
          musl = pkgs.pkgsCross.musl64;

          toolchain = (rustBinFor pkgs).stable.latest.minimal.override {
            targets = [ musl.stdenv.hostPlatform.rust.rustcTarget ];
          };
        in
        musl.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

      cargoToml = lib.importTOML ./Cargo.toml;

      # Everything the build reads, and nothing else — a rendered
      # `resources/showcase/build` or a touched `target` must not invalidate it.
      ownFiles = lib.fileset.toSource {
        root = ./.;
        fileset = lib.fileset.unions [
          ./Cargo.toml
          ./Cargo.lock
          ./crates
          ./src
          ./tests
          ./README.md
          (lib.fileset.difference ./resources (lib.fileset.maybeMissing ./resources/showcase/build))
        ];
      };

      # The two checkouts, side by side, because that is the layout the path
      # dependency in `Cargo.toml` describes. Reproducing it here is what lets
      # the manifest stay as it is for local development.
      sourceFor =
        pkgs:
        pkgs.runCommandLocal "antors-source" { } ''
          mkdir -p $out/antors
          cp -R ${ownFiles}/. $out/antors/
          cp -R ${adocers}/. $out/adocers/
          chmod -R u+w $out
        '';

      antorsPackage =
        {
          lib,
          pkgs,
          rustPlatform,
          rustflags ? null,
        }:
        rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          inherit (cargoToml.workspace.package) version;

          src = sourceFor pkgs;
          sourceRoot = "antors-source/antors";

          cargoLock.lockFile = ./Cargo.lock;

          # The profile the manifest keeps for what is shipped: one codegen
          # unit, link-time optimization, and the symbols stripped.
          buildType = "deploy";

          # The suites drive the library rather than a checked-out site, and the
          # showcase they build is in `src`.
          doCheck = true;

          # A full strip rather than the default `-S -p`. What is being removed
          # is not only size: the debug information names the store paths the
          # binary was linked against, and nix reads those as references — so an
          # unstripped binary drags a compiler's worth of shared objects it
          # never opens into the image behind it.
          stripAllList = [ "bin" ];
        }
        // lib.optionalAttrs (rustflags != null) {
          env.RUSTFLAGS = rustflags;
        }
        // {

          meta = {
            inherit (cargoToml.package) description;
            homepage = "https://github.com/AlexanderThaller/antors";
            license = with lib.licenses; [
              mit
              asl20
            ];
            mainProgram = "antors";
            platforms = lib.platforms.unix;
          };
        };

      # The container: the binary and the two shared objects it opens, and
      # nothing else. No shell, no `coreutils`, no package manager — an image
      # with a shell in it is an image someone will debug in, and there is
      # nothing in this one to debug.
      #
      # It is the static musl build, so the image is one file: no libc, no
      # `/bin`, no loader.
      #
      # That was worth checking rather than assuming. A build of this shape is
      # almost entirely allocation, and musl's allocator is slow enough at it
      # to dominate the run — against a 232-page site, 1697 ms where glibc took
      # 739 ms. Routing only *Rust's* allocations through mimalloc does not fix
      # it, because most of them are not Rust's: the tree-sitter grammars are C
      # and call `malloc` themselves. mimalloc's `override` feature takes the C
      # half too, and that closes it — 850 ms, against 741 ms for the same
      # binary on glibc.
      #
      # So the image costs 15% of build time and saves the whole of libc.
      containerFor =
        pkgs:
        let
          antors = self.packages.${pkgs.stdenv.hostPlatform.system}.antors-static;
        in
        pkgs.dockerTools.buildLayeredImage {
          name = "antors";
          tag = cargoToml.workspace.package.version;

          # `WorkingDir` has to exist for a run that does *not* mount over it —
          # `antors --help`, or a playbook given by absolute path. `/tmp` is
          # there because `-o /tmp/...` is the obvious way to build a site
          # whose output you do not want to keep, and an image with no
          # writable directory at all fails that with `Permission denied`.
          extraCommands = ''
            mkdir -p site tmp
            chmod 1777 tmp
          '';

          config = {
            # The store path itself rather than a symlink at the root, which is
            # what keeps even `/bin` out of the image.
            Entrypoint = [ (lib.getExe antors) ];

            # A site is mounted here and the playbook named relative to it, so
            # the common case is `docker run -v "$PWD:/site" antors`. Add
            # `--user "$(id -u):$(id -g)"` and the pages it writes belong to you
            # rather than to root.
            WorkingDir = "/site";

            # `antors serve` defaults to loopback, which reaches nothing from
            # outside the container; `--bind 0.0.0.0:4000` is what makes this
            # port worth exposing.
            ExposedPorts = {
              "4000/tcp" = { };
            };

            Labels = {
              "org.opencontainers.image.title" = "antors";
              "org.opencontainers.image.description" = cargoToml.package.description;
              "org.opencontainers.image.source" = "https://github.com/AlexanderThaller/antors";
              "org.opencontainers.image.version" = cargoToml.workspace.package.version;
              "org.opencontainers.image.licenses" = "MIT OR Apache-2.0";
            };
          };
        };
    in
    {
      overlays.default = final: _prev: {
        antors = final.callPackage antorsPackage {
          pkgs = final;
          rustPlatform = rustPlatformFor final;
        };
      };

      packages = forAllSystems (
        pkgs:
        {
          antors = pkgs.callPackage antorsPackage {
            inherit pkgs;
            rustPlatform = rustPlatformFor pkgs;
          };

          default = self.packages.${pkgs.stdenv.hostPlatform.system}.antors;
        }
        // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          # The same binary, linked against musl and depending on nothing.
          antors-static = pkgs.callPackage antorsPackage {
            inherit pkgs;
            rustPlatform = staticRustPlatformFor pkgs;

            # nixpkgs builds for musl dynamically by default, against a loader
            # in the store that a container would then have to carry. Asking
            # for the static C runtime is what makes the result one file.
            rustflags = "-C target-feature=+crt-static";
          };

          container = containerFor pkgs;
        }
      );

      apps = forAllSystems (pkgs: rec {
        antors = {
          type = "app";
          program = lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.antors;
          meta.description = "Build and serve an Antora documentation site";
        };

        default = antors;
      });

      devShells = forAllSystems (
        pkgs:
        let
          rustBin = rustBinFor pkgs;
        in
        {
          default = pkgs.mkShell {
            packages = [
              (rustBin.stable.latest.minimal.override {
                extensions = [
                  "clippy"
                  "rust-analyzer"
                  "rust-src"
                ];
              })

              # `.rustfmt.toml` asks for options only a nightly rustfmt accepts.
              # It is a separate toolchain so that nothing else here is nightly.
              (rustBin.selectLatestNightlyWith (
                toolchain: toolchain.minimal.override { extensions = [ "rustfmt" ]; }
              ))

              # `resources/showcase/compare.sh` builds the same site with Antora
              # and diffs the two; it is the only thing here that wants Node.
              pkgs.nodejs

              pkgs.git
            ];
          };
        }
      );

      checks = forAllSystems (pkgs: {
        inherit (self.packages.${pkgs.stdenv.hostPlatform.system}) antors;
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
