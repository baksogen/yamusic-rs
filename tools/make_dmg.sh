#!/bin/bash
# Собирает universal .app (через tools/make_app.sh), затем упаковывает его в .dmg:
# том с приложением и симлинком /Applications, чтобы установка была перетаскиванием.
# Использование: tools/make_dmg.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# Собираем universal .app и упаковываем его в .dmg.
MACOS_UNIVERSAL=1 tools/make_app.sh

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
# Имя берём из того же места, что и make_app.sh, — из build.rs.
NAME=$(sed -n 's/^const APP_NAME: &str = "\(.*\)";/\1/p' build.rs)
APP="target/release/$NAME.app"
DMG="target/release/yamusic-rs-$VERSION.dmg"
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

# Печать бандла проверяем до упаковки: иначе Gatekeeper скажет «damaged» уже у пользователя.
if ! codesign --verify --strict "$APP" 2>/dev/null; then
  echo "Подпись $APP недействительна — пересоберите его: tools/make_app.sh" >&2
  exit 1
fi

cp -R "$APP" "$STAGE/"
# Симлинк, чтобы приложение перетаскивалось в «Программы» прямо из окна тома.
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname "$NAME" -srcfolder "$STAGE" \
  -fs HFS+ -format UDZO -ov -quiet "$DMG"
echo "Готово: $DMG"
