# home-manager module: the accessibility app as a launchd agent at login, reading
# $XDG_CONFIG_HOME/accessibility/config.toml. Activation copies the app to ~/Applications and signs
# it with `certificate`, so its Accessibility permission survives rebuilds; `accessibility setup`
# creates the certificate. The app logs to the unified log.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  service = config.services.accessibility;
  bundle = "${config.home.homeDirectory}/Applications/Accessibility.app";
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

    certificate = lib.mkOption {
      type = lib.types.str;
      example = "something-in-reality";
      description = "The code-signing identity in the login keychain that signs the app.";
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
      certificate=${lib.escapeShellArg service.certificate}
      if ! /usr/bin/security find-identity -v -p codesigning | grep -qF "\"$certificate\""; then
        echo "accessibility: no code-signing identity \"$certificate\"; run" \
          "accessibility setup --certificate $certificate" >&2
      elif [ "$(cat ${bundle}/Contents/source 2>/dev/null)" != "$source" ]; then
        run rm -rf ${bundle}
        run cp -R "$source" ${bundle}
        run chmod -R u+w ${bundle}
        run /usr/bin/codesign --force --sign "$certificate" ${bundle}
        echo "$source" > ${bundle}/Contents/source
        run /usr/bin/pkill -f ${bundle}/Contents/MacOS/accessibility || true
        run /bin/launchctl kickstart -k gui/$(id -u)/org.nix-community.home.accessibility || true
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
