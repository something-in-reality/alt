# home-manager module: the accessibility binary as a launchd agent at login, reading
# $XDG_CONFIG_HOME/accessibility/config.toml.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  service = config.services.accessibility;
in
{
  options.services.accessibility = {
    enable = lib.mkEnableOption "keyboard, pointer, window and display behaviour outside the terminal";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      description = "The accessibility package.";
    };

    configFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "The configuration file; null leaves it unmanaged.";
    };

    path = lib.mkOption {
      type = lib.types.listOf lib.types.package;
      default = [ ];
      description = "Packages on the PATH of `exec` commands.";
    };
  };

  config = lib.mkIf service.enable {
    home.packages = [ service.package ];

    xdg.configFile."accessibility/config.toml" = lib.mkIf (service.configFile != null) {
      source = service.configFile;
    };

    launchd.agents.accessibility = {
      enable = true;
      config = {
        ProgramArguments = [
          (lib.getExe service.package)
          "run"
        ];
        RunAtLoad = true;
        KeepAlive = true;
        ProcessType = "Interactive";
        StandardErrorPath = "${config.home.homeDirectory}/Library/Logs/accessibility.log";
        EnvironmentVariables.PATH = lib.makeBinPath service.path + ":/usr/bin:/bin";
      };
    };
  };
}
