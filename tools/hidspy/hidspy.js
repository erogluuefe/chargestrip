// Bir programın belirli bir USB cihazıyla (VID) HID trafiğini yalnızca DİNLER; hiçbir şey göndermez.
// run.py tarafından yüklenir; __VID__ yer tutucusu 4 haneli küçük harf hex VID ile değiştirilir.
const VID = /vid___VID__/i;
const hid = Module.load('hid.dll');
const kb = Process.getModuleByName('KERNELBASE.dll');
const handles = new Map(); // dosya tanıtıcısı → arayüz adı (mi_03 vb.)
const pending = new Map(); // bekleyen OVERLAPPED okuma → tampon

function hex(p, n) {
  if (p.isNull() || n <= 0) return '';
  return Array.from(new Uint8Array(p.readByteArray(Math.min(n, 128))))
    .map(b => b.toString(16).padStart(2, '0')).join(' ');
}
const tag = h => handles.get(h.toString());

Interceptor.attach(kb.getExportByName('CreateFileW'), {
  onEnter(a) { this.path = a[0].isNull() ? null : a[0].readUtf16String(); },
  onLeave(r) {
    if (this.path && VID.test(this.path)) {
      const m = this.path.match(/mi_\d\d(&col\d\d)?/i);
      handles.set(r.toString(), m ? m[0] : this.path);
    }
  }
});
Interceptor.attach(kb.getExportByName('CloseHandle'), {
  onEnter(a) { handles.delete(a[0].toString()); }
});
Interceptor.attach(kb.getExportByName('WriteFile'), {
  onEnter(a) { const d = tag(a[0]); if (d) send({ t: 'WRITE', dev: d, data: hex(a[1], a[2].toInt32()) }); }
});
Interceptor.attach(kb.getExportByName('ReadFile'), {
  onEnter(a) { this.d = tag(a[0]); this.buf = a[1]; this.pn = a[3]; this.ovl = a[4]; },
  onLeave(r) {
    if (!this.d) return;
    if (r.toInt32() !== 0 && !this.pn.isNull()) send({ t: 'READ', dev: this.d, data: hex(this.buf, this.pn.readU32()) });
    else if (!this.ovl.isNull()) pending.set(this.ovl.toString(), { d: this.d, buf: this.buf });
  }
});
Interceptor.attach(kb.getExportByName('GetOverlappedResult'), {
  onEnter(a) { this.ovl = a[1]; this.pn = a[2]; },
  onLeave(r) {
    const p = pending.get(this.ovl.toString());
    if (p && r.toInt32() !== 0) {
      send({ t: 'READ', dev: p.d, data: hex(p.buf, this.pn.readU32()) });
      pending.delete(this.ovl.toString());
    }
  }
});
// Feature/output raporları ReadFile/WriteFile yerine bu çağrılarla da gidebilir.
for (const [name, after] of [['HidD_SetFeature', false], ['HidD_GetFeature', true],
                             ['HidD_SetOutputReport', false], ['HidD_GetInputReport', true]]) {
  Interceptor.attach(hid.getExportByName(name), {
    onEnter(a) {
      this.d = tag(a[0]); this.buf = a[1]; this.n = a[2].toInt32();
      if (this.d && !after) send({ t: name, dev: this.d, data: hex(this.buf, this.n) });
    },
    onLeave(r) { if (this.d && after && r.toInt32() !== 0) send({ t: name, dev: this.d, data: hex(this.buf, this.n) }); }
  });
}
