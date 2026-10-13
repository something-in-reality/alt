# home-manager module: the accessibility app as a launchd agent at login, reading
# $XDG_CONFIG_HOME/accessibility/config.toml. Activation copies the app to ~/Applications and signs
# it with rcodesign under an identity derived from `seed`: every build meets one designated
# requirement, so its Accessibility permission survives rebuilds. The app logs to the unified log.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  service = config.services.accessibility;
  bundle = "${config.home.homeDirectory}/Applications/Accessibility.app";
  signed = "${config.xdg.stateHome}/accessibility/signed";
  agent = config.launchd.agents.accessibility.config.Label;
  identifier = "something-in-reality.accessibility";
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

    seed = lib.mkOption {
      type = lib.types.str;
      example = "/Users/me/infrastructure.key";
      description = ''
        Absolute path of the 64-hex-character seed file that the app's signing identity derives
        from; activation reads it in place.
      '';
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
      app=${service.package}/Applications/Accessibility.app
      if [ "$(cat ${lib.escapeShellArg signed} 2>/dev/null)" != "$app" ]; then
        run rm -rf ${lib.escapeShellArg bundle}
        run cp -R "$app" ${lib.escapeShellArg bundle}
        run chmod -R u+w ${lib.escapeShellArg bundle}
        ${service.package}/bin/signing-identity ${lib.escapeShellArg service.seed} \
          | run ${lib.getExe pkgs.rcodesign} sign --timestamp-url none --pem-file /dev/stdin \
            ${lib.escapeShellArg bundle}
        run mkdir -p ${lib.escapeShellArg (dirOf signed)}
        echo "$app" | run --quiet tee ${lib.escapeShellArg signed} >/dev/null
        run /bin/launchctl kickstart -k gui/$(/usr/bin/id -u)/${agent} || true
      fi
    '';

    xdg.configFile."accessibility/config.toml" = lib.mkIf (service.configFile != null) {
      source = service.configFile;
    };

    # The agent runs the app's executable, outside the store, so System Settings names it after
    # the app.
    launchd.agents.accessibility = {
      enable = true;
      waitForNixStore = false;
      config = {
        ProgramArguments = [ "${bundle}/Contents/MacOS/accessibility" ];
        AssociatedBundleIdentifiers = identifier;
        EnvironmentVariables.PATH = lib.makeBinPath service.path + ":/usr/bin:/bin";
        RunAtLoad = true;
        KeepAlive = true;
        ProcessType = "Interactive";
      };
    };
  };
}
