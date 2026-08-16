{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flakelight.url = "github:nix-community/flakelight";
  };

  outputs =
    { flakelight, ... }@inputs:
    flakelight ./. {
      inherit inputs;

      devShell =
        pkgs: with pkgs; rec {
          packages = [
            pkg-config
            pre-commit
            go_latest
            gotools
            golangci-lint
            gopls
          ];

          env = {
            LD_LIBRARY_PATH = lib.makeLibraryPath packages;
            GO111MODULE = "on";
            GOPROXY = "https://goproxy.cn";
          };

          shellHook = ''
            export PATH="$HOME/go/bin:$PATH"
          '';
        };
    };
}
