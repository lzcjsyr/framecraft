#!/usr/bin/env bash
# Stable local signing lets Keychain recognize Framecraft across rebuilds.
set -euo pipefail
APP_BUNDLE="${1:?Pass the Framecraft.app path}"
SIGNING_NAME="Framecraft Local Development"
if [[ -n "${FRAMECRAFT_SIGNING_IDENTITY:-}" ]]; then
  /usr/bin/codesign --force --sign "$FRAMECRAFT_SIGNING_IDENTITY" --identifier com.framecraft.studio "$APP_BUNDLE"
else
  LOGIN_KEYCHAIN="$(/usr/bin/security default-keychain -d user | tr -d '"' | xargs)"
  if ! /usr/bin/security find-certificate -c "$SIGNING_NAME" "$LOGIN_KEYCHAIN" >/dev/null 2>&1; then
    SIGNING_TEMP="$(mktemp -d)"
    chmod 700 "$SIGNING_TEMP"
    trap 'rm -rf "$SIGNING_TEMP"' EXIT
    cat > "$SIGNING_TEMP/certificate.cnf" <<'CONFIG'
[req]
distinguished_name = name
x509_extensions = signing
prompt = no
[name]
CN = Framecraft Local Development
[signing]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = codeSigning
CONFIG
    /usr/bin/openssl req -new -newkey rsa:2048 -nodes -x509 -days 3650 \
      -config "$SIGNING_TEMP/certificate.cnf" -keyout "$SIGNING_TEMP/private.pem" \
      -out "$SIGNING_TEMP/certificate.pem" >/dev/null 2>&1
    SIGNING_PASSWORD="$(/usr/bin/openssl rand -hex 24)"
    /usr/bin/openssl pkcs12 -export -inkey "$SIGNING_TEMP/private.pem" -in "$SIGNING_TEMP/certificate.pem" \
      -name "$SIGNING_NAME" -out "$SIGNING_TEMP/identity.p12" -passout "pass:$SIGNING_PASSWORD" -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 >/dev/null 2>&1
    /usr/bin/security import "$SIGNING_TEMP/identity.p12" -k "$LOGIN_KEYCHAIN" -P "$SIGNING_PASSWORD" -T /usr/bin/codesign >/dev/null
    # Trust only code signing, in the current user's login keychain.
    /usr/bin/security add-trusted-cert -r trustRoot -p codeSign -k "$LOGIN_KEYCHAIN" "$SIGNING_TEMP/certificate.pem"
  fi
  /usr/bin/codesign --force --sign "$SIGNING_NAME" --identifier com.framecraft.studio "$APP_BUNDLE"
fi
/usr/bin/codesign --verify --strict "$APP_BUNDLE"
