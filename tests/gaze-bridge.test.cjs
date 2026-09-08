// Run: node --experimental-vm-modules --test tests/gaze-bridge.test.cjs
const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');
const script = fs.readFileSync(path.join(__dirname, '../assets/gaze-bridge.js'), 'utf8');

function host(options = {}) {
    let now = 1000, count = options.count ?? 1, id = 0;
    const delivered = [], subscribers = new Set(), listeners = new Map(), timers = new Map(), frames = new Map();
    const image = { tagName: 'IMG', currentSrc: 'app://-/pet.webp', src: '' };
    const box = { left: 400, top: 400, right: 512, bottom: 521, width: 112, height: 121 };
    const root = { tagName: 'DIV', querySelectorAll: () => [image], getAttribute: () => null, getBoundingClientRect: () => box };
    image.getBoundingClientRect = () => box;
    const bus = {
        subscribe(name, callback) { assert.equal(name, 'avatar-overlay-computer-use-cursor-changed'); subscribers.add(callback); return () => subscribers.delete(callback); },
        dispatchHostMessage(message) { delivered.push(JSON.parse(JSON.stringify(message))); for (const fn of subscribers) fn(message); }
    };
    const timer = (fn, ms, repeat) => { const key = ++id; timers.set(key, { fn, at: now + ms, repeat }); return key; };
    const sandbox = {
        URL, console,
        location: { href: options.url || 'app://-/index.html?initialRoute=%2Favatar-overlay' },
        performance: { now: () => now, getEntriesByType: () => [{ name: 'app://-/assets/app-initial-abcdef.js' }] },
        document: { visibilityState: 'visible', querySelectorAll: selector => selector.includes('script[') ? [] : Array(count).fill(root),
            elementFromPoint: () => ({ closest: () => root }),
            createElement: () => ({ getContext: () => ({ clearRect() {}, drawImage() {}, getImageData: () => ({ data: [0, 0, 0, options.alpha ?? 255] }) }) }) },
        getComputedStyle: () => ({ backgroundImage: 'none', backgroundSize: '800% 1100%', backgroundPosition: '0% 0%', opacity: options.opacity ?? '1', transform: options.transform ?? 'none' }),
        Image: class { constructor() { this.naturalWidth = options.width ?? 1536; this.naturalHeight = options.height ?? 2288; } async decode() { if (options.decode) await options.decode(this); } },
        devicePixelRatio: options.ratio || 1, innerWidth: 1000, innerHeight: 1000,
        setTimeout: (fn, ms) => timer(fn, ms, 0), clearTimeout: key => timers.delete(key),
        setInterval: (fn, ms) => timer(fn, ms, ms), clearInterval: key => timers.delete(key),
        requestAnimationFrame: fn => { const key = ++id; frames.set(key, fn); return key; }, cancelAnimationFrame: key => frames.delete(key),
        addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name)
    };
    sandbox.window = sandbox;
    const context = vm.createContext(sandbox);
    const evaluate = () => new vm.Script(script, { importModuleDynamically: async () => {
        const exports = options.ambiguous ? { a: bus, b: { ...bus } } : { a: bus, alias: bus, unrelated: () => { throw Error('must not call'); } };
        const module = new vm.SyntheticModule(Object.keys(exports), function () { for (const [key, value] of Object.entries(exports)) this.setExport(key, value); }, { context });
        await module.link(() => {}); await module.evaluate(); return module;
    } }).runInContext(context);
    return {
        evaluate, sandbox, image, box, delivered, subscribers, listeners, timers, frames,
        get api() { return sandbox.__petRepairGazeV1; },
        advance(ms) { now += ms; for (const [key, task] of [...timers]) { if (task.at <= now && timers.has(key)) { if (task.repeat) task.at = now + task.repeat; else timers.delete(key); task.fn(); } } },
        async frame() { this.advance(40); const tasks = [...frames.values()]; frames.clear(); for (const fn of tasks) fn(now); await new Promise(setImmediate); },
        emit(name, fields = {}) { listeners.get(name)?.({ pointerId: 1, ...fields }); },
        move(x = 650, y = 460, extra = {}) { this.emit('mousemove', { clientX: x, clientY: y, isTrusted: true, buttons: 0, ...extra }); },
        count(value) { count = value; },
        foreign(point) { for (const fn of subscribers) fn({ point }); },
        check(extra = {}) { return this.api.check({ paused: false, x: 0, y: 0, width: 1000 * sandbox.devicePixelRatio, height: 1000 * sandbox.devicePixelRatio, ...extra }); },
        async start() { assert.equal((await evaluate()).code, 'ready'); assert.equal((await this.check()).code, 'ready'); }
    };
}
test('input diagnostics require an opaque current sprite pixel and do not enable gaze', async () => {
    const h = host(); await h.start();
    assert.equal((await h.check({ gaze: false, cursor: { x: 450, y: 460 } })).inputRegion, true);
    h.move(); await h.frame(); assert.equal(h.delivered.length, 0);
    assert.equal((await h.check({ gaze: false, cursor: { x: 900, y: 900 } })).inputRegion, false);
    h.emit('gotpointercapture');
    assert.equal((await h.check({ cursor: { x: 450, y: 460 } })).inputRegion, false);
    const transparent = host({ alpha: 0 }); await transparent.start();
    assert.equal((await transparent.check({ cursor: { x: 450, y: 460 } })).inputRegion, false);
    for (const options of [{ opacity: '0' }, { transform: 'matrix(0, 1, -1, 0, 0, 0)' }]) {
        const hidden = host(options); await hidden.start();
        assert.equal((await hidden.check({ cursor: { x: 450, y: 460 } })).inputRegion, false);
    }
});
test('unique dispatcher installs once, starts paused and coalesces native movement', async () => {
    const h = host(); await h.evaluate(); h.move(); await h.frame(); assert.equal(h.delivered.length, 0);
    await h.check(); h.move(600); h.move(650); h.move(700); await h.frame();
    assert.deepEqual(h.delivered, [{ type: 'avatar-overlay-computer-use-cursor-changed', point: { x: 700, y: 460 } }]);
    assert.equal((await h.evaluate()).code, 'ready'); assert.equal(h.subscribers.size, 1);
});
test('wrong route, duplicate mascots, V1 and ambiguous dispatcher fail closed', async () => {
    for (const [options, code] of [
        [{ url: 'https://example.org/avatar-overlay' }, 'wrong_route'],
        [{ count: 2 }, 'mascot_not_unique'], [{ count: 0 }, 'mascot_not_unique'],
        [{ height: 1872 }, 'sprite_v2_required'], [{ width: 0 }, 'sprite_v2_required'],
        [{ ambiguous: true }, 'dispatcher_not_unique']
    ]) { const h = host(options); assert.equal((await h.evaluate()).code, code); assert.equal(h.api, undefined); assert.equal(h.delivered.length, 0); assert.equal(h.subscribers.size, 0); }
});
test('untrusted DOM events and invalid coordinates never become gaze input', async () => {
    const h = host(); await h.start(); h.move(650, 460, { isTrusted: false }); h.move(NaN); h.move(650, Infinity); await h.frame(); assert.equal(h.delivered.length, 0);
});
test('all sixteen directions use CSS client coordinates without applying DPI twice', async () => {
    for (const ratio of [1, 1.25, 1.5, 2]) {
        const h = host({ ratio }); await h.start();
        for (let n = 0; n < 16; n++) {
            const angle = n * Math.PI / 8, x = 456 + Math.sin(angle) * 475, y = 460.5 - Math.cos(angle) * 475;
            h.move(x, y); await h.frame(); assert.deepEqual(h.delivered.at(-1).point, { x, y });
        }
        assert.equal((await h.check({ width: 500 })).code, 'viewport_mismatch'); assert.equal(h.delivered.at(-1).point, null);
        h.api.stop();
    }
});
test('hover, window leave and distant cursor release the bridge point', async () => {
    const h = host(); await h.start(); h.move(); await h.frame(); h.move(450); await h.frame(); assert.equal(h.delivered.at(-1).point, null);
    h.move(); await h.frame(); h.emit('mouseleave'); assert.equal(h.delivered.at(-1).point, null);
    h.move(); await h.frame(); h.move(1100); await h.frame(); assert.equal(h.delivered.at(-1).point, null);
});
test('stationary refresh events do not postpone idle and do not schedule an idle frame loop', async () => {
    const h = host(); await h.start(); h.move(); await h.frame();
    for (let n = 0; n < 4; n++) { h.advance(320); h.move(); await h.frame(); await h.check(); }
    assert.equal(h.delivered.at(-1).point, null); assert.equal(h.frames.size, 0);
    const count = h.delivered.length; h.move(); await h.frame(); assert.equal(h.delivered.length, count);
    h.move(660); await h.frame(); assert.deepEqual(h.delivered.at(-1).point, { x: 660, y: 460 });
});
test('resize cancels stale queued input, keeps one listener and accepts fresh coordinates', async () => {
    const h = host(); await h.start(); h.move(); await h.frame(); h.move(670); h.emit('resize'); await h.frame();
    assert.equal(h.delivered.at(-1).point, null); const count = h.delivered.length;
    for (let n = 0; n < 20; n++) h.emit('resize');
    await h.frame(); assert.equal(h.delivered.length, count); assert.equal(h.listeners.size, 9);
    h.move(300, 460); await h.frame(); assert.deepEqual(h.delivered.at(-1).point, { x: 300, y: 460 });
    assert.equal((await h.check()).resizes, 21);
    await h.check({ x: -1200 }); assert.equal(h.delivered.at(-1).point, null);
    h.move(700); await h.frame(); assert.deepEqual(h.delivered.at(-1).point, { x: 700, y: 460 });
});
test('repair pause cancels queued frames and resumes only on a fresh mouse event', async () => {
    const h = host(); await h.start(); h.move(); await h.frame(); h.move(700); await h.check({ paused: true }); await h.frame();
    assert.equal(h.delivered.at(-1).point, null); const count = h.delivered.length;
    h.move(); await h.frame(); await h.check(); await h.frame(); assert.equal(h.delivered.length, count);
    h.move(); await h.frame(); assert.deepEqual(h.delivered.at(-1).point, { x: 650, y: 460 });
});
test('host cursor preempts native events and is never cleared by bridge cleanup', async () => {
    const h = host(); await h.start(); h.move(); await h.frame(); const count = h.delivered.length;
    h.foreign({ x: 1, y: 2 }); h.move(700); await h.frame(); assert.equal(h.delivered.length, count);
    h.foreign(null); await h.frame(); assert.equal(h.delivered.length, count);
    h.move(700); await h.frame(); assert.equal(h.delivered.length, count + 1);
    h.foreign({ x: 1, y: 2 }); h.api.stop(); assert.equal(h.delivered.length, count + 1);
});
test('capture, other-pointer release, cancel, buttons and blur respect native interaction', async () => {
    const h = host(); await h.start(); h.move(); await h.frame(); h.emit('gotpointercapture', { pointerId: 5 });
    assert.equal(h.delivered.at(-1).point, null); const count = h.delivered.length;
    h.emit('pointerup', { pointerId: 6 }); h.move(); await h.frame(); assert.equal(h.delivered.length, count);
    h.emit('pointercancel', { pointerId: 5 }); h.move(); await h.frame(); assert.equal(h.delivered.length, count + 1);
    h.move(670, 460, { buttons: 1 }); assert.equal(h.delivered.at(-1).point, null);
    h.emit('pointerdown'); h.emit('blur'); h.move(); await h.frame(); assert.deepEqual(h.delivered.at(-1).point, { x: 650, y: 460 });
});
test('hidden page and mascot replacement stop input without retaining stale compatibility', async () => {
    const h = host(); await h.start(); h.move(); await h.frame(); h.sandbox.document.visibilityState = 'hidden'; await h.check(); assert.equal(h.delivered.at(-1).point, null);
    h.sandbox.document.visibilityState = 'visible'; await h.check(); h.move(); await h.frame(); h.count(2); assert.equal((await h.check()).code, 'sprite_v2_required'); assert.equal(h.delivered.at(-1).point, null);
});
test('sprite replacement during decode cannot grant stale V2 compatibility', async () => {
    let h; h = host({ decode: () => { h.image.currentSrc = 'app://-/different.webp'; } });
    assert.equal((await h.evaluate()).code, 'sprite_v2_required'); assert.equal(h.api, undefined);
});
test('resize during asynchronous decode cannot dispatch an outdated point', async () => {
    let release, pending = false;
    const h = host({ decode: async () => { if (pending) await new Promise(r => release = r); } });
    await h.start(); h.move(); await h.frame(); pending = true; h.image.currentSrc = 'app://-/new.webp'; h.move(700); await h.frame();
    assert.ok(release); h.emit('resize'); release(); await new Promise(setImmediate); assert.equal(h.delivered.at(-1).point, null);
    pending = false; h.move(300); await h.frame(); assert.deepEqual(h.delivered.at(-1).point, { x: 300, y: 460 });
});
test('temporary image decode failure retries after backoff', async () => {
    let decodes = 0;
    const h = host({ decode: () => { if (++decodes === 2) throw Error('temporary'); } });
    await h.start(); h.image.currentSrc = 'app://-/new.webp'; assert.equal((await h.check()).code, 'sprite_v2_required');
    h.advance(1600); await h.check(); assert.equal(decodes, 2);
    h.advance(1500); assert.equal((await h.check()).code, 'ready'); assert.equal(decodes, 3);
});
test('stop, navigation and lost heartbeat remove timers, frames, listeners and subscriptions', async () => {
    for (const mode of ['stop', 'navigate', 'heartbeat']) {
        const h = host(); await h.start(); h.move(); await h.frame(); h.move(700);
        if (mode === 'stop') h.api.stop();
        else if (mode === 'navigate') { h.sandbox.location.href = 'app://-/chat'; await h.check(); }
        else h.advance(3100);
        assert.equal(h.api, undefined); assert.equal(h.delivered.at(-1).point, null);
        assert.equal(h.subscribers.size, 0); assert.equal(h.listeners.size, 0); assert.equal(h.timers.size, 0); assert.equal(h.frames.size, 0);
    }
});
