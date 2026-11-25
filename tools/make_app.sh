#!/bin/bash
# Собирает .app-бандл для macOS: бинарник, Info.plist и иконка из packaging/icons/256.png.
# Источник иконки — кадр 256×256, крупные размеры растягиваются из него.
# Использование: tools/make_app.sh (сборка всегда release).
set -euo pipefail
cd "$(dirname "$0")/.."

# Сборка бинарника. Три режима:
#  - MACOS_UNIVERSAL=1 — обе арки (arm64 + x86_64) и lipo в universal (CI: раннер macOS);
#  - MACOS_TARGET=<triple> — одна кросс-арка;
#  - иначе — нативно под хост (локальная сборка на Apple Silicon).
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
# Имя задаётся один раз в build.rs (APP_NAME): здесь только читаем, чтобы не дублировать.
NAME=$(sed -n 's/^const APP_NAME: &str = "\(.*\)";/\1/p' build.rs)
APP="target/release/$NAME.app"

if [ "${MACOS_UNIVERSAL:-}" = "1" ]; then
  # Кросс-сборка x86_64 нужна только здесь: без std под цель rustc падает на
  # «can't find crate for core», поэтому проверяем заранее и подсказываем лечение.
  LIBDIR=$(rustc --print target-libdir --target x86_64-apple-darwin 2>/dev/null || true)
  if [ -z "$LIBDIR" ] || [ ! -d "$LIBDIR" ]; then
    echo "x86_64-apple-darwin std не найдена, пробуем доустановить..." >&2
    rustup target add x86_64-apple-darwin || {
      echo "Не удалось установить x86_64-apple-darwin. Если rustc из Homebrew, поставьте rustup-тулчейн первым в PATH." >&2
      exit 1
    }
  fi
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  BIN="target/release/yamusic-rs-universal"
  lipo -create \
    "target/aarch64-apple-darwin/release/yamusic-rs" \
    "target/x86_64-apple-darwin/release/yamusic-rs" \
    -output "$BIN"
elif [ -n "${MACOS_TARGET:-}" ]; then
  cargo build --release --target "$MACOS_TARGET"
  BIN="target/$MACOS_TARGET/release/yamusic-rs"
else
  cargo build --release
  BIN="target/release/yamusic-rs"
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/yamusic-rs"

# Иконка: packaging/icons/256.png → все размеры → icns.
ICONSET="$APP/Contents/Resources/AppIcon.iconset"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP" "$ICONSET"' EXIT
mkdir -p "$ICONSET"
cp packaging/icons/256.png "$TMP/base.png"
for n in 16 32 64 128 256 512 1024; do
  sips -z "$n" "$n" "$TMP/base.png" --out "$TMP/$n.png" >/dev/null
done
cp "$TMP/16.png"   "$ICONSET/icon_16x16.png"
cp "$TMP/32.png"   "$ICONSET/icon_16x16@2x.png"
cp "$TMP/32.png"   "$ICONSET/icon_32x32.png"
cp "$TMP/64.png"   "$ICONSET/icon_32x32@2x.png"
cp "$TMP/128.png"  "$ICONSET/icon_128x128.png"
cp "$TMP/256.png"  "$ICONSET/icon_128x128@2x.png"
cp "$TMP/256.png"  "$ICONSET/icon_256x256.png"
cp "$TMP/512.png"  "$ICONSET/icon_256x256@2x.png"
cp "$TMP/512.png"  "$ICONSET/icon_512x512.png"
cp "$TMP/1024.png" "$ICONSET/icon_512x512@2x.png"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
# Набор PNG — временный: удаляем до подписи, иначе codesign запечатает эти файлы,
# trap уберёт их после подписи, и печать бандла станет недействительной.
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key><string>$NAME</string>
	<key>CFBundleDisplayName</key><string>$NAME</string>
	<key>CFBundleExecutable</key><string>yamusic-rs</string>
	<key>CFBundleIdentifier</key><string>io.github.baksogen.yamusic-rs</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>$VERSION</string>
	<key>CFBundleVersion</key><string>$VERSION</string>
	<key>CFBundleIconFile</key><string>AppIcon</string>
	<key>LSMinimumSystemVersion</key><string>12.0</string>
	<key>NSHighResolutionCapable</key><true/>
	<key>LSApplicationCategoryType</key><string>public.app-category.music</string>
</dict>
</plist>
EOF

# Ad-hoc подпись: без неё передача бандла на другой мак упрётся в Gatekeeper.
# Подпись: ad-hoc по умолчанию; Developer ID — SIGN_ID="Developer ID Application: …".
SIGN_ID="${SIGN_ID:--}"
codesign --force --sign "$SIGN_ID" "$APP"

echo "Готово: $APP"
echo "Запуск: open \"$APP\""
