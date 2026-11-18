"""Üretici yazılımını başlatıp USB cihazıyla HID trafiğini kaydeder (yalnızca dinler).

Kullanım:  python run.py <program.exe> <vid> <kayit.log>
Örnek:     python run.py "C:\\Program Files (x86)\\Waizowl Driver Software\\DeviceDriver.exe" 1915 kayit.log
Gerekli:   pip install frida
Program kapatılınca kayıt biter.
"""

import pathlib
import sys
import threading
import time

import frida


def main() -> None:
    exe, vid, log_path = sys.argv[1], sys.argv[2].lower().zfill(4), sys.argv[3]
    source = (pathlib.Path(__file__).with_name("hidspy.js")).read_text(encoding="utf-8")
    source = source.replace("__VID__", vid)
    log = open(log_path, "a", encoding="utf-8", buffering=1)
    done = threading.Event()

    def on_message(msg, _data):
        stamp = time.strftime("%H:%M:%S")
        p = msg.get("payload", msg)
        log.write(f"{stamp} {p.get('t')} {p.get('dev')} {p.get('data')}\n" if isinstance(p, dict) else f"{stamp} {p}\n")

    device = frida.get_local_device()
    pid = device.spawn([exe], cwd=str(pathlib.Path(exe).parent))
    session = device.attach(pid)
    session.on("detached", lambda *_: done.set())
    script = session.create_script(source)
    script.on("message", on_message)
    script.load()
    device.resume(pid)
    print(f"kaydediliyor → {log_path} (programı kapatınca biter)")
    done.wait()


if __name__ == "__main__":
    main()
