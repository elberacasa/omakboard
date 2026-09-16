// Omakboard WebHID tap — records the AULA web driver's conversation with the keyboard.
//
// Research tooling, not project code. It is a passive observer: it wraps the WebHID
// methods, logs what passes through, and calls the original. It never sends a report
// of its own, so the keyboard only ever sees what the official driver decided to send.
//
// Use: paste into the web driver tab's console BEFORE connecting the keyboard, then
//   omakboard.mark('rapid trigger on, W key, 0.30mm')   // label the action you are about to do
//   ... change exactly one setting in the UI ...
//   omakboard.save()                                    // downloads the annotated JSON log
//
// Every entry keeps the raw bytes. Decoding comes later, from the diffs between marks.

(() => {
  if (globalThis.omakboard) { console.warn('[omakboard] tap already installed'); return; }

  const t0 = performance.now();
  const log = [];
  const hex = (view) => {
    const b = new Uint8Array(view.buffer ?? view, view.byteOffset ?? 0, view.byteLength ?? view.length);
    return Array.from(b, (x) => x.toString(16).padStart(2, '0')).join(' ');
  };
  const record = (dir, reportId, data, extra = {}) => {
    const entry = { t: +(performance.now() - t0).toFixed(1), dir, reportId, len: data ? (data.byteLength ?? data.length) : 0, hex: data ? hex(data) : '', ...extra };
    log.push(entry);
    console.log(`[omakboard] ${String(entry.t).padStart(9)}  ${dir.padEnd(6)} id=${reportId}  ${entry.hex}`);
    return entry;
  };

  const seen = new WeakSet();
  const watch = (device) => {
    if (!device || seen.has(device)) return device;
    seen.add(device);
    record('OPEN', '-', null, { product: device.productName, vendorId: device.vendorId, productId: device.productId,
      collections: device.collections?.map((c) => ({ usagePage: c.usagePage, usage: c.usage,
        in: c.inputReports?.map((r) => r.reportId), out: c.outputReports?.map((r) => r.reportId), feat: c.featureReports?.map((r) => r.reportId) })) });
    device.addEventListener('inputreport', (e) => record('IN', e.reportId, e.data));
    return device;
  };

  const HD = globalThis.HIDDevice?.prototype;
  if (HD) {
    const { sendReport, sendFeatureReport, receiveFeatureReport, open } = HD;
    HD.sendReport = function (id, data) { record('OUT', id, data); return sendReport.call(this, id, data); };
    HD.sendFeatureReport = function (id, data) { record('SETFT', id, data); return sendFeatureReport.call(this, id, data); };
    HD.receiveFeatureReport = function (id) {
      return receiveFeatureReport.call(this, id).then((d) => { record('GETFT', id, d); return d; });
    };
    HD.open = function () { return open.call(this).then((r) => { watch(this); return r; }); };
  }

  const H = globalThis.HID?.prototype ?? Object.getPrototypeOf(navigator.hid);
  if (H?.requestDevice) {
    const requestDevice = H.requestDevice;
    H.requestDevice = function (opts) {
      record('REQ', '-', null, { filters: opts?.filters });
      return requestDevice.call(this, opts).then((ds) => { ds.forEach(watch); return ds; });
    };
  }
  navigator.hid.getDevices().then((ds) => ds.forEach(watch)).catch(() => {});

  globalThis.omakboard = {
    log,
    mark: (label) => { record('MARK', '-', null, { label }); console.info(`[omakboard] ── ${label}`); },
    dump: () => JSON.stringify(log, null, 1),
    clear: () => { log.length = 0; },
    save: (name = `omakboard-capture-${Date.now()}.json`) => {
      const a = document.createElement('a');
      a.href = URL.createObjectURL(new Blob([JSON.stringify(log, null, 1)], { type: 'application/json' }));
      a.download = name; a.click();
      console.info(`[omakboard] saved ${log.length} entries to ${name}`);
    },
  };
  console.info('[omakboard] tap installed. omakboard.mark("what you are about to do") then change one setting.');
})();
