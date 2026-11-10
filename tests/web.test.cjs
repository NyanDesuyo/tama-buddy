// Exercise the buildless frontend without a browser or third-party packages.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const html = fs.readFileSync(path.join(__dirname, '../web/index.html'), 'utf8');
const script = fs.readFileSync(path.join(__dirname, '../web/app.js'), 'utf8');

class Element {
  constructor() {
    this.dataset = {};
    this.attributes = {};
    this.listeners = {};
    this.children = [];
    this.parentElement = { dataset: {} };
    this.textContent = '';
    this.value = '';
  }
  addEventListener(name, handler) { this.listeners[name] = handler; }
  setAttribute(name, value) { this.attributes[name] = value; }
  closest() { return this; }
  append(...elements) { this.children.push(...elements); }
  replaceChildren() { this.children = []; }
  focus() { this.focused = true; }
}
function data(pet = {}, message = null) {
  return { pet: { name: 'Mochi', born: 1000, last_tick: 1000, fullness: 80,
    happiness: 80, energy: 80, hygiene: 80, health: 100, sleeping: false,
    last_action: 0, ...pet }, events: [], message, server_time: 1010 };
}
const flush = () => new Promise(setImmediate);
async function fixture(initial) {
  const elements = new Map([...html.matchAll(/\bid="([^"]+)"/g)].map((m) => [m[1], new Element()]));
  const buttons = [...html.matchAll(/<button[^>]+data-action="([^"]+)"/g)].map((m) => {
    const element = new Element();
    element.dataset.action = m[1];
    return element;
  });
  const calls = [];
  const network = { handler: async () => initial };
  const timers = new Map();
  let timerId = 0;
  const context = vm.createContext({
    document: { hidden: false, getElementById: (id) => {
      assert.ok(elements.has(id), `Unknown HTML id: ${id}`);
      return elements.get(id);
    }, querySelectorAll: (selector) => selector === '[data-action]' ? buttons : [], createElement: () => new Element(), addEventListener() {} },
    window: { addEventListener() {} }, AbortSignal, Date, console,
    setInterval() {},
    setTimeout(handler) { timers.set(++timerId, handler); return timerId; },
    clearTimeout(id) { timers.delete(id); },
    fetch: async (url, options) => {
      calls.push({ url, options });
      const result = await network.handler(url, options);
      return { ok: true, json: async () => result };
    },
  });
  vm.runInContext(script, context);
  await flush();
  return { get: (id) => elements.get(id), button: (action) => buttons.find((b) => b.dataset.action === action),
    network, calls, timers, run: (code) => vm.runInContext(code, context) };
}

test('successful care animates each action and wake, then returns to idle', async () => {
  const ui = await fixture(data());
  for (const action of ['feed', 'play', 'clean', 'sleep', 'medicine']) {
    ui.network.handler = async () => data({ sleeping: action === 'sleep' });
    await ui.button(action).listeners.click();
    assert.equal(ui.get('habitat').dataset.action, action);
    assert.equal(ui.timers.size, 1, 'A new animation cancels the previous cleanup');
  }
  ui.network.handler = async () => data({ sleeping: false });
  await ui.button('sleep').listeners.click();
  assert.equal(ui.get('habitat').dataset.action, 'wake');
  [...ui.timers.values()][0]();
  assert.equal(ui.get('habitat').dataset.action, undefined);
});

test('failed care does not animate, and a dead pet clears active effects', async () => {
  const ui = await fixture(data());
  ui.network.handler = async () => { throw Object.assign(new Error('Please wait'), { status: 409 }); };
  await ui.button('feed').listeners.click();
  assert.equal(ui.get('habitat').dataset.action, undefined);
  ui.network.handler = async () => data();
  await ui.button('clean').listeners.click();
  assert.equal(ui.get('habitat').dataset.action, 'clean');
  ui.network.handler = async () => data({ health: 0 });
  await ui.run('poll()');
  assert.equal(ui.get('habitat').dataset.action, undefined);
  assert.equal(ui.timers.size, 0);
});

test('hatches from the nursery, validates names, and renders names as text', async () => {
  const ui = await fixture({ pet: null, events: [], message: null, server_time: 1010 });
  assert.equal(ui.get('hatch-form').hidden, false);
  assert.equal(ui.button('feed').disabled, true);
  ui.get('pet-name-input').value = 'a'.repeat(21);
  ui.get('hatch-form').listeners.submit({ preventDefault() {} });
  assert.match(ui.get('message').textContent, /20 characters/);
  assert.equal(ui.calls.length, 1);
  const name = '<img src=x>';
  ui.get('pet-name-input').value = name;
  ui.network.handler = async () => data({ name }, 'Hatched!');
  ui.get('hatch-form').listeners.submit({ preventDefault() {} });
  await flush();
  assert.equal(ui.calls[1].url, '/api/pet');
  assert.deepEqual(JSON.parse(ui.calls[1].options.body), { name });
  assert.equal(ui.get('pet-name').textContent, name);
  assert.equal(ui.get('hatch-form').hidden, true);
  assert.equal(ui.button('feed').disabled, false);
});

test('care controls honor sleep, cooldowns, and death; replacement can be cancelled', async () => {
  const ui = await fixture(data({ sleeping: true }));
  assert.equal(ui.button('feed').disabled, true);
  assert.equal(ui.button('sleep').disabled, false);
  assert.equal(ui.get('sleep-label').textContent, 'Wake up');
  ui.network.handler = async () => data({ sleeping: false }, 'Good morning!');
  await ui.button('sleep').listeners.click();
  assert.deepEqual(JSON.parse(ui.calls.at(-1).options.body), { action: 'sleep' });
  assert.equal(ui.button('feed').disabled, false);
  ui.network.handler = async () => data({ last_action: 1009 });
  await ui.run('poll()');
  assert.equal(ui.button('feed').disabled, true);
  assert.equal(ui.button('sleep').disabled, false);
  ui.network.handler = async () => data({ health: 0 });
  await ui.run('poll()');
  assert.equal(ui.get('farewell').hidden, false);
  assert.equal(ui.button('sleep').disabled, true);
  const callsBefore = ui.calls.length;
  ui.get('new-egg').listeners.click();
  assert.equal(ui.get('hatch-form').hidden, false);
  assert.equal(ui.get('cancel-hatch').hidden, false);
  ui.get('cancel-hatch').listeners.click();
  assert.equal(ui.get('hatch-form').hidden, true);
  assert.equal(ui.calls.length, callsBefore, 'Opening/cancelling a new egg must not change the saved pet');
});

test('disconnects disable care and recover on the next successful poll', async () => {
  const ui = await fixture(data());
  ui.network.handler = async () => { throw new Error('Connection refused'); };
  await ui.run('poll()');
  assert.equal(ui.get('connection').dataset.state, 'offline');
  assert.equal(ui.button('feed').disabled, true);
  ui.network.handler = async () => data();
  await ui.run('poll()');
  assert.equal(ui.get('connection').dataset.state, 'online');
  assert.equal(ui.button('feed').disabled, false);
  assert.match(ui.get('message').textContent, /connected again/);
});

test('a stale poll cannot overwrite a newer care response', async () => {
  const ui = await fixture(data());
  let resolvePoll;
  ui.network.handler = (url, options) => options.method === 'GET'
    ? new Promise((resolve) => { resolvePoll = resolve; })
    : Promise.resolve(data({ fullness: 100, last_action: 1010 }, 'Yum!'));
  const pending = ui.run('poll()');
  await ui.button('feed').listeners.click();
  assert.equal(ui.get('value-fullness').textContent, '100 / 100');
  resolvePoll(data({ fullness: 80 }));
  await pending;
  assert.equal(ui.get('value-fullness').textContent, '100 / 100');
  assert.equal(ui.get('message').textContent, 'Yum!');
});


test('renders server growth, bond levels, and unlocked milestones', async () => {
  const snapshot = data();
  snapshot.progression = {
    stage: 'Child', next_stage: 'Teen', seconds_until_growth: 5400,
    growth_percent: 70, bond_level: 2, bond_progress: 6, care_points: 46,
    achievements: [{ title: 'First little care', description: 'Share your first helpful care action.', unlocked: true },
      { title: 'All grown up', description: 'Raise your buddy to adulthood.', unlocked: false }],
  };
  const ui = await fixture(snapshot);
  assert.equal(ui.get('growth-panel').hidden, false);
  assert.equal(ui.get('growth-progress').value, 70);
  assert.match(ui.get('growth-time').textContent, /1h 30m/);
  assert.equal(ui.get('bond-level').textContent, 'Bond level 2');
  assert.equal(ui.get('bond-progress').value, 6);
  assert.equal(ui.get('achievements').children.length, 2);
  assert.equal(ui.get('achievements').children[0].dataset.unlocked, 'true');
  assert.equal(ui.get('achievements').children[1].dataset.unlocked, 'false');
  assert.equal(ui.get('habitat').dataset.stage, 'Child');
  snapshot.pet.health = 0;
  snapshot.progression.seconds_until_growth = null;
  ui.network.handler = async () => snapshot;
  await ui.run('poll()');
  assert.equal(ui.get('growth-time').textContent, 'A life to remember');
  ui.get('new-egg').listeners.click();
  assert.equal(ui.get('growth-panel').hidden, true);
  assert.equal(ui.get('achievements-section').hidden, true);
});
