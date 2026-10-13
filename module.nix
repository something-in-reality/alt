# home-manager module: the accessibility binary as a launchd agent at login, reading
# $XDG_CONFIG_HOME/accessibility/config.toml. Activation copies the app to ~/Applications and signs
# it with a designated requirement of its identifier alone, so its Accessibility permission
# survives rebuilds.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  service = config.services.accessibility;
  bundle = "${config.home.homeDirectory}/Applications/Accessibility.app";
  identifier = "dev.012.accessibility";
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

    # Copies and signs the app when the package changes, then restarts the agent onto it.
    home.activation.accessibility = lib.hm.dag.entryAfter [ "setupLaunchAgents" ] ''
      source=${service.package}/Applications/Accessibility.app
      if [ "$(cat ${bundle}/Contents/source 2>/dev/null)" != "$source" ]; then
        run rm -rf ${bundle}
        run cp -R "$source" ${bundle}
        run chmod -R u+w ${bundle}
        run /usr/bin/codesign --force --sign - --identifier ${identifier} \
          -r='designated => identifier "${identifier}"' ${bundle}
        echo "$source" > ${bundle}/Contents/source
        run /bin/launchctl kickstart -k gui/$(id -u)/org.nix-community.home.accessibility || true
      fi
    '';

    xdg.configFile."accessibility/config.toml" = lib.mkIf (service.configFile != null) {
      source = service.configFile;
    };

    launchd.agents.accessibility = {
      enable = true;
      config = {
        ProgramArguments = [
          "${bundle}/Contents/MacOS/accessibility"
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
