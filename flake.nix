{
  description = "Rust PHP 8.5 engine development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          isDarwin = nixpkgs.lib.hasSuffix "-darwin" system;

          # Tooling both shells provide. Darwin deliberately uses the host
          # Rust toolchain and host clang; Linux pins the full toolchain and
          # native build inputs below.
          commonPackages = with pkgs; [
            git
            just
            jq
            hyperfine
            ripgrep
            fd
            sccache
            cargo-nextest
            cargo-deny
            cargo-machete
            cargo-llvm-cov
            cargo-mutants
            cargo-fuzz
            cargo-semver-checks
            sonar-scanner-cli
            file
            libxml2
            pkg-config
            python3
          ];

          linuxPackages = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            llvmPackages.llvm

            curl
            cacert
            tzdata

            autoconf
            automake
            libtool
            bison
            re2c
            gnumake
            cmake
            ninja
            clang
            llvmPackages.libclang
            sqlite
            openssl
            zlib
            bzip2
            xz
            libzip
            pcre2

            gdb
            mold
            valgrind
            perf
            shellcheck
          ];

          commonEnv = {
            PHP_REF_SERIES = "8.5";
            PHP_REF_VERSION = "8.5.7";
            PHP_REF_TAG = "php-8.5.7";
            PHP_REF_REPO = "https://github.com/php/php-src.git";
            PHPRUST_LIBMAGIC_LIB_DIR = "${pkgs.file}/lib";
            PHPRUST_LIBMAGIC_INCLUDE_DIR = "${pkgs.file.dev}/include";
            RUST_BACKTRACE = "1";
            CARGO_REGISTRIES_CRATES_IO_PROTOCOL = "sparse";
            SCCACHE_CACHE_SIZE = "20G";
          };

          commonHook = ''
            export CARGO_TARGET_DIR="$PWD/target"
            export SCCACHE_DIR="$PWD/.cache/sccache"
            mkdir -p "$CARGO_TARGET_DIR" "$SCCACHE_DIR"
            phrust_workspace_root="$(git rev-parse --show-toplevel 2>/dev/null || printf '%s' "$PWD")"
            if [ -x "$phrust_workspace_root/scripts/development/parallel_php_vm_rustc.sh" ]; then
              export RUSTC_WRAPPER="$phrust_workspace_root/scripts/development/parallel_php_vm_rustc.sh"
            else
              unset RUSTC_WRAPPER
              printf '%s\n' '[skip] project rustc wrapper unavailable; using rustc directly' >&2
            fi
            unset phrust_workspace_root
          '';

          bannerHook = name: ''
            printf '%s\n' '${name}' >&2
            printf '  Cargo target: %s\n' "$CARGO_TARGET_DIR" >&2
            printf '%s\n' '  just help' >&2
            printf '%s\n' '  just verify' >&2
          '';
        in
        {
          default =
            if isDarwin then
              pkgs.mkShellNoCC (
                commonEnv
                // {
                  packages = commonPackages;
                  shellHook =
                    commonHook
                    + ''
                      if command -v sccache >/dev/null 2>&1; then
                        export PHRUST_RUSTC_CACHE_WRAPPER="$(command -v sccache)"
                      else
                        unset PHRUST_RUSTC_CACHE_WRAPPER
                        printf '%s\n' '[skip] sccache unavailable; using host rustc directly' >&2
                      fi
                    ''
                    + bannerHook "phrust Darwin host dev shell";
                }
              )
            else
              pkgs.mkShellNoCC (
                commonEnv
                // {
                  packages = commonPackages ++ linuxPackages;
                  PHRUST_RUSTC_CACHE_WRAPPER = "${pkgs.sccache}/bin/sccache";
                  CARGO_INCREMENTAL = "0";
                  LLVM_COV = "${pkgs.llvmPackages.llvm}/bin/llvm-cov";
                  LLVM_PROFDATA = "${pkgs.llvmPackages.llvm}/bin/llvm-profdata";
                  shellHook =
                    commonHook
                    + ''
                      export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
                      export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang
                      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=clang
                      case " ''${RUSTFLAGS:-} " in
                        *" -C link-arg=-fuse-ld=mold "*) ;;
                        *) export RUSTFLAGS="-C link-arg=-fuse-ld=mold ''${RUSTFLAGS:-}" ;;
                      esac
                    ''
                    + bannerHook "phrust dev shell";
                }
              );
        }
      );
    };
}
