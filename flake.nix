{
  description = "Build and serve an Antora documentation site, in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Builds the crate graph as two derivations rather than one: the
    # dependencies, from nothing but `Cargo.toml` and `Cargo.lock`, and then
    # the workspace on top of them. A change that does not touch the lock file
    # leaves the first one's hash alone, so CI's binary cache hands it back
    # and only the workspace crates are compiled again.
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
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

      craneLibFor = pkgs: (crane.mkLib pkgs).overrideToolchain (p: (rustBinFor p).stable.latest.minimal);

      # The same toolchain, told to emit for musl, over a musl *cross* stdenv so
      # that the tree-sitter grammars — which are C — are compiled for it too.
      # The compiler still runs natively; only what it produces changes.
      # Crane sees the cross stdenv and points cargo and the `cc` crate at its
      # compilers itself.
      #
      # `pkgsCross.musl64` and not `pkgsStatic`: the latter puts `-static` in
      # the linker flags for everything, build scripts and proc macros
      # included, and a proc macro has to be a shared object the compiler can
      # load. Under `pkgsStatic` every build script in the tree segfaults on
      # its first instruction.
      #
      # The toolchain is handed over as a function so that crane can ask for it
      # from the build platform's package set: the compiler runs natively, and
      # only its extra target is musl.
      staticCraneLibFor =
        pkgs:
        let
          musl = pkgs.pkgsCross.musl64;
        in
        (crane.mkLib musl).overrideToolchain (
          p:
          (rustBinFor p).stable.latest.minimal.override {
            targets = [ musl.stdenv.hostPlatform.rust.rustcTarget ];
          }
        );

      # What a dynamically linked build should look for at run time, or `null`
      # where the question does not arise. See `postFixup` in `antorsPackage`.
      runpathFor =
        pkgs:
        lib.optionals pkgs.stdenv.hostPlatform.isLinux [
          pkgs.stdenv.cc.libc
          pkgs.stdenv.cc.cc.libgcc
        ];

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

      # What every cargo derivation below starts from. `profile` is the one
      # thing that differs between them; everything else that decides how a
      # crate is compiled has to match between a package and the dependencies
      # it is handed, or cargo finds their fingerprints stale and builds them
      # all over again.
      commonArgsFor =
        {
          profile,
          rustflags ? null,
          doCheck,
        }:
        {
          pname = cargoToml.package.name;
          inherit (cargoToml.workspace.package) version;

          src = ownFiles;
          strictDeps = true;
          inherit doCheck;

          CARGO_PROFILE = profile;

          env = lib.optionalAttrs (rustflags != null) { RUSTFLAGS = rustflags; };
        };

      # The dependencies alone, from nothing but `Cargo.toml` and `Cargo.lock`.
      # Nothing downstream reads `cargo check`'s output, so that pass is
      # skipped; with `doCheck` on, the dev-dependencies are compiled too.
      depsFor = craneLib: args: craneLib.buildDepsOnly (args // { cargoCheckCommand = "true"; });

      antorsPackage =
        {
          lib,
          craneLib,
          rustflags ? null,
          runpath ? null,
        }:
        let
          # The profile the manifest keeps for what is shipped: one codegen
          # unit, link-time optimization, and the symbols stripped.
          #
          # No tests here: they are `checks.tests`, under a profile that does
          # not spend minutes link-time optimizing binaries that are run once
          # and thrown away. Leaving them out also leaves the dev-dependencies
          # out of what this has to compile.
          commonArgs = commonArgsFor {
            profile = "deploy";
            inherit rustflags;
            doCheck = false;
          };
        in
        craneLib.buildPackage (
          commonArgs
          // {
            cargoArtifacts = depsFor craneLib commonArgs;

            # The suites run against the glibc build only (see `checks.tests`),
            # so this is what is left to say that each build of the binary
            # starts at all — the one thing a static musl link is most likely
            # to get wrong without failing to link.
            doInstallCheck = true;
            installCheckPhase = ''
              runHook preInstallCheck
              $out/bin/antors --version
              runHook postInstallCheck
            '';

            # A full strip rather than the default `-S -p`. What is being removed
            # is not only size: the debug information names the store paths the
            # binary was linked against, and nix reads those as references — so an
            # unstripped binary drags a compiler's worth of shared objects it
            # never opens into the image behind it.
            stripAllList = [ "bin" ];

            # `libgcc_s.so.1` is the only file this binary ever opens out of
            # gcc's `lib` output, and that output is ten megabytes of libstdc++
            # and sanitizer runtimes it never touches — all of which the linker's
            # runpath drags into the image behind that one shared object.
            # nixpkgs also ships `libgcc_s.so.1` on its own, at 200 kB, and glibc
            # already puts that in the closure, so narrowing the runpath to what
            # is actually opened costs nothing and sheds the fat output.
            #
            # A static build opens nothing and wants no runpath at all.
            postFixup = lib.optionalString (runpath != null && runpath != [ ]) ''
              patchelf --set-rpath ${lib.makeLibraryPath runpath} $out/bin/antors
            '';

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
          }
        );

      # The container: the binary and the two shared objects it opens, and
      # nothing else. No shell, no `coreutils`, no package manager — an image
      # with a shell in it is an image someone will debug in, and there is
      # nothing in this one to debug.
      #
      # It is the static musl build, so the image is one file: no libc, no
      # loader, nothing to resolve at start-up.
      #
      # Which was worth measuring rather than assuming, because musl's
      # allocator very nearly made it impossible. A build of this shape is
      # almost entirely allocation, and musl's is slow enough at it to dominate
      # the run — 1697 ms against glibc's 739 ms on a 232-page site. Routing
      # only *Rust's* allocations through mimalloc does not fix that, because
      # most of them are not Rust's: the tree-sitter grammars are C and call
      # `malloc` themselves. mimalloc's `override` feature takes the C half
      # too, and under a static link it does hold — which is the whole reason
      # this is viable.
      #
      # What it costs, measured over nine interleaved runs of a 240-page site:
      # 207 ms against 184 ms for the same binary on glibc, so about 12%. What
      # it saves is every shared object in the image — 72.5 MB down to 33 MB.
      # That the gap is 12% and not 130% is the standing check that `override`
      # is still winning; if this ever regresses towards 2x, that is what
      # broke. Swap `antors-static` for `antors` below to take the other side.
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
          craneLib = craneLibFor final;
          runpath = runpathFor final;
        };
      };

      packages = forAllSystems (
        pkgs:
        {
          antors = pkgs.callPackage antorsPackage {
            craneLib = craneLibFor pkgs;

            runpath = runpathFor pkgs;
          };

          default = self.packages.${pkgs.stdenv.hostPlatform.system}.antors;
        }
        // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          # The same binary, linked against musl and depending on nothing.
          antors-static = pkgs.callPackage antorsPackage {
            craneLib = staticCraneLibFor pkgs;

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

            # The nightly `rustfmt` is dynamically linked against zlib and
            # carries no runpath that finds it, so it does not start in a shell
            # that has not put zlib where the loader looks — `cargo fmt` fails
            # with `libz.so.1: cannot open shared object file`. Nothing else
            # here needs this.
            LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.zlib ];
          };
        }
      );

      checks = forAllSystems (
        pkgs:
        let
          craneLib = craneLibFor pkgs;

          # `ci` rather than `deploy`: optimized, so the suites that build the
          # showcase finish quickly, but without LTO and with the debug
          # assertions and overflow checks a plain `cargo test` would have.
          # The suites drive the library rather than a checked-out site, and
          # the showcase they build is in `src`.
          commonArgs = commonArgsFor {
            profile = "ci";
            doCheck = true;
          };
        in
        {
          inherit (self.packages.${pkgs.stdenv.hostPlatform.system}) antors;

          tests = craneLib.cargoTest (commonArgs // { cargoArtifacts = depsFor craneLib commonArgs; });
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
