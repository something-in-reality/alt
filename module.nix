# home-manager module: the alt app as a launchd agent at login, reading
# $XDG_CONFIG_HOME/alt/config.toml. Activation copies the app to ~/Applications and signs
# it with rcodesign under an identity derived from `seed`: every build meets one designated
# requirement, so its Accessibility permission survives rebuilds. With `glove80.enable`, the
# keyboard's firmware is at $XDG_DATA_HOME/alt/glove80.uf2 for `alt setup` to
# flash. The app logs to the unified log. `self` is the alt flake.
self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  service = config.services.alt;
  bundle = "${config.home.homeDirectory}/Applications/Alt.app";
  signed = "${config.xdg.stateHome}/alt/signed";
  agent = config.launchd.agents.alt.config.Label;
in
{
  options.services.alt = {
    enable = lib.mkEnableOption "keyboard, pointer, window and display behaviour outside the terminal";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      defaultText = lib.literalMD "this flake's package";
      description = "The alt package.";
    };

    glove80 = {
      enable = lib.mkEnableOption "the Glove80's firmware, which `alt setup` flashes";

      package = lib.mkOption {
        type = lib.types.package;
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.glove80;
        defaultText = lib.literalMD "this flake's `glove80` package";
        description = "The Glove80 firmware, whose output holds glove80.uf2.";
      };
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
    assertions = [
      (lib.hm.assertions.assertPlatform "services.alt" pkgs lib.platforms.darwin)
    ];

    home.packages = [ service.package ];

    # Copies and signs the app when the package changes, then restarts the agent onto it.
    home.activation.alt = lib.hm.dag.entryAfter [ "setupLaunchAgents" ] ''
      app=${service.package}/Applications/Alt.app
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

    xdg.dataFile."alt/glove80.uf2" = lib.mkIf service.glove80.enable {
      source = "${service.glove80.package}/glove80.uf2";
    };

    # The agent runs the app's executable, outside the store, so System Settings names it after
    # the app.
    launchd.agents.alt = {
      enable = true;
      waitForNixStore = false;
      config = {
        ProgramArguments = [
          "${bundle}/Contents/MacOS/alt"
          "run"
        ];
        AssociatedBundleIdentifiers = service.package.identifier;
        EnvironmentVariables.PATH = lib.makeBinPath service.path + ":/usr/bin:/bin";
        RunAtLoad = true;
        KeepAlive = true;
        ProcessType = "Interactive";
      };
    };
  };
}
