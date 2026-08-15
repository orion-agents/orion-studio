{ inputs, ... }:
{
  flake.overlays.default =
    final: _:
    let
      mkOrionStudio = import ../toolchain.nix { inherit inputs; };
      orionStudio = mkOrionStudio final;
    in
    {
      orion-studio = orionStudio;
      # Compatibility alias for existing overlay consumers.
      zed-editor = orionStudio;
    };
}
