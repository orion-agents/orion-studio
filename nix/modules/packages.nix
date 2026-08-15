{ inputs, ... }:
{
  perSystem =
    {
      pkgs,
      lib,
      system,
      ...
    }:
    let
      mkOrionStudio = import ../toolchain.nix { inherit inputs; };
      orionStudio = mkOrionStudio pkgs;
    in
    {
      packages = {
        default = orionStudio;
        orion-studio = orionStudio;
        debug = orionStudio.override { profile = "dev"; };
        # Compatibility alias for existing flake consumers.
        zed-editor = orionStudio;
      };
    }
    // lib.optionalAttrs (lib.hasSuffix "linux" system) {
      checks = {
        a11y-test = import ../tests/a11y.nix {
          inherit pkgs inputs;
        };
      }
      // import ../tests/sandboxing { inherit pkgs inputs; };
    };
}
