'use strict';

// No dependencies, no build step. The backend remains the source of truth.
const $ = (id) => document.getElementById(id);
const actionButtons = [...document.querySelectorAll('[data-action]')];
const statNames = ['fullness', 'happiness', 'energy', 'hygiene', 'health'];
let snapshot = null;
let online = false;
let busy = false;
let replacing = false;
let previousEvents = '';
let generation = 0;
let polling = false;
let serverOffset = 0;
let summaryBusy = false;
let summaryKey = '';

function statusKey(pet) {
  return pet ? JSON.stringify([pet.name, pet.born, ...statNames.map((name) => pet[name]), pet.sleeping]) : '';
}

async function summarizeStatus() {
  if (summaryBusy || busy || !online || !snapshot?.pet || snapshot.pet.health <= 0 || replacing) return;
  summaryBusy = true;
  const key = summaryKey;
  $('pet-summary').hidden = true;
  $('summary-status').dataset.error = 'false';
  $('summary-status').textContent = 'Your buddy is thinking…';
  updateControls();
  try {
    const result = await request('/api/pet/summary', {}, 65000);
    if (key !== summaryKey) return;
    $('pet-summary').textContent = result.summary;
    $('pet-summary').hidden = false;
    $('summary-status').textContent = `Your buddy’s feelings · Ollama (${result.model})`;
  } catch (error) {
    if (key !== summaryKey) return;
    $('summary-status').dataset.error = 'true';
    $('summary-status').textContent = error.status ? error.message : 'Could not reach Ollama for a summary. Check the backend and Ollama, then try again.';
  } finally {
    summaryBusy = false;
    updateControls();
  }
}
let careAnimationTimer = null;

function stopCareAnimation() {
  clearTimeout(careAnimationTimer);
  careAnimationTimer = null;
  delete $('habitat').dataset.action;
}

function animateCare(action) {
  stopCareAnimation();
  // Restart even when the same action is used twice in a row.
  void $('character').offsetWidth;
  $('habitat').dataset.action = action;
  careAnimationTimer = setTimeout(stopCareAnimation, 2600);
}

function tell(message, error = false) {
  $('message').textContent = message;
  $('message').parentElement.dataset.error = String(error);
}

function connection(connected) {
  online = connected;
  $('connection').dataset.state = connected ? 'online' : 'offline';
  $('connection-label').textContent = connected ? 'Connected' : 'Reconnecting';
  $('save-note').textContent = connected ? 'Your little world, saved as you go.' : 'Offline. Trying to reconnect…';
}

async function request(path, body, timeout = 5000) {
  const response = await fetch(path, {
    method: body === undefined ? 'GET' : 'POST',
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    cache: 'no-store',
    signal: AbortSignal.timeout(timeout),
  });
  const data = await response.json();
  if (!response.ok) {
    const error = new Error(data.error || 'Something went wrong. Please try again.');
    error.status = response.status;
    throw error;
  }
  return data;
}

function stage(age) {
  if (age < 3600) return 'Baby';
  if (age < 21600) return 'Child';
  if (age < 86400) return 'Teen';
  return 'Adult';
}
function mood(pet) {
  if (pet.health <= 0) return ['Resting in the stars', 'dead'];
  if (pet.sleeping) return ['Sweet dreams', 'sleeping'];
  if (pet.health < 40) return ['Feeling poorly', 'poorly'];
  if (pet.fullness < 25) return ['A little hungry', 'hungry'];
  if (pet.energy < 25) return ['Getting sleepy', 'sleepy'];
  if (pet.hygiene < 25) return ['Bath time?', 'messy'];
  if (pet.happiness < 30) return ['Needs a friend', 'lonely'];
  return ['Happy & content', 'happy'];
}

function updateControls() {
  const pet = snapshot?.pet;
  const alive = pet && pet.health > 0;
  $('summary-button').disabled = summaryBusy || busy || !online || !alive || replacing;
  const time = Math.floor(Date.now() / 1000 + serverOffset);
  const cooldown = pet ? Math.max(0, 5 - (time - pet.last_action)) : 0;
  for (const button of actionButtons) {
    const sleep = button.dataset.action === 'sleep';
    button.disabled = busy || !online || !alive || replacing || (!sleep && (pet.sleeping || cooldown > 0));
    button.title = !online ? 'Waiting for a connection' : !alive ? 'Hatch a buddy to begin' :
      replacing ? 'Finish naming your buddy first' : !sleep && pet.sleeping ? 'Wake your buddy first' :
      !sleep && cooldown > 0 ? `Give your buddy ${cooldown} more seconds` : '';
  }
  $('hatch-button').disabled = busy || !online;
  $('pet-name-input').disabled = busy;
  $('new-egg').disabled = busy || !online;
  $('cancel-hatch').disabled = busy;
  $('sleep-label').textContent = pet?.sleeping ? 'Wake up' : 'Sleep';
  $('sleep-description').textContent = pet?.sleeping ? 'Hello, sunshine' : 'Sweet dreams';
  $('care-note').textContent = !online ? 'Waiting for your little world to reconnect.' : !pet ? 'Hatch your buddy to begin.' :
    !alive ? 'A new beginning, whenever you’re ready.' : pet.sleeping ? 'Shh… your buddy is getting some rest.' :
    cooldown > 0 ? `A little breather… ${cooldown}s` : 'A little love, one action at a time.';
}

function duration(seconds) {
  const minutes = Math.ceil(Math.max(0, seconds) / 60);
  return minutes >= 60 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${minutes}m`;
}

function renderProgression(progression, pet, nursery) {
  $('growth-panel').hidden = !progression || nursery;
  $('bond-panel').hidden = !progression || nursery;
  $('achievements-section').hidden = !progression || nursery;
  if (!progression || nursery) return;
  const alive = pet.health > 0;
  $('habitat').dataset.stage = progression.stage;
  $('growth-label').textContent = !alive ? `${progression.stage} · always remembered` :
    progression.next_stage ? `Growing into a ${progression.next_stage.toLowerCase()}` : 'All grown up, still your little buddy';
  $('growth-time').textContent = !alive ? 'A life to remember' : progression.seconds_until_growth === null ? 'Adult' : `${duration(progression.seconds_until_growth)} to go`;
  $('growth-progress').value = progression.growth_percent;
  $('growth-progress').setAttribute('aria-valuetext', !alive ? 'Growth has stopped' : progression.next_stage ? `${progression.growth_percent}% toward ${progression.next_stage}` : 'Fully grown');
  for (const label of document.querySelectorAll('.growth-stages span')) {
    label.dataset.current = String(label.textContent === progression.stage);
  }
  $('bond-level').textContent = `Bond level ${progression.bond_level}`;
  $('bond-points').textContent = `${progression.bond_progress} / 40 to next level`;
  $('bond-progress').value = progression.bond_progress;
  $('bond-description').textContent = `${progression.care_points} care points shared. Feed +4 · Play +6 · Clean +4 · Medicine +4.`;
  const unlocked = progression.achievements.filter((a) => a.unlocked).length;
  $('achievement-summary').textContent = `${unlocked} of ${progression.achievements.length} milestones shared`;
  const key = JSON.stringify(progression.achievements);
  if ($('achievements').dataset.key !== key) {
    $('achievements').replaceChildren();
    for (const achievement of progression.achievements) {
      const card = document.createElement('article');
      card.className = 'achievement';
      card.dataset.unlocked = String(achievement.unlocked);
      const symbol = document.createElement('span');
      symbol.className = 'achievement-symbol';
      symbol.textContent = achievement.unlocked ? '✧' : '·';
      symbol.setAttribute('aria-hidden', 'true');
      const copy = document.createElement('div');
      const title = document.createElement('h3');
      title.textContent = achievement.title;
      const description = document.createElement('p');
      description.textContent = achievement.description;
      const status = document.createElement('small');
      status.textContent = achievement.unlocked ? 'A moment to keep' : 'Still to discover';
      copy.append(title, description, status);
      card.append(symbol, copy);
      $('achievements').append(card);
    }
    $('achievements').dataset.key = key;
  }
}

function render(data) {
  snapshot = data;
  serverOffset = data.server_time - Date.now() / 1000;
  const pet = data.pet;
  const alive = pet && pet.health > 0;
  if (alive) replacing = false;
  const nursery = !pet || replacing;
  const nextSummaryKey = statusKey(pet);
  if (summaryKey !== nextSummaryKey || nursery) {
    summaryKey = nextSummaryKey;
    $('pet-summary').hidden = true;
    $('summary-status').dataset.error = 'false';
    $('summary-status').textContent = 'Ask your buddy how they feel about their current wellbeing.';
  }
  $('summary-section').hidden = !alive || nursery;
  if (nursery || !alive) stopCareAnimation();
  $('hatch-form').hidden = !nursery;
  $('farewell').hidden = !pet || alive || replacing;
  $('cancel-hatch').hidden = !replacing;
  $('life-stage').hidden = !pet || nursery;
  $('mood').hidden = !pet || nursery;
  $('pet-name').textContent = nursery ? 'A tiny hello, a new beginning.' : pet.name;
  if (nursery) {
    $('habitat').dataset.mood = 'nursery';
    $('habitat').setAttribute('aria-label', 'A spotted egg waiting to hatch in a peaceful garden');
    $('stage').textContent = 'A new beginning';
    $('pet-description').textContent = 'Someone small is waiting to meet you.';
    $('scene-caption').textContent = 'A whole friendship, tucked inside.';
    $('care-tip').textContent = 'Give your buddy a name. We’ll take it one little moment at a time.';
  } else {
    const age = Math.max(0, (alive ? data.server_time : pet.died_at ?? pet.last_tick) - pet.born);
    const lifeStage = data.progression?.stage ?? stage(age);
    const [feeling, state] = mood(pet);
    $('habitat').dataset.mood = state;
    $('habitat').setAttribute('aria-label', `${pet.name}, a little green cat: ${feeling.toLowerCase()}`);
    $('stage').textContent = `${lifeStage} days`;
    $('life-stage').textContent = lifeStage;
    $('pet-description').textContent = `${Math.floor(age / 3600)}h ${Math.floor(age % 3600 / 60)}m old · Your little companion`;
    $('mood').textContent = feeling;
    $('scene-caption').textContent = !alive ? 'A little friend, always remembered.' : pet.sleeping ? 'Dreaming of lovely little things.' : 'There’s no place like being with you.';
    $('care-tip').textContent = !alive ? 'Every friendship leaves a little warmth behind. A new egg is here when you’re ready.' :
      pet.sleeping ? 'Rest restores energy. Your buddy will still need food and a bath after their nap.' :
      pet.health < 40 ? 'A little medicine will help. Keep their tummy full and their fur clean, too.' :
      pet.fullness < 25 ? 'A little snack would make their day. Hungry buddies need a bit of extra care.' :
      pet.energy < 25 ? 'It’s been a big day for a little buddy. A nap will help them recharge.' :
      pet.hygiene < 25 ? 'A few bubbles and a little splash. Bath time will have them feeling fresh.' :
      pet.happiness < 30 ? 'A little time together means a lot. Try playing with your buddy.' :
      'A full tummy, a little play, and plenty of rest. You’re doing lovely.';
  }
  renderProgression(data.progression, pet, nursery);
  for (const name of statNames) {
    const value = pet ? Math.max(0, Math.min(100, pet[name])) : 0;
    $(`value-${name}`).textContent = pet ? `${value} / 100` : '—';
    $(`stat-${name}`).value = value;
    $(`stat-${name}`).setAttribute('aria-valuetext', pet ? `${value} out of 100` : 'No buddy yet');
    $(`stat-${name}`).closest('.stat').dataset.low = String(Boolean(pet && value < 25));
  }
  const eventsKey = JSON.stringify(data.events);
  if (eventsKey !== previousEvents) {
    const list = $('memories');
    list.replaceChildren();
    if (data.events.length === 0) {
      const empty = document.createElement('li');
      empty.className = 'empty-memory';
      empty.textContent = 'Your story is just beginning. The little moments will live here.';
      list.append(empty);
    }
    data.events.forEach((event, index) => {
      const li = document.createElement('li');
      const mark = document.createElement('span');
      mark.className = 'memory-mark';
      mark.textContent = '✧';
      mark.setAttribute('aria-hidden', 'true');
      const text = document.createElement('span');
      text.textContent = event;
      const number = document.createElement('span');
      number.className = 'memory-number';
      number.textContent = index === 0 ? 'LATEST' : '';
      li.append(mark, text, number);
      list.append(li);
    });
    if (data.events[0]) tell(data.events[0]);
    previousEvents = eventsKey;
  }
  if (data.message) tell(data.message);
  updateControls();
}

async function poll() {
  if (polling || busy || document.hidden) return;
  polling = true;
  const version = generation;
  try {
    const data = await request('/api/pet');
    // A GET started before a care request must not overwrite its newer response.
    if (version !== generation || busy) return;
    const reconnecting = !online;
    connection(true);
    if (reconnecting) tell(snapshot ? 'Welcome back. Your little world is connected again.' : data.pet ? 'A familiar face, a lovely little moment. Welcome home.' : 'Your little friendship starts with a name.');
    render(data);
  } catch (error) {
    if (version !== generation || busy) return;
    connection(false);
    tell('We can’t reach your buddy right now. Keep the backend running; we’ll reconnect automatically.', true);
    updateControls();
  } finally {
    polling = false;
  }
}

async function mutate(path, body) {
  if (busy || !online) return;
  busy = true;
  generation += 1;
  updateControls();
  try {
    const data = await request(path, body);
    connection(true);
    render(data);
    if (path === '/api/pet/actions' && data.pet?.health > 0) {
      animateCare(body.action === 'sleep' && !data.pet.sleeping ? 'wake' : body.action);
    }
  } catch (error) {
    if (error.status) {
      tell(error.message, true);
    } else {
      connection(false);
      tell('The connection paused. We’ll check your buddy again shortly. Your action hasn’t been retried.', true);
    }
  } finally {
    busy = false;
    updateControls();
  }
}

$('summary-button').addEventListener('click', summarizeStatus);

for (const button of actionButtons) {
  button.addEventListener('click', () => mutate('/api/pet/actions', { action: button.dataset.action }));
}
$('hatch-form').addEventListener('submit', (event) => {
  event.preventDefault();
  const name = $('pet-name-input').value.trim();
  if ([...name].length > 20 || /[\u0000-\u001f\u007f-\u009f]/u.test(name)) {
    tell('A little name, please: up to 20 characters, without control characters.', true);
    $('pet-name-input').focus();
    return;
  }
  mutate('/api/pet', { name });
});
$('new-egg').addEventListener('click', () => {
  replacing = true;
  $('pet-name-input').value = '';
  render(snapshot);
  $('pet-name-input').focus();
});
$('cancel-hatch').addEventListener('click', () => {
  replacing = false;
  render(snapshot);
  $('new-egg').focus();
});
document.addEventListener('visibilitychange', () => { if (!document.hidden) poll(); });
window.addEventListener('online', poll);
setInterval(poll, 2000);
setInterval(updateControls, 250);
poll();
