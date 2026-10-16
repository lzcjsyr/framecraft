#!/usr/bin/env bash
# Development uses ad hoc signing without accessing Keychain.
set -euo pipefail
APP_BUNDLE="${1:?Pass the Framecraft.app path}"
/usr/bin/codesign --force --sign "${FRAMECRAFT_SIGNING_IDENTITY:--}" \
  --identifier com.framecraft.studio "$APP_BUNDLE"
/usr/bin/codesign --verify --strict "$APP_BUNDLE"
