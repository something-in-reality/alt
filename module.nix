# nix-darwin module: the alt app as a launchd agent at login, reading
# $XDG_CONFIG_HOME/alt/config.toml. Activation copies the app to the package's `bundle` path and
# signs it with rcodesign under an identity derived from `seed`: every build meets one designated
# requirement, so its Accessibility permission survives rebuilds. With `glove80.enable`, the
# keyboard's firmware is at share/alt/glove80.uf2 in the system profile for `alt setup` to
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
  inherit (service.package) bundle;
  agent = config.launchd.user.agents.alt.serviceConfig.Label;
  user = lib.escapeShellArg config.system.primaryUser;
in
{
  options.services.alt = {
    enable = lib.mkEnableOption "keyboard, pointer, window and display behaviour outside the terminal";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      defaultText = lib.literalMD "this flake's package";
      description = "The alt package, whose `app` output holds the app bundle.";
    };

    glove80 = {
      enable = lib.mkEnableOption "the Glove80's firmware, which `alt setup` flashes";

      package = lib.mkOption {
        type = lib.types.package;
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.glove80;
        defaultText = lib.literalMD "this flake's `glove80` package";
        description = "The Glove80 firmware, whose output holds share/alt/glove80.uf2.";
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

  # nix-darwin's launchd.user.agents.<name>.serviceConfig declares no AssociatedBundleIdentifiers.
  options.launchd.user.agents = lib.mkOption {
    type = lib.types.attrsOf (
      lib.types.submodule {
        options.serviceConfig = lib.mkOption {
          type = lib.types.submodule {
            options.AssociatedBundleIdentifiers = lib.mkOption {
              type = lib.types.nullOr (lib.types.either lib.types.str (lib.types.listOf lib.types.str));
              default = null;
              description = "Bundle identifiers whose apps System Settings names for the job.";
            };
          };
        };
      }
    );
  };

  config = lib.mkIf service.enable {
    environment.systemPackages = [
      service.package
    ]
    ++ lib.optional service.glove80.enable service.glove80.package;
    # Links the firmware into /run/current-system/sw/share, on XDG_DATA_DIRS.
    environment.pathsToLink = [ "/share/alt" ];

    # Runs as root, last. Trusts the app's signing certificate for code signing, then copies and
    # signs the app unless the installed executable is this build, by its Mach-O UUID, and validly
    # signed; the agent restarts onto a changed certificate or app.
    system.activationScripts.postActivation.text = ''
      alt_restart=
      alt_certificate=$(mktemp)
      ${service.package}/bin/signing-identity ${lib.escapeShellArg service.seed} \
        | /usr/bin/sed '/BEGIN PRIVATE KEY/,$d' > "$alt_certificate"
      if ! /usr/bin/security verify-cert -q -p codeSign -c "$alt_certificate" >/dev/null 2>&1; then
        /usr/bin/security add-trusted-cert -d -r trustRoot -p codeSign "$alt_certificate"
        /usr/bin/tccutil reset Accessibility ${service.package.identifier}
        alt_restart=1
      fi
      rm "$alt_certificate"

      alt_app=${service.package.app}/Applications/${baseNameOf bundle}
      alt_uuid() {
        ${lib.getExe' pkgs.cctools "otool"} -l "$1/Contents/MacOS/alt" 2>/dev/null \
          | ${lib.getExe pkgs.gawk} '$1 == "uuid" { print $2; exit }'
      }
      if [ "$(alt_uuid ${bundle})" != "$(alt_uuid "$alt_app")" ] || ! /usr/bin/codesign --verify ${bundle} 2>/dev/null; then
        rm -rf ${bundle}
        cp -R "$alt_app" ${bundle}
        chmod -R u+w ${bundle}
        ${service.package}/bin/signing-identity ${lib.escapeShellArg service.seed} \
          | ${lib.getExe pkgs.rcodesign} sign --timestamp-url none --pem-file /dev/stdin ${bundle}
        alt_restart=1
      fi

      if [ -n "$alt_restart" ]; then
        /bin/launchctl kickstart -k "gui/$(/usr/bin/id -u ${user})/${agent}" || true
      fi
    '';

    # The agent runs the app's executable, outside the store, so System Settings names it after
    # the app.
    launchd.user.agents.alt.serviceConfig = {
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
}
