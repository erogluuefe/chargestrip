# Waizowl OGM Pro V2 — battery protocol

Source: Waizowl Driver Software 1.0.5.1 (`DeviceDriver.exe`), observed with `tools/hidspy` (Frida)
on 2026-11-18. Traffic was only listened to; nothing was sent to the device while recording.

## Device

- Receiver "OGM 8K", USB `1915:0728` (Nordic), interface 3, usage page `0xFF0A`, usage `0x01`.
- With the mouse on its charging cable, traffic still went through the same receiver.

## Frame

Every request and reply is report ID `0x00` + 64 bytes.

| byte | meaning |
|---|---|
| 0 | command |
| 1 | `0x00` |
| 2 | `0x81` in requests (`0xA5` for `0x44`); `0x8C` / `0x9A` / `0xA5` in replies |
| 3 | `0x01` (`0x00` for the `0x40` / `0x00` requests) |
| 4…62 | data |
| 63 | checksum: the 64 bytes sum to **`0xA1`** (mod 256) |

## The two requests Chargestrip uses

| request (without report ID) | reply |
|---|---|
| `01 00 81 01 00… 1E` — link state | `[4]` = `0x0A` connected, `0x02` mouse off / asleep |
| `41 00 81 01 00… DE` — battery | `[5]` = `0x01` charging cable plugged in; `[6]` = percent (0–100) |

Samples from the recording:

| state | frame start | checksum |
|---|---|---|
| connected | `01 00 8c 01 0a 15 19 28 07 06 01 f5 07` | `a9` |
| mouse switched off | `01 00 8c 01 02 15 19 28 07 06 01 f5 07` | `b1` |
| battery 63 % (Waizowl app showed 63) | `41 00 8c 01 00 00 3f 00 01 05 06` | `88` |
| cable plugged in, 68 % | `41 00 8c 01 00 01 44 00 01 05 06` | `82` |
| cable unplugged, 68 % | `41 00 8c 01 00 00 44 00 01 05 06` | `83` |

Notes:

- Byte `[8]` of the `0x41` reply cycled 1→2→3→4→0→1 with the CPI button: the DPI stage, unused.
- Plugging in the cable made the level jump from 62 to 68 (voltage-based reading, rises while charging).
- The receiver also sends these frames on its own when battery or link state changes. Frames whose
  command byte or checksum don't match are skipped while reading.

## Why it is read-only

- On start-up and on every reconnect (mouse switched on, cable plugged in) the Waizowl software sends
  the same sequence without any user action: `01`, `41`, `43`, `44`, `40`, `00`. All have an empty
  data field and every reply returns the current state.
- No request that writes a setting appears in the recording; no settings were changed while recording.
- Check: send the requests repeatedly, then listen to the Waizowl software again and confirm that the
  `43` / `44` replies (settings tables, e.g. DPI 400/900/1600/3200/6400) are unchanged.

## Other requests (unused)

- `43 00 81 01` → `a5` reply, looks like the button / settings table.
- `44 00 a5 01` → DPI stages (`0x0190` = 400 … `0x1900` = 6400) and other settings.
- `40 00 81 00`, `00 00 81 00` → `9a` reply, version / identity information.
