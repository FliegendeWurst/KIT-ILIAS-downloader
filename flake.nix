{
  description = "Download content from ilias.studium.kit.edu";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, crane, ... }: let
    systems = [ "x86_64-linux" ];
    inherit (nixpkgs) lib;
    forEachSystem = lib.genAttrs systems;
    craneLib = forEachSystem (system: crane.lib.${system});

    toHydraJob = with lib; foldlAttrs
      (jobset: system: attrs: recursiveUpdate jobset
        (mapAttrs (const (drv: { ${system} = drv; }))
          (filterAttrs (name: const (name != "default")) attrs)))
      { };

    builds = forEachSystem (system: (lib.fix (final: {
      common = {
        pname = "KIT-ILIAS-Downloader";
        src = craneLib.${system}.cleanCargoSource self;
      };
      cargoArtifacts = craneLib.${system}.buildDepsOnly (final.common // {
        doCheck = false;
      });
      clippy = craneLib.${system}.cargoClippy (final.common // {
        inherit (final) cargoArtifacts;
        cargoClippyExtraArgs = lib.escapeShellArgs [
          "--all-targets"
          "--"
          "-D"
          "warnings"
          "-A"
          "non-snake-case"
          "-A"
          "clippy::upper-case-acronyms"
        ];
      });
      format = craneLib.${system}.cargoFmt (final.common // {
        inherit (final) cargoArtifacts;
      });
      kit-ilias-downloader = craneLib.${system}.buildPackage (final.common // {
        inherit (final) cargoArtifacts;
        doCheck = false;
        meta.license = lib.licenses.gpl3Plus;
        meta.platforms = systems;
      });
    })));
  in {
    packages = forEachSystem (system: {
      default = self.packages.${system}.kit-ilias-downloader;
      inherit (builds.${system}) kit-ilias-downloader;
    });
    checks = forEachSystem (system: {
      inherit (builds.${system}) format clippy;
    });
    hydraJobs = {
      packages = toHydraJob self.packages;
      checks = toHydraJob self.checks;
    };
  };
}
