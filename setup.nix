# One-time setup of this machine for accessibility; each step skips itself once done.
# - A self-signed code-signing certificate in the login keychain, trusted for code signing; it signs
#   the app, so its permissions survive rebuilds. `sudo` asks for the password once.
{ openssl, writeShellApplication }:
writeShellApplication {
  name = "setup";
  runtimeInputs = [ openssl ];
  text = ''
    name=''${ACCESSIBILITY_CERTIFICATE:-something-in-reality}
    keychain=$HOME/Library/Keychains/login.keychain-db

    certificate() {
      if security find-identity -v -p codesigning "$keychain" | grep -qF "\"$name\""; then
        echo "certificate: $name exists"
        return
      fi
      dir=$(mktemp -d)
      trap 'rm -rf "$dir"' RETURN
      openssl req -x509 -newkey rsa:3072 -nodes -days 3650 -subj "/CN=$name" \
        -addext "keyUsage=critical,digitalSignature" \
        -addext "extendedKeyUsage=critical,codeSigning" \
        -keyout "$dir/key.pem" -out "$dir/certificate.pem"
      openssl pkcs12 -export -legacy -passout pass:import \
        -inkey "$dir/key.pem" -in "$dir/certificate.pem" -out "$dir/identity.p12"
      security import "$dir/identity.p12" -k "$keychain" -P import -T /usr/bin/codesign
      sudo security add-trusted-cert -d -r trustRoot -p codeSign -k "$keychain" "$dir/certificate.pem"
      echo "certificate: $name created"
    }

    certificate
  '';
}
