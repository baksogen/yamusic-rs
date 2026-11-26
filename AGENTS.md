# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Яндекс Музыка RS — неофициальный клиент Яндекс Музыки на Rust и Slint (Skia без GPU), без Electron. Десктоп: Windows (основная платформа), Linux и macOS; Android — отдельный крейт в `android/`. Комментарии, сообщения коммитов и тексты интерфейса — на русском.

## Команды

```bash
cargo run                                          # debug: лог дублируется в консоль
cargo build --release                              # первая сборка 5–10 мин (Skia + LTO)
cargo clippy --release --all-targets -- -D warnings   # так проверяет CI на каждой платформе
cargo test --release                               # все тесты
cargo test --release keychain_roundtrip            # один тест по имени
cargo test --release -- --ignored                  # сетевые/внешние тесты (Discord, стрим, сайт Яндекс ID)
```

- macOS: `tools/make_dmg.sh` собирает universal `.app` (aarch64 + x86_64 через `lipo`) и `.dmg` в `target/release/`. Подпись ad-hoc; Developer ID — через `SIGN_ID`. Кросс-проверка: `cargo clippy --release --target aarch64-apple-darwin -- -D warnings` (и `x86_64-apple-darwin`).
- Linux-пакеты из готового бинарника: `cargo deb --no-build`, `cargo generate-rpm`, `tools/appimage.sh`. Релизы собираются на Ubuntu 22.04 (glibc).
- Android: `tools/apk.sh` (Linux/CI, нужны `ANDROID_HOME`, `JAVA_HOME`) или `android/build.ps1`, без Gradle.
- Если Skia не скачивается: `SKIA_BINARIES_URL=file:///путь/к/skia.tar.gz cargo build --release`.
- Релиз: тег `vX.Y.Z` должен совпадать с `version` в `Cargo.toml`; файлы собирает `.github/workflows/build.yml`.
- **Перед пушем тега релиза ОБЯЗАТЕЛЬНО проверить:**
  1. Версия в `Cargo.toml` совпадает с тегом (без префикса `v`)
  2. Workflow `.github/workflows/build.yml` содержит jobs: `windows`, `linux`, `linux-arm64`, `macos` — и НЕ содержит `android`
  3. Job `release` зависит от них: `needs: [windows, linux, linux-arm64, macos]`, но в `if` требует успеха только `windows`, `linux`, `linux-arm64` и не-`failure` у `macos` (через `always()`). Если GitHub не выдал мак-машину (job `macos` cancelled) — релиз выходит без мака, потом «Re-run failed jobs» догружает файлы мака в тот же релиз
  4. Локально проходят: `cargo clippy --release --all-targets -- -D warnings` и `cargo test --release`
  5. macOS: `tools/make_dmg.sh` работает (проверяется локально на маке)

## Форк

Это форк `SpolzerMod/yamusic-rs` (remote `upstream`); наш репозиторий — `origin` (`baksogen/yamusic-rs`).

- **Нужны только Windows, Linux и macOS.** Android-сборка не нужна: задания `android` в `build.yml` нет и в `release.needs` его нет. Папку `android/` не трогаем и не удаляем — это код оригинала, удаление даст конфликты на каждом мерже.
- **Мерж с оригиналом:** изменения в `android/` принимаются как есть; если мерж вернул задание `android` в `build.yml` — убрать его снова. Версию в `Cargo.toml` после мержа выставить по правилу ниже (мерж затирает её версией оригинала).
- **Версии и теги.** Взяли версию оригинала `X.Y.Z`, она собирается без правок и своих изменений в форке нет — версия и тег `vX.Y.Z`, как у оригинала. Иначе (пришлось чинить сборку или поверх есть свои изменения кода) — `X.Y.Z+fork.N` и тег `vX.Y.Z+fork.N`; `N` растёт с каждым нашим релизом (fork.1, fork.2…) и начинается заново со следующей версией оригинала. Правки только CI и `.md` своими изменениями не считаются. Пока порт на macOS (`src/mac/`, `src/sys/macos.rs`, `tools/make_*.sh`) есть только у нас, релизы фактически всегда `+fork.N`. Номер, уже запушенный тегом на GitHub, не переиспользуется — даже если сборка упала, берём следующий.
- Обновлялка (`update.rs`, `parse_version`) считает `X.Y.Z+fork.N` новее `X.Y.Z`, а `fork.2` новее `fork.1`.

## Архитектура

- **Потоки.** `main.rs` занимает единственный экземпляр и запускает `gui::run`. UI-поток владеет Slint и состоянием `Ctx` (`gui.rs`, доступ через `with_ctx`). Аудиодвижок `player.rs` живёт в своём потоке: UI шлёт ему `Cmd`, читает `Shared`/`state()`, а поток будит UI через notifier; `gui::sync_player` раз в 250 мс переносит состояние в интерфейс. Данные страниц (`net.rs`) и обложки (`images.rs`) грузятся в фоне с кэшами по бюджету памяти. Из чужих потоков и колбэков ОС в Slint заходят только через `upgrade_in_event_loop` / `slint::invoke_from_event_loop`.
- **Контракт UI ↔ Rust** — глобал `App` в `ui/state.slint`: свойства и колбэки, которые `gui.rs` выставляет и обрабатывает. Платформенные отличия вёрстки — через флаги вроде `App.linux`, `App.mac`, `App.chrome-top`.
- **Платформенный слой.** `mod sys` подставляется через `#[cfg_attr(..., path = "sys/<os>.rs")]` в `main.rs`: `windows.rs`, `linux.rs`, `macos.rs` (у Android — `android/src/sys.rs`) отдают один и тот же набор функций (пути, диалоги, буфер обмена, автозапуск, один экземпляр, окно). Новая функция в `sys` нужна во всех вариантах, иначе сломается сборка другой ОС. Медиаклавиши и трей отдельно: `tray.rs` (Windows), `mpris.rs` (Linux), `src/mac/` (macOS).
- **macOS (`src/mac/`)** — прямые вызовы objc runtime без крейтов: `objc_msgSend` приводится к точной сигнатуре каждого вызова; структуры больше 16 байт на x86_64 возвращаются через `objc_msgSend_stret`. Dock и ⌘Q — методы, добавленные в делегат winit (`WinitApplicationDelegate`) в рантайме. Всё — из главного потока.
- **Токен** (`secret.rs`): DPAPI на Windows, Secret Service на Linux, Keychain на macOS; в `config.json` лежит шифротекст или ссылка вида `keyring:`/`keychain:`. Настройки и лог — `config.rs`.
- **Яндекс.** `api.rs` — запросы; `client.rs` — всё, что Яндекс может поменять (OAuth-клиент, ключи подписи, заголовки): ломается вход или подписи — правят здесь. Страницы собирает сервер (`landing.rs`: `/landing/skeleton/<id>`, каждый тип блока — компонент в `ui/landing.slint`, неизвестные типы пропускаются). «Моя волна» — сессии ротора с фидбеком в `player.rs`.
- **Ассеты не в репозитории.** Шрифты, спрайт иконок и словари берутся из `app.asar` установленного официального приложения или скачиваются с music.yandex.ru (`assets.rs`). Перевод (`i18n::tr`) использует русскую строку как ключ — новые строки интерфейса пишутся по-русски и совпадают с русским словарём оригинала.
- **Сгенерированные файлы — руками не править:** `ui/theme.slint` (`tools/gen_theme.py`) и `src/palette.rs` (`tools/gen_palette.py`) из CSS/JS распакованного `app.asar`. Размеры и цвета сняты с оригинала, новые экраны сверяют с ним попиксельно.
- **Android разделяет код с десктопом**: `android/src/lib.rs` подключает модули `../../src/*.rs` через `#[path]` (api, player, config, secret и др.). Правка общего модуля должна собираться и под `target_os = "android"`. У `android/` свои `Cargo.toml`/`Cargo.lock` и UI.
- **Обновления** (`update.rs`) смотрят релизы репозитория из `repository` в `Cargo.toml`; на Windows exe заменяется на лету (ищется asset ровно `yamusic-rs.exe`), на остальных системах открывается страница релиза.
