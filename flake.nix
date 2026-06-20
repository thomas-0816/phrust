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
          inherit (pkgs) lib stdenv;

          commonPackages = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer

            git
            curl
            wget
            cacert
            just
            jq
            ripgrep
            fd
            tree

            autoconf
            automake
            libtool
            bison
            re2c
            gnumake
            pkg-config
            ccache

            cmake
            ninja
            clang

            libxml2
            sqlite
            openssl
            zlib
            bzip2
            xz
            libzip

            python3
          ];

          linuxPackages = with pkgs; [
            gdb
            valgrind
          ];

          darwinPackages = with pkgs; [
            libiconv
          ];
        in
        {
          default = pkgs.mkShell {
            packages =
              commonPackages
              ++ lib.optionals stdenv.isLinux linuxPackages
              ++ lib.optionals stdenv.isDarwin darwinPackages;

            PHP_REF_SERIES = "8.5";
            PHP_REF_VERSION = "8.5.7";
            PHP_REF_TAG = "php-8.5.7";
            PHP_REF_REPO = "https://github.com/php/php-src.git";
            RUST_BACKTRACE = "1";

            shellHook = ''
              echo "phrust dev shell" >&2
              echo "  just help" >&2
              echo "  just verify" >&2
            '';
          };
        }
      );
    };
}
