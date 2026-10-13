# Creates a self-signed code-signing certificate in the login keychain and trusts it for code
# signing; `sudo` asks for the password once. An existing certificate is kept.
{ openssl, writeShellApplication }:
writeShellApplication {
  name = "certificate";
  runtimeInputs = [ openssl ];
  text = ''
    name=''${1:-something-in-reality}
    keychain=$HOME/Library/Keychains/login.keychain-db
    if security find-identity -v -p codesigning "$keychain" | grep -qF "\"$name\""; then
      echo "$name is already a code-signing identity"
      exit
    fi
    dir=$(mktemp -d)
    trap 'rm -rf "$dir"' EXIT
    openssl req -x509 -newkey rsa:3072 -nodes -days 3650 -subj "/CN=$name" \
      -addext "keyUsage=critical,digitalSignature" -addext "extendedKeyUsage=critical,codeSigning" \
      -keyout "$dir/key.pem" -out "$dir/certificate.pem"
    openssl pkcs12 -export -legacy -passout pass:import \
      -inkey "$dir/key.pem" -in "$dir/certificate.pem" -out "$dir/identity.p12"
    security import "$dir/identity.p12" -k "$keychain" -P import -T /usr/bin/codesign
    sudo security add-trusted-cert -d -r trustRoot -p codeSign -k "$keychain" "$dir/certificate.pem"
    security find-identity -v -p codesigning "$keychain" | grep -F "\"$name\""
  '';
}
