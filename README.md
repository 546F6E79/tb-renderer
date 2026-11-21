# tb-renderer

Кастомный рендерер Touch Bar для MacBook Pro 2016–2017 (T1/iBridge) поверх
[t1bridge](https://github.com/standardagents/t1bridge). Замещает встроенный ряд:
**Esc, яркость дисплея, громкость, живое управление MPRIS-плеером (⏮ ▶ ⏸ ⏭),
подсветка клавиатуры, Fn → F1–F12.** Медиа-кнопки, которые во встроенном
рендерере не работают на KDE (нужен desktop-provider), здесь оживлены напрямую
через `playerctl`/`wpctl`.

Rust, единственная зависимость — `libc`. Работает с любым композитором
(KDE/Wayland, Hyprland, что угодно) — протокол не зависит от десктопа.

## Возможности

- Полный ряд: `esc | яркость экрана −/+ | подсветка клавиатуры −/+ | трек | ⏮ ▶⏸ ⏭ | громкость −/+`
- Стиль Control Strip macOS: чёрный фон, графитовые кнопки, полумесяц/солнце для яркости и клавиатура с лучами
- Тонкие антиалиасированные векторные иконки плеера и громкости вместо Font Awesome
- Fn удержание → F1–F12 (кейкоды — работают в любой раскладке)
- Hover-подсветка сегмента + яркая отдача при нажатии
- Медиа через MPRIS: Brave/любой плеер; оптимистичное переключение play/pause без задержки
- **Бегущая строка "artist — title"** в центре ряда (полный ASCII + кириллица;
  ~66 px/с; предпочитает plasma-browser-integration — браузерный MPRIS отдаёт
  заголовок вкладки; пустой artist режется)
- Громкость и подсветка клавиатуры с автоповтором (удержание)
- Автопереподключение к сервису, graceful-fallback: при падении процесса t1-touchbar
  сам возвращается к встроенному рендереру

## Сборка и установка

```bash
cargo build --release          # libc подтянется, других зависимостей нет
mkdir -p ~/.config/t1bridge
ln -s "$(pwd)/target/release/tb-renderer" ~/.config/t1bridge/renderer
systemctl --user restart t1-touchbar
```

Откат к встроенному: `rm ~/.config/t1bridge/renderer && systemctl --user restart t1-touchbar`.

Требования: пользователь в группе `t1bridge`, установленный и запущенный t1bridge
(см. [macbook_pro_2017/docs/t1-recovery.md](../macbook_pro_2017/docs/t1-recovery.md)),
`playerctl` и `wpctl` в PATH.

## Атлас глифов

Иконки и текст — 8-битные альфа-битмапы, сгенерированные из системного шрифта
freetype'ом на этапе разработки (не в рантайме!). Перегенерация после смены
символов/кеглей:

```bash
clang -shared -fPIC -O2 -I/usr/include/freetype2 tools/raster.c -o tools/raster.so -lfreetype
python3 tools/gen_atlas.py
```

Грабли, на которые наступили (см. git log): unicode ⏸⏮⏭ в JetBrains Mono — tofu,
настоящие иконки лежат в PUA-диапазоне Nerd Font (fa-backward `U+F04A` и т.д.);
`F_ADD_SEALS` на x86_64 — **1033**, не 1031.

## Протокол renderer-selection v1 (клиентские заметки)

Единственная известная автору публичная выжимка клиентской стороны. Всё
проверено на t1bridge 0.1.12, MacBookPro14,3.

- Транспорт: `SOCK_SEQPACKET`, `/run/t1bridge/touchbar.sock` (постоянный путь,
  ничего не передаётся через env/argv). Один клиент одновременно; сервис
  проверяет SO_PEERCRED + группу `t1bridge` + активную seat0-сессию.
- Пакет: 16-байтовый заголовок `T1HW` + u16 major=1 + u16 type + u32 len + u32
  request_id (0 у событий), всё little-endian; длина payload — точно.
- `Hello(0x0001)`: minor=1, u64 features → `HelloAck(0x8002→0x8001)`: width,
  height (2170×60), pixel_format=1 (XRGB8888, в памяти B,G,R,X), max_buffers=3.
  Флаги: memfd 0x01, input 0x02, tapkeys 0x04, display-brightness 0x08,
  kbd-backlight 0x10; сервис всегда принимает 0x27; неподдерживаемый набор =
  Error 2 и разрыв → пробовать меньший (как built-in: 0x3f→0x2f→0x37→0x27).
- Буферы: `RegisterBuffer(0x0002)` + ровно один SCM_RIGHTS: memfd ровно
  stride×height, печати `F_SEAL_SHRINK|F_SEAL_GROW|F_SEAL_SEAL` (без SEAL_WRITE —
  рисуем через свой mapping). stride = width×4.
- Кадры: `SubmitFrame(0x0003)` (damage_count=0 = весь кадр) → Ack (владение у
  сервиса) → `FrameReleased(0x9001)` (можно рисовать снова). Первый кадр всегда
  показывается целиком. frame_id ≠ 0, request_id уникальны до ответа.
- Ввод: `InputFrame(0x9002)`: fn_pressed + контакты {id, tip, in_range, x, y};
  пустой кадр = все отпущены; смена Fn приходит и без касаний.
- Клавиши: `TapKeys(0x0004)` — только Esc=1, F1..F10=59..68, F11=87, F12=88.
  Медиа-кодов НЕТ (в этом и причина этого рендерера).
- Яркость: `StepDisplayBrightness(0x0008)/StepKeyboardBacklight(0x0009)` —
  1 байт направления, не чаще раза в 20 мс.

## Структура

```
src/wire.rs    — формат пакетов (encode/decode всех сообщений)
src/conn.rs    — SOCK_SEQPACKET через libc + poll
src/frame.rs   — memfd, печати, SCM_RIGHTS, RW-mapping
src/atlas.rs   — СГЕНЕРИРОВАННЫЕ глифы (freetype, см. tools/)
src/draw.rs    — холст XRGB8888: примитивы + альфа-смешивание глифов
src/layout.rs  — сегменты ряда, hit-test
src/media.rs   — playerctl-поллер (2 c) + wpctl-громкость
src/main.rs    — сессия: handshake, буферы, ввод (тап/hover/hold-repeat), отрисовка
tools/         — генератор атласа (C-шим над freetype + python)
```

## Лицензия

MIT
