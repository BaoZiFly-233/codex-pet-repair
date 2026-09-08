/* Native mouse events feed Codex's own gaze renderer. No global cursor polling. */
(async () => {
    "use strict";
    const key = "__petRepairGazeV1", version = 3;
    const event = "avatar-overlay-computer-use-cursor-changed";
    const selector = '[data-avatar-mascot="true"][data-testid="avatar-mascot-button"]';
    const route = () => {
        const url = new URL(location.href), routes = url.searchParams.getAll("initialRoute");
        return ["app:", "file:"].includes(url.protocol) &&
            (routes.length ? routes.length === 1 && routes[0] === "/avatar-overlay" : url.pathname === "/avatar-overlay");
    };
    if (!route()) return { code: "wrong_route" };
    if (window[key]) return { code: window[key].version === version ? "ready" : "bridge_conflict" };
    const mascots = () => [...document.querySelectorAll(selector)];
    if (mascots().length !== 1) return { code: "mascot_not_unique" };
    const urls = new Set([
        ...[...document.querySelectorAll('script[src],link[rel="modulepreload"][href]')].map(e => e.src || e.href),
        ...performance.getEntriesByType("resource").map(e => e.name)
    ].filter(value => {
        try {
            const u = new URL(value, location.href), here = new URL(location.href);
            return u.protocol === here.protocol && u.host === here.host &&
                /\/(?:app-initial|vscode-api)-[\w-]+\.js$/.test(u.pathname);
        } catch { return false; }
    }));
    if (!urls.size || urls.size > 4) return { code: "dispatcher_module_missing" };
    const dispatchers = new Set();
    for (const url of urls) {
        const module = await import(url);
        for (const value of Object.values(module)) {
            if (value && typeof value === "object" && typeof value.dispatchHostMessage === "function" &&
                typeof value.subscribe === "function") dispatchers.add(value);
        }
    }
    if (dispatchers.size !== 1) return { code: "dispatcher_not_unique" };
    const bus = [...dispatchers][0];
    let source = null, sourceElement = null, isV2 = false, decoded = false, retryAt = 0, decodedImage = null, pixelCanvas = null;
    function sprite() {
        const roots = mascots();
        if (roots.length !== 1) return null;
        const root = roots[0], sources = [];
        for (const element of [root, ...root.querySelectorAll("img,[style]")]) {
            const match = /^url\(["']?(.+?)["']?\)$/.exec(getComputedStyle(element).backgroundImage);
            const url = element.tagName === "IMG" ? element.currentSrc || element.src : match?.[1];
            if (url) sources.push({ url, element });
        }
        return sources.length === 1 ? { ...sources[0], root } : null;
    }
    async function validateSprite() {
        const current = sprite();
        if (!current) { source = sourceElement = null; isV2 = false; return false; }
        if (current.url === source && current.element === sourceElement && (decoded || performance.now() < retryAt)) return isV2;
        source = current.url; sourceElement = current.element; isV2 = false; decoded = false; retryAt = performance.now() + 3000;
        const expected = source, element = sourceElement, image = new Image();
        image.src = expected;
        let timeout;
        try { await Promise.race([image.decode(), new Promise((_, reject) => { timeout = setTimeout(() => reject(new Error("decode_timeout")), 1000); })]); }
        catch { return false; }
        finally { clearTimeout(timeout); }
        const fresh = sprite();
        if (!fresh || fresh.url !== expected || fresh.element !== element || source !== expected) return false;
        decoded = true;
        isV2 = image.naturalWidth === 1536 && image.naturalHeight === 2288;
        decodedImage = isV2 ? image : null;
        return isV2;
    }
    function opaqueAt(x, y) {
        if (!decodedImage || !isV2 || !sourceElement) return false;
        for (let element = sourceElement; element; element = element.parentElement) {
            const appearance = getComputedStyle(element);
            if (Number(appearance.opacity ?? 1) < .99) return false;
            if (appearance.transform && appearance.transform !== "none") {
                const matrix = /^matrix\(([^)]+)\)$/.exec(appearance.transform)?.[1].split(",").map(Number);
                if (!matrix || matrix.length !== 6 || matrix.some(v => !Number.isFinite(v)) || matrix[0] <= 0 || matrix[3] <= 0 || Math.abs(matrix[1]) > .0001 || Math.abs(matrix[2]) > .0001) return false;
            }
        }
        const style = getComputedStyle(sourceElement), box = sourceElement.getBoundingClientRect();
        // Only the verified V2 background atlas layout is supported for native hit diagnostics.
        if (style.backgroundSize !== "800% 1100%" || box.width <= 0 || box.height <= 0) return false;
        const position = /^([\d.]+)% ([\d.]+)%$/.exec(style.backgroundPosition);
        if (!position) return false;
        const col = Number(position[1]) * 7 / 100, row = Number(position[2]) * 10 / 100;
        if (col < 0 || col > 7 || row < 0 || row > 10 || Math.abs(col - Math.round(col)) > .001 || Math.abs(row - Math.round(row)) > .001) return false;
        const localX = (x - box.left) / box.width, localY = (y - box.top) / box.height;
        if (localX < 0 || localX >= 1 || localY < 0 || localY >= 1) return false;
        try {
            if (!pixelCanvas) { pixelCanvas = document.createElement("canvas"); pixelCanvas.width = pixelCanvas.height = 1; }
            const ctx = pixelCanvas.getContext("2d", { willReadFrequently: true });
            if (!ctx) return false;
            ctx.clearRect(0, 0, 1, 1);
            ctx.drawImage(decodedImage, Math.round(col) * 192 + Math.floor(localX * 192), Math.round(row) * 208 + Math.floor(localY * 208), 1, 1, 0, 0, 1, 1);
            return ctx.getImageData(0, 0, 1, 1).data[3] >= 240;
        } catch { return false; }
    }
    if (!await validateSprite()) return { code: "sprite_v2_required" };

    let stopped = false, paused = true, gazeEnabled = true, active = false, foreign = false, sending = false;
    let heartbeat = performance.now(), sequence = 0, pending = null, painting = false;
    let raf = 0, delay = 0, expiry = 0, lastPaint = -Infinity, lastPoint = null;
    let code = "ready", geometry = "", moves = 0, resizes = 0;
    const held = new Set();
    const send = point => {
        sending = true;
        try { bus.dispatchHostMessage({ type: event, point }); active = point !== null; }
        finally { sending = false; }
    };
    const clear = () => { if (active && !foreign) send(null); active = false; };
    const reset = () => {
        sequence++; pending = lastPoint = null;
        cancelAnimationFrame(raf); clearTimeout(delay); clearTimeout(expiry);
        raf = delay = expiry = 0; clear();
    };
    let unsubscribe;
    try {
        unsubscribe = bus.subscribe(event, message => {
            if (sending) return;
            foreign = message.point != null; active = false; reset();
        });
    } catch { return { code: "subscription_failed" }; }
    if (typeof unsubscribe !== "function") return { code: "subscription_contract_changed" };
    function schedule() {
        if (raf || delay || painting || !pending || stopped) return;
        const remaining = 33 - (performance.now() - lastPaint);
        if (remaining > 0) { delay = setTimeout(() => { delay = 0; schedule(); }, remaining); return; }
        raf = requestAnimationFrame(paint);
    }
    async function paint() {
        raf = 0; painting = true;
        const point = pending, token = sequence; pending = null;
        try {
            if (!point || stopped || paused || foreign || held.size || document.visibilityState !== "visible") return;
            if (!route()) { stop(); return; }
            if (!await validateSprite()) { clear(); code = "sprite_v2_required"; return; }
            if (stopped || paused || foreign || token !== sequence) return;
            const current = sprite(), box = current?.root.getBoundingClientRect();
            if (!box || box.width <= 0 || box.height <= 0 || current.root.getAttribute("aria-hidden") === "true") { reset(); return; }
            const { x, y } = point;
            if ((x >= box.left && x <= box.right && y >= box.top && y <= box.bottom) ||
                Math.hypot(x - box.left - box.width / 2, y - box.top - box.height / 2) > 480) { reset(); return; }
            code = "ready"; lastPaint = performance.now();
            // Repeated host refreshes at the same point must not suppress idle indefinitely.
            if (lastPoint && Math.hypot(x - lastPoint.x, y - lastPoint.y) < 2) return;
            lastPoint = { x, y }; send(lastPoint);
            clearTimeout(expiry); expiry = setTimeout(() => { expiry = 0; clear(); }, 1400);
        } catch { code = "dispatch_failed"; reset(); }
        finally { painting = false; schedule(); }
    }
    const move = e => {
        if (!e.isTrusted || stopped || paused || foreign || document.visibilityState !== "visible") return;
        if (e.buttons || held.size) { reset(); return; }
        if (!Number.isFinite(e.clientX) || !Number.isFinite(e.clientY)) return;
        if (!gazeEnabled) return;
        moves++; sequence++; pending = { x: e.clientX, y: e.clientY }; schedule();
    };
    const resize = () => { resizes++; reset(); };
    const down = e => { held.add(e.pointerId); reset(); };
    const up = e => { held.delete(e.pointerId); };
    const blur = () => { held.clear(); reset(); };
    const listeners = [["mousemove", move], ["resize", resize], ["mouseleave", reset],
        ["pointerdown", down], ["gotpointercapture", down], ["pointerup", up],
        ["pointercancel", up], ["lostpointercapture", up], ["blur", blur]];
    for (const [name, fn] of listeners) window.addEventListener(name, fn, true);
    function stop() {
        if (stopped) return;
        stopped = true;
        try { reset(); } finally {
            unsubscribe(); clearInterval(watchdog);
            for (const [name, fn] of listeners) window.removeEventListener(name, fn, true);
            if (window[key] === api) delete window[key];
        }
    }
    const api = {
        version, stop,
        async check(input) {
            if (stopped) return { code: "stopped" };
            heartbeat = performance.now();
            if (!route()) { stop(); return { code: "wrong_route" }; }
            const next = input?.paused !== false || document.visibilityState !== "visible";
            const enabled = input?.gaze !== false;
            if (enabled !== gazeEnabled) { reset(); gazeEnabled = enabled; }
            const currentGeometry = [input?.x, input?.y, input?.width, input?.height].join();
            if (next !== paused || currentGeometry !== geometry) { reset(); geometry = currentGeometry; }
            paused = next;
            if (paused) return { code: "paused", moves, resizes };
            const valid = await validateSprite();
            if (stopped) return { code: "stopped" };
            if (!valid) { reset(); return { code: "sprite_v2_required", moves, resizes }; }
            if (!Number.isFinite(devicePixelRatio) || devicePixelRatio <= 0 ||
                Math.abs(innerWidth * devicePixelRatio - input.width) > 3 ||
                Math.abs(innerHeight * devicePixelRatio - input.height) > 3) {
                paused = true; reset(); return { code: "viewport_mismatch", moves, resizes };
            }
            // The native host samples the real cursor. DOM hit testing supplies only
            // the intended region, never a synthetic mouse event or input policy write.
            let inputRegion = false;
            const cursor = input?.cursor;
            if (!held.size && !foreign && cursor && Number.isFinite(cursor.x) && Number.isFinite(cursor.y)) {
                const x = (cursor.x - input.x) / devicePixelRatio;
                const y = (cursor.y - input.y) / devicePixelRatio;
                const hit = document.elementFromPoint?.(x, y);
                inputRegion = mascots().length === 1 && hit?.closest?.(selector) === mascots()[0] && opaqueAt(x, y);
            }
            return { code, moves, resizes, inputRegion };
        }
    };
    const watchdog = setInterval(() => { if (performance.now() - heartbeat > 3000) stop(); }, 1000);
    window[key] = api;
    return { code: "ready" };
})()
